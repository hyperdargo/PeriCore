//! Dock screen tab: upload a GIF or picture to the APEX receiver's 128x128
//! screen and sync its clock.

use super::theme;
use crate::devices::ry::screen::{self, Animation, ScreenLink};
use crate::os;
use eframe::egui::{self, ColorImage, RichText, TextureHandle, TextureOptions};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;

struct Job {
    progress: Arc<AtomicUsize>,
    cancel: Arc<AtomicBool>,
    total: usize,
    handle: JoinHandle<(ScreenLink, anyhow::Result<()>)>,
}

pub struct DockTab {
    link: Option<ScreenLink>,
    version: Option<(u16, u16)>,
    file: Option<PathBuf>,
    anim: Option<Animation>,
    textures: Vec<TextureHandle>,
    layer: u8,
    job: Option<Job>,
    vendor_running: bool,
    last_check: f64,
    status: Option<Result<String, String>>,
}

impl DockTab {
    pub fn new(link: Option<ScreenLink>) -> Self {
        let version = link.as_ref().and_then(ScreenLink::version);
        Self { link, version, file: None, anim: None, textures: Vec::new(), layer: 0, job: None, vendor_running: false, last_check: -10.0, status: None }
    }

    pub fn show(&mut self, ui: &mut egui::Ui) {
        self.poll_job(ui.ctx());
        let dropped = ui.ctx().input(|i| i.raw.dropped_files.first().and_then(|f| f.path.clone()));
        if let Some(p) = dropped {
            self.load(&p);
        }

        theme::card(ui, "Dock screen · 128 × 128", |ui| {
            if self.link.is_none() && self.job.is_none() {
                ui.colored_label(theme::BAD, "Couldn't open the receiver for screen access.");
                return;
            }
            ui.horizontal(|ui| {
                self.preview(ui);
                ui.add_space(12.0);
                ui.vertical(|ui| {
                    let busy = self.job.is_some();
                    if ui.add_enabled(!busy, egui::Button::new("📂  Choose GIF or picture…")).clicked() {
                        if let Some(p) = os::pick_image_file() {
                            self.load(&p);
                        }
                    }
                    ui.label(RichText::new("…or drag a file onto this window.").color(theme::MUTED));
                    if let (Some(f), Some(a)) = (&self.file, &self.anim) {
                        let name = f.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned());
                        let kind = if a.frames.len() > 1 {
                            format!("{} frames · {} ms per frame", a.frames.len(), a.delay_ms)
                        } else {
                            "still picture".into()
                        };
                        ui.label(RichText::new(name).strong());
                        ui.label(RichText::new(kind).color(theme::MUTED));
                        if a.frames.len() == 1 {
                            ui.horizontal(|ui| {
                                ui.label("Slot");
                                for l in 0..5u8 {
                                    ui.selectable_value(&mut self.layer, l, format!("{}", l + 1));
                                }
                            });
                        }
                    }
                });
            });
            ui.add_space(8.0);
            self.controls(ui);
        });

        theme::card(ui, "Clock", |ui| {
            ui.label(RichText::new("The dock shows the time; sync it with this PC's clock.").color(theme::MUTED));
            let can = self.link.is_some();
            if ui.add_enabled(can, egui::Button::new("🕑  Sync time now")).clicked() {
                if let Some(link) = &self.link {
                    self.status = Some(
                        link.sync_time(os::local_time()).map(|_| "Clock synced".into()).map_err(|e| format!("{e:#}")),
                    );
                }
            }
            if let Some((oled, flash)) = self.version {
                ui.label(RichText::new(format!("Screen firmware {oled} · flash {flash}")).size(12.0).color(theme::MUTED));
            }
        });
        super::status_line(ui, &self.status);
    }

    fn controls(&mut self, ui: &mut egui::Ui) {
        if let Some(job) = &self.job {
            let done = job.progress.load(Ordering::Relaxed);
            let frac = done as f32 / job.total.max(1) as f32;
            ui.add(egui::ProgressBar::new(frac).show_percentage().animate(true));
            ui.horizontal(|ui| {
                ui.label(RichText::new("Uploading… keep the dock connected.").color(theme::MUTED));
                if ui.button("Cancel").clicked() {
                    job.cancel.store(true, Ordering::Relaxed);
                }
            });
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
            return;
        }
        let now = ui.input(|i| i.time);
        if now - self.last_check > 3.0 {
            self.vendor_running = os::vendor_app_running();
            self.last_check = now;
        }
        if self.vendor_running {
            ui.horizontal(|ui| {
                ui.colored_label(theme::WARN, "⚠  The AJAZZ app is running and will break the upload.");
                if ui.button("Close AJAZZ app").clicked() {
                    os::close_vendor_apps();
                    self.vendor_running = os::vendor_app_running();
                }
            });
        }
        let ready = self.anim.is_some() && self.link.is_some() && !self.vendor_running;
        if ui.add_enabled(ready, egui::Button::new(RichText::new("⬆  Upload to dock").strong())).clicked() {
            self.start_upload();
        }
        ui.label(
            RichText::new("A full-screen frame takes about 3 s, so long GIFs take a few minutes.").size(12.0).color(theme::MUTED),
        );
    }

    fn preview(&mut self, ui: &mut egui::Ui) {
        let size = egui::vec2(160.0, 160.0);
        if self.textures.is_empty() {
            let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
            ui.painter().rect_filled(rect, egui::Rounding::same(10.0), egui::Color32::BLACK);
            ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, "No image", egui::FontId::proportional(13.0), theme::MUTED);
            return;
        }
        let delay = self.anim.as_ref().map_or(100, |a| a.delay_ms.max(20)) as f64 / 1000.0;
        let t = ui.input(|i| i.time);
        let idx = ((t / delay) as usize) % self.textures.len();
        ui.add(egui::Image::new(&self.textures[idx]).fit_to_exact_size(size).rounding(10.0));
        if self.textures.len() > 1 {
            ui.ctx().request_repaint_after(std::time::Duration::from_secs_f64(delay));
        }
    }

    fn load(&mut self, path: &Path) {
        match screen::load(path) {
            Ok(a) => {
                self.textures.clear();
                self.file = Some(path.to_path_buf());
                self.anim = Some(a);
                self.status = None;
            }
            Err(e) => self.status = Some(Err(format!("{e:#}"))),
        }
    }

    fn ensure_textures(&mut self, ctx: &egui::Context) {
        if !self.textures.is_empty() {
            return;
        }
        if let Some(a) = &self.anim {
            self.textures = a
                .previews
                .iter()
                .enumerate()
                .map(|(i, img)| {
                    let ci = ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw());
                    ctx.load_texture(format!("dock{i}"), ci, TextureOptions::NEAREST)
                })
                .collect();
        }
    }

    fn start_upload(&mut self) {
        let (Some(link), Some(anim)) = (self.link.take(), self.anim.clone()) else { return };
        let progress = Arc::new(AtomicUsize::new(0));
        let cancel = Arc::new(AtomicBool::new(false));
        let total = anim.frames.len() * screen::CHUNKS_PER_FRAME;
        let (p, c, layer) = (progress.clone(), cancel.clone(), self.layer);
        let handle = std::thread::spawn(move || {
            let _busy = os::SingleInstance::acquire(os::SCREEN_BUSY);
            let res = link.upload(&anim, layer, &p, &c);
            (link, res)
        });
        self.status = None;
        self.job = Some(Job { progress, cancel, total, handle });
    }

    fn poll_job(&mut self, ctx: &egui::Context) {
        self.ensure_textures(ctx);
        if self.job.as_ref().is_some_and(|j| j.handle.is_finished()) {
            let job = self.job.take().expect("checked above");
            match job.handle.join() {
                Ok((link, res)) => {
                    self.link = Some(link);
                    self.status = Some(res.map(|_| "Uploaded to the dock screen".into()).map_err(|e| format!("{e:#}")));
                }
                Err(_) => self.status = Some(Err("upload thread crashed".into())),
            }
        }
    }
}
