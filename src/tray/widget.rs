//! Taskbar widget: "🖱 93%  🎮 30%" drawn on the taskbar just left of the
//! notification area, using the Windows icon font. It is a small layered,
//! always-on-top window repositioned over the taskbar; clicking it opens
//! the dashboard.

use super::monitor::Reading;
use crate::devices::Kind;
use anyhow::{anyhow, Result};
use std::sync::atomic::{AtomicBool, Ordering};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, CreateFontW, DeleteDC, DeleteObject, GetDC, GetTextExtentPoint32W, ReleaseDC,
    SelectObject, SetBkMode, SetTextColor, TextOutW, AC_SRC_ALPHA, AC_SRC_OVER, BITMAPINFO, BITMAPINFOHEADER,
    BLENDFUNCTION, DIB_RGB_COLORS, HDC, HFONT,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};
use windows_sys::Win32::UI::HiDpi::{GetDpiForWindow, SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, FindWindowExW, FindWindowW, GetWindowRect, RegisterClassExW, SetWindowPos,
    ShowWindow, UpdateLayeredWindow, HWND_TOPMOST, IDC_HAND, LoadCursorW, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
    SW_HIDE, SW_SHOWNOACTIVATE, ULW_ALPHA, WM_LBUTTONUP, WNDCLASSEXW, WS_EX_LAYERED, WS_EX_NOACTIVATE,
    WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};

// Segoe Fluent Icons / Segoe MDL2 Assets code points.
const ICON_MOUSE: char = '\u{E962}';
const ICON_PAD: char = '\u{E7FC}';
const ICON_KEYBOARD: char = '\u{E765}';
const ICON_BOLT: char = '\u{E945}';
const ICON_CHECK: char = '\u{E73E}';

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Font {
    Icon,
    Text,
}

/// Colour roles; resolved against the taskbar theme when drawing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Normal,
    Low,
    Charge,
    Full,
    Dim,
}

/// One run of text in the widget.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Seg {
    pub text: String,
    pub font: Font,
    pub tone: Tone,
    /// Extra space before this run, in 1/4 of the font height.
    pub gap: u8,
}

fn seg(text: impl Into<String>, font: Font, tone: Tone, gap: u8) -> Seg {
    Seg { text: text.into(), font, tone, gap }
}

/// Builds the widget's runs: icon, percentage, then a charge/full mark.
pub fn layout(readings: &[Reading]) -> Vec<Seg> {
    let mut out = Vec::new();
    for (i, r) in readings.iter().filter(|r| r.percent.is_some()).enumerate() {
        let p = r.percent.unwrap_or(0);
        let icon = match r.kind {
            Kind::Controller => ICON_PAD,
            Kind::Keyboard => ICON_KEYBOARD,
            Kind::Mouse | Kind::Receiver => ICON_MOUSE,
        };
        let base = if r.asleep { Tone::Dim } else { Tone::Normal };
        let level = if r.asleep {
            Tone::Dim
        } else if p <= 15 && !r.charging() {
            Tone::Low
        } else {
            Tone::Normal
        };
        out.push(seg(icon.to_string(), Font::Icon, base, if i == 0 { 0 } else { 4 }));
        out.push(seg(format!("{p}%"), Font::Text, level, 1));
        if r.full() {
            out.push(seg(ICON_CHECK.to_string(), Font::Icon, Tone::Full, 1));
        } else if r.charging() {
            out.push(seg(ICON_BOLT.to_string(), Font::Icon, Tone::Charge, 1));
        }
    }
    out
}

/// BGR colour for a tone on a dark or light taskbar.
fn color(tone: Tone, light: bool) -> [u8; 3] {
    match (tone, light) {
        (Tone::Normal, false) => [255, 255, 255],
        (Tone::Normal, true) => [0, 0, 0],
        (Tone::Low, _) => [60, 76, 231],
        (Tone::Charge, false) => [255, 200, 60],
        (Tone::Charge, true) => [200, 120, 0],
        (Tone::Full, _) => [113, 204, 46],
        (Tone::Dim, false) => [150, 150, 150],
        (Tone::Dim, true) => [110, 110, 110],
    }
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// True when Windows uses the light taskbar theme.
fn light_taskbar() -> bool {
    let key = wide(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize");
    let name = wide("SystemUsesLightTheme");
    let mut v: u32 = 0;
    let mut len = 4u32;
    // SAFETY: valid NUL-terminated strings and a 4-byte out buffer.
    let rc = unsafe {
        RegGetValueW(HKEY_CURRENT_USER, key.as_ptr(), name.as_ptr(), RRF_RT_REG_DWORD, std::ptr::null_mut(), (&mut v as *mut u32).cast(), &mut len)
    };
    rc == 0 && v == 1
}

static CLICKED: AtomicBool = AtomicBool::new(false);

unsafe extern "system" fn wndproc(h: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    if msg == WM_LBUTTONUP {
        CLICKED.store(true, Ordering::Relaxed);
        return 0;
    }
    // SAFETY: forwarding the unmodified message to the default handler.
    unsafe { DefWindowProcW(h, msg, w, l) }
}

pub struct Widget {
    hwnd: HWND,
    segs: Vec<Seg>,
    visible: bool,
}

/// Where to put the widget: right-aligned to the left edge of the tray area,
/// filling the taskbar's height. None if the taskbar isn't horizontal or found.
fn anchor() -> Option<RECT> {
    let cls = wide("Shell_TrayWnd");
    let notify = wide("TrayNotifyWnd");
    // SAFETY: plain window lookups with valid class-name strings.
    unsafe {
        let bar = FindWindowW(cls.as_ptr(), std::ptr::null());
        if bar.is_null() {
            return None;
        }
        let mut rb: RECT = std::mem::zeroed();
        GetWindowRect(bar, &mut rb);
        if rb.right - rb.left < rb.bottom - rb.top {
            return None; // vertical taskbar
        }
        let tray = FindWindowExW(bar, std::ptr::null_mut(), notify.as_ptr(), std::ptr::null());
        let mut rt: RECT = std::mem::zeroed();
        if tray.is_null() || GetWindowRect(tray, &mut rt) == 0 {
            return None;
        }
        Some(RECT { left: rb.left, top: rb.top, right: rt.left, bottom: rb.bottom })
    }
}

impl Widget {
    pub fn new() -> Result<Self> {
        // Crisp text at any scaling; fails harmlessly if already set.
        // SAFETY: process-wide setting with a valid predefined context.
        unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
        let class = wide("PeriCoreTaskbarWidget");
        // SAFETY: registers a window class and creates a hidden popup owned by this thread.
        let hwnd = unsafe {
            let inst = GetModuleHandleW(std::ptr::null());
            let wc = WNDCLASSEXW {
                cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                lpfnWndProc: Some(wndproc),
                hInstance: inst,
                lpszClassName: class.as_ptr(),
                hCursor: LoadCursorW(std::ptr::null_mut(), IDC_HAND),
                ..std::mem::zeroed()
            };
            RegisterClassExW(&wc);
            CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOACTIVATE,
                class.as_ptr(),
                class.as_ptr(),
                WS_POPUP,
                0,
                0,
                1,
                1,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                inst,
                std::ptr::null(),
            )
        };
        if hwnd.is_null() {
            return Err(anyhow!("could not create the taskbar widget window"));
        }
        Ok(Self { hwnd, segs: Vec::new(), visible: false })
    }

    /// True once after the user clicked the widget.
    pub fn take_click(&self) -> bool {
        CLICKED.swap(false, Ordering::Relaxed)
    }

    pub fn update(&mut self, readings: &[Reading]) {
        self.segs = layout(readings);
        self.redraw();
    }

    pub fn hide(&mut self) {
        // SAFETY: hwnd is our own window.
        unsafe { ShowWindow(self.hwnd, SW_HIDE) };
        self.visible = false;
    }

    /// Re-places and repaints (also brings it back above the taskbar, which
    /// rises to the top whenever it's clicked). Call about once a second.
    pub fn redraw(&mut self) {
        let Some(area) = anchor().filter(|_| !self.segs.is_empty()) else {
            self.hide();
            return;
        };
        if let Err(e) = self.paint(area) {
            if std::env::var_os("PERICORE_DEBUG").is_some() {
                eprintln!("taskbar widget: {e:#}");
            }
            self.hide();
            return;
        }
        // SAFETY: hwnd is our own window.
        unsafe {
            if !self.visible {
                ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
                self.visible = true;
            }
            SetWindowPos(self.hwnd, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
        }
    }

    /// Renders the runs into a 32-bit bitmap and pushes it with per-pixel alpha.
    fn paint(&self, area: RECT) -> Result<()> {
        // SAFETY: hwnd is ours. All GDI objects are released before returning.
        unsafe {
            let dpi = GetDpiForWindow(self.hwnd).max(96) as i32;
            let px = |v: i32| v * dpi / 96;
            let height = area.bottom - area.top;
            let screen = GetDC(std::ptr::null_mut());
            let dc = CreateCompatibleDC(screen);
            let text_font = make_font("Segoe UI Variable Text", px(15), 500);
            let icon_font = make_font("Segoe Fluent Icons", px(16), 400);
            let icon_fallback = make_font("Segoe MDL2 Assets", px(16), 400);
            let icon_font = if icon_font.is_null() { icon_fallback } else { icon_font };

            // Measure.
            let unit = px(4);
            let mut runs: Vec<(i32, i32, &Seg, HFONT)> = Vec::new();
            let mut x = px(8);
            for s in &self.segs {
                let f = if s.font == Font::Icon { icon_font } else { text_font };
                SelectObject(dc, f);
                let w = wide(&s.text);
                let mut sz: SIZE = std::mem::zeroed();
                GetTextExtentPoint32W(dc, w.as_ptr(), (w.len() - 1) as i32, &mut sz);
                x += i32::from(s.gap) * unit / 4 * 2;
                runs.push((x, sz.cy, s, f));
                x += sz.cx;
            }
            let width = x + px(8);

            // Draw white-on-black, then turn brightness into alpha per run colour.
            let bmi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: width,
                    biHeight: -height,
                    biPlanes: 1,
                    biBitCount: 32,
                    ..std::mem::zeroed()
                },
                ..std::mem::zeroed()
            };
            let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
            let bmp = CreateDIBSection(dc, &bmi, DIB_RGB_COLORS, &mut bits, std::ptr::null_mut(), 0);
            if bmp.is_null() || bits.is_null() {
                cleanup(screen, dc, &[text_font, icon_font, icon_fallback]);
                return Err(anyhow!("CreateDIBSection failed"));
            }
            let old = SelectObject(dc, bmp);
            SetBkMode(dc, 1); // transparent
            SetTextColor(dc, 0x00FF_FFFF);
            for (rx, cy, s, f) in &runs {
                SelectObject(dc, *f);
                let w = wide(&s.text);
                TextOutW(dc, *rx, (height - cy) / 2, w.as_ptr(), (w.len() - 1) as i32);
            }
            let pixels = std::slice::from_raw_parts_mut(bits.cast::<u8>(), (width * height * 4) as usize);
            let light = light_taskbar();
            colorize(pixels, width, &runs.iter().map(|(rx, _, s, _)| (*rx, color(s.tone, light))).collect::<Vec<_>>());

            let size = SIZE { cx: width, cy: height };
            let src = POINT { x: 0, y: 0 };
            let dst = POINT { x: area.right - width - px(4), y: area.top };
            let blend = BLENDFUNCTION { BlendOp: AC_SRC_OVER as u8, BlendFlags: 0, SourceConstantAlpha: 255, AlphaFormat: AC_SRC_ALPHA as u8 };
            let ok = UpdateLayeredWindow(self.hwnd, screen, &dst, &size, dc, &src, 0, &blend, ULW_ALPHA);

            SelectObject(dc, old);
            DeleteObject(bmp);
            cleanup(screen, dc, &[text_font, icon_font, icon_fallback]);
            if ok == 0 {
                return Err(anyhow!("UpdateLayeredWindow failed"));
            }
        }
        Ok(())
    }
}

/// Converts white-on-black text into premultiplied coloured pixels. Each
/// run's colour applies from its x position to the next run. Background
/// keeps alpha 1 so the whole widget stays clickable.
fn colorize(pixels: &mut [u8], width: i32, runs: &[(i32, [u8; 3])]) {
    for (i, px) in pixels.as_chunks_mut::<4>().0.iter_mut().enumerate() {
        let x = i as i32 % width;
        let c = runs.iter().rev().find(|(rx, _)| x >= *rx).or(runs.first()).map_or([255, 255, 255], |r| r.1);
        let a = u16::from(px[0].max(px[1]).max(px[2])).max(1);
        for k in 0..3 {
            px[k] = (u16::from(c[k]) * a / 255) as u8;
        }
        px[3] = a as u8;
    }
}

unsafe fn make_font(face: &str, height: i32, weight: i32) -> HFONT {
    let f = wide(face);
    // SAFETY: valid NUL-terminated face name; 4 = ANTIALIASED_QUALITY (no ClearType, so alpha works).
    unsafe { CreateFontW(-height, 0, 0, 0, weight, 0, 0, 0, 1, 0, 0, 4, 0, f.as_ptr()) }
}

unsafe fn cleanup(screen: HDC, dc: HDC, fonts: &[HFONT]) {
    // SAFETY: objects created by `paint`; deleting a null handle is a no-op.
    unsafe {
        for f in fonts {
            if !f.is_null() {
                DeleteObject(*f);
            }
        }
        DeleteDC(dc);
        ReleaseDC(std::ptr::null_mut(), screen);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::devices::ry::protocol::ChargeState;

    fn r(kind: Kind, percent: Option<u8>, state: ChargeState, asleep: bool) -> Reading {
        Reading { key: "k".into(), name: "n".into(), kind, percent, state, asleep, minutes_left: None }
    }

    #[test]
    fn layout_icons_percent_and_marks() {
        let segs = layout(&[
            r(Kind::Receiver, Some(93), ChargeState::Discharging, false),
            r(Kind::Controller, Some(30), ChargeState::Charging, false),
            r(Kind::Controller, None, ChargeState::Discharging, false),
        ]);
        let text: Vec<&str> = segs.iter().map(|s| s.text.as_str()).collect();
        assert_eq!(text, vec!["\u{E962}", "93%", "\u{E7FC}", "30%", "\u{E945}"]);
        assert_eq!(segs[2].gap, 4, "space between devices");
        assert_eq!(segs[4].tone, Tone::Charge);
    }

    #[test]
    fn low_full_and_asleep_tones() {
        let low = layout(&[r(Kind::Controller, Some(5), ChargeState::Discharging, false)]);
        assert_eq!(low[1].tone, Tone::Low);
        let full = layout(&[r(Kind::Controller, Some(100), ChargeState::Full, false)]);
        assert_eq!(full[2].text, "\u{E73E}");
        let asleep = layout(&[r(Kind::Receiver, Some(48), ChargeState::Discharging, true)]);
        assert!(asleep.iter().all(|s| s.tone == Tone::Dim));
    }

    #[test]
    fn colorize_premultiplies_and_keeps_background_clickable() {
        // Two pixels: background (black) then full-white text in a red run.
        let mut px = vec![0, 0, 0, 0, 255, 255, 255, 0];
        colorize(&mut px, 2, &[(1, [0, 0, 200])]);
        assert_eq!(px[3], 1, "background alpha 1");
        assert_eq!(&px[4..8], &[0, 0, 200, 255]);
    }
}
