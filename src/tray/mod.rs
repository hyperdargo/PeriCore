//! System-tray process: battery icon + menu. Very small and always running.

pub mod estimate;
pub mod icon;
pub mod monitor;
mod promote;
mod sound_menu;
pub mod widget;

use crate::os;
use anyhow::Result;
use icon::IconState;
use monitor::Reading;
use std::sync::mpsc;
use std::time::{Duration, Instant};
use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

struct Items {
    open: MenuItem,
    refresh: MenuItem,
    taskbar: CheckMenuItem,
    autostart: CheckMenuItem,
    sound: sound_menu::SoundMenu,
    quit: MenuItem,
}

fn build_menu() -> Result<(Menu, Items)> {
    let items = Items {
        open: MenuItem::new("Open settings", true, None),
        refresh: MenuItem::new("Refresh battery", true, None),
        taskbar: CheckMenuItem::new("Show battery on taskbar", true, taskbar_enabled(), None),
        autostart: CheckMenuItem::new("Start with Windows", true, os::autostart_enabled(), None),
        sound: sound_menu::SoundMenu::new()?,
        quit: MenuItem::new("Quit", true, None),
    };
    let menu = Menu::new();
    menu.append_items(&[
        &items.open,
        &items.refresh,
        &items.sound.submenu,
        &PredefinedMenuItem::separator(),
        &items.taskbar,
        &items.autostart,
        &PredefinedMenuItem::separator(),
        &items.quit,
    ])?;
    Ok((menu, items))
}

const TASKBAR_SETTING: &str = "taskbar-widget";

/// The taskbar widget is off unless the user turned it on (the tray icon
/// already shows the battery).
fn taskbar_enabled() -> bool {
    crate::store::load::<bool>(TASKBAR_SETTING).unwrap_or(false)
}

fn make_icon(state: IconState) -> Result<Icon> {
    Ok(Icon::from_rgba(icon::render(state), icon::SIZE as u32, icon::SIZE as u32)?)
}

fn state_of(r: &Reading, phase: u8) -> IconState {
    let device = match r.kind {
        crate::devices::Kind::Controller => icon::Device::Controller,
        crate::devices::Kind::Mouse | crate::devices::Kind::Receiver => icon::Device::Mouse,
        crate::devices::Kind::Keyboard => icon::Device::Unknown,
    };
    IconState { percent: r.percent, charging: r.charging(), full: r.full(), asleep: r.asleep, phase, device }
}

/// How often the charging animation advances.
const ANIM_STEP: Duration = Duration::from_millis(600);

/// One tray icon per device (or a single placeholder when none is found).
struct Icons {
    menu: Menu,
    shown: Vec<(String, TrayIcon)>,
}

impl Icons {
    fn new_icon(&self, state: IconState, tip: &str) -> Result<TrayIcon> {
        Ok(TrayIconBuilder::new()
            .with_menu(Box::new(self.menu.clone()))
            .with_menu_on_left_click(false)
            .with_tooltip(tip)
            .with_icon(make_icon(state)?)
            .build()?)
    }

    /// Makes the set of icons match `readings`, then redraws them.
    fn sync(&mut self, readings: &[Reading], phase: u8) -> Result<()> {
        let wanted: Vec<String> = if readings.is_empty() { vec![String::new()] } else { readings.iter().map(|r| r.key.clone()).collect() };
        self.shown.retain(|(k, _)| wanted.contains(k));
        for k in &wanted {
            if !self.shown.iter().any(|(s, _)| s == k) {
                let icon = self.new_icon(IconState::default(), "PeriCore")?;
                self.shown.push((k.clone(), icon));
            }
        }
        self.shown.sort_by_key(|(k, _)| wanted.iter().position(|w| w == k));
        if readings.is_empty() {
            let (_, t) = &self.shown[0];
            t.set_icon(Some(make_icon(IconState::default())?))?;
            t.set_tooltip(Some(monitor::tooltip(readings)))?;
            return Ok(());
        }
        for r in readings {
            if let Some((_, t)) = self.shown.iter().find(|(k, _)| *k == r.key) {
                t.set_icon(Some(make_icon(state_of(r, phase))?))?;
                t.set_tooltip(Some(format!("PeriCore - {}", monitor::line(r)).chars().take(127).collect::<String>()))?;
            }
        }
        Ok(())
    }

    /// Advances only the charging icons (cheap; runs a few times a second).
    fn animate(&self, readings: &[Reading], phase: u8) -> Result<()> {
        for r in readings.iter().filter(|r| r.charging() && !r.full()) {
            if let Some((_, t)) = self.shown.iter().find(|(k, _)| *k == r.key) {
                t.set_icon(Some(make_icon(state_of(r, phase))?))?;
            }
        }
        Ok(())
    }
}

/// Calls `f` once a second until it succeeds or `tries` run out.
fn retry<T>(tries: u32, mut f: impl FnMut() -> Result<T>) -> Result<T> {
    let mut last = None;
    for _ in 0..tries {
        match f() {
            Ok(v) => return Ok(v),
            Err(e) => last = Some(e),
        }
        os::pump_messages(1000);
    }
    Err(last.unwrap_or_else(|| anyhow::anyhow!("no attempts made")))
}

pub fn run() -> Result<()> {
    let Some(_guard) = os::SingleInstance::acquire("PeriCore.Tray") else {
        return Ok(());
    };
    let (menu, mut items) = build_menu()?;
    let mut icons = Icons { menu, shown: Vec::new() };
    // At login Explorer may not be ready yet: keep retrying for a minute.
    let first = retry(60, || icons.new_icon(IconState::default(), "PeriCore - reading battery..."))?;
    icons.shown.push((String::new(), first));

    let (tx, rx) = mpsc::channel::<Vec<Reading>>();
    let (wake_tx, wake_rx) = mpsc::channel::<()>();
    std::thread::spawn(move || monitor::run(tx, wake_rx));

    let (mut last, mut synced): (Vec<Reading>, bool) = (Vec::new(), false);
    let (mut phase, mut next_frame) = (0u8, Instant::now());
    let mut widget = if taskbar_enabled() { widget::Widget::new().ok() } else { None };
    let mut next_place = Instant::now();
    // Explorer adds a settings entry for each new icon shortly after it appears.
    let mut next_promote = Instant::now() + Duration::from_secs(2);
    loop {
        os::pump_messages(200);

        while let Ok(ev) = TrayIconEvent::receiver().try_recv() {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = ev {
                os::open_settings_window();
            }
        }
        while let Ok(ev) = MenuEvent::receiver().try_recv() {
            if items.sound.handle(&ev.id) {
                continue;
            }
            if ev.id == items.open.id() {
                os::open_settings_window();
            } else if ev.id == items.refresh.id() {
                let _ = wake_tx.send(());
            } else if ev.id == items.taskbar.id() {
                let on = items.taskbar.is_checked();
                let _ = crate::store::save(TASKBAR_SETTING, &on);
                if on {
                    widget = widget::Widget::new().ok();
                    if let Some(w) = &mut widget {
                        w.update(&last);
                    }
                } else if let Some(mut w) = widget.take() {
                    w.hide();
                }
            } else if ev.id == items.autostart.id() {
                let on = items.autostart.is_checked();
                if os::set_autostart(on).is_err() {
                    items.autostart.set_checked(!on);
                }
            } else if ev.id == items.quit.id() {
                return Ok(());
            }
        }
        while let Ok(readings) = rx.try_recv() {
            if !synced || readings != last {
                synced = true;
                if let Err(e) = icons.sync(&readings, phase) {
                    os::log_error(&format!("tray update: {e:#}"));
                }
                if let Some(w) = &mut widget {
                    w.update(&readings);
                }
                last = readings;
            }
        }
        if let Some(w) = &mut widget {
            if w.take_click() {
                os::open_settings_window();
            }
            // Keep it in place and above the taskbar (which rises when clicked).
            if Instant::now() >= next_place {
                next_place = Instant::now() + Duration::from_secs(1);
                w.redraw();
            }
        }
        if Instant::now() >= next_promote {
            next_promote = Instant::now() + Duration::from_secs(10);
            promote::promote();
            if let Err(e) = items.sound.refresh() {
                os::log_error(&format!("sound menu: {e:#}"));
            }
        }
        if Instant::now() >= next_frame {
            next_frame = Instant::now() + ANIM_STEP;
            if last.iter().any(|r| r.charging() && !r.full()) {
                phase = (phase + 1) % icon::CHARGE_PHASES;
                if let Err(e) = icons.animate(&last, phase) {
                    os::log_error(&format!("tray animation: {e:#}"));
                }
            }
        }
    }
}
