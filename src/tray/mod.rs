//! System-tray process: battery icon + menu. Very small and always running.

pub mod icon;
pub mod monitor;

use crate::os;
use anyhow::Result;
use icon::IconState;
use monitor::Reading;
use std::sync::mpsc;
use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

struct Items {
    open: MenuItem,
    refresh: MenuItem,
    autostart: CheckMenuItem,
    quit: MenuItem,
}

fn build_menu() -> Result<(Menu, Items)> {
    let items = Items {
        open: MenuItem::new("Open settings", true, None),
        refresh: MenuItem::new("Refresh battery", true, None),
        autostart: CheckMenuItem::new("Start with Windows", true, os::autostart_enabled(), None),
        quit: MenuItem::new("Quit", true, None),
    };
    let menu = Menu::new();
    menu.append_items(&[
        &items.open,
        &items.refresh,
        &PredefinedMenuItem::separator(),
        &items.autostart,
        &PredefinedMenuItem::separator(),
        &items.quit,
    ])?;
    Ok((menu, items))
}

fn make_icon(state: IconState) -> Result<Icon> {
    Ok(Icon::from_rgba(icon::render(state), icon::SIZE as u32, icon::SIZE as u32)?)
}

fn primary_state(readings: &[Reading]) -> IconState {
    readings
        .iter()
        .find(|r| r.percent.is_some())
        .or(readings.first())
        .map_or(IconState { percent: None, charging: false }, |r| IconState { percent: r.percent, charging: r.charging })
}

pub fn run() -> Result<()> {
    let Some(_guard) = os::SingleInstance::acquire("PeriCore.Tray") else {
        return Ok(());
    };
    let (menu, items) = build_menu()?;
    let tray: TrayIcon = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_menu_on_left_click(false)
        .with_tooltip("PeriCore - reading battery...")
        .with_icon(make_icon(IconState { percent: None, charging: false })?)
        .build()?;

    let (tx, rx) = mpsc::channel::<Vec<Reading>>();
    let (wake_tx, wake_rx) = mpsc::channel::<()>();
    std::thread::spawn(move || monitor::run(tx, wake_rx));

    let mut last: Option<Vec<Reading>> = None;
    loop {
        os::pump_messages(200);

        while let Ok(ev) = TrayIconEvent::receiver().try_recv() {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = ev {
                os::open_settings_window();
            }
        }
        while let Ok(ev) = MenuEvent::receiver().try_recv() {
            if ev.id == items.open.id() {
                os::open_settings_window();
            } else if ev.id == items.refresh.id() {
                let _ = wake_tx.send(());
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
            if last.as_ref() != Some(&readings) {
                tray.set_icon(Some(make_icon(primary_state(&readings))?))?;
                tray.set_tooltip(Some(monitor::tooltip(&readings)))?;
                last = Some(readings);
            }
        }
    }
}
