//! "Sound preset" submenu in the tray: lists the saved presets and switches
//! between them without opening the window.

use crate::audio;
use anyhow::Result;
use tray_icon::menu::{CheckMenuItem, MenuId, MenuItem, Submenu};

pub struct SoundMenu {
    pub submenu: Submenu,
    items: Vec<(CheckMenuItem, String)>,
    placeholder: MenuItem,
    /// Names and active preset the menu currently shows.
    shown: (Vec<String>, Option<String>),
}

fn current() -> (Vec<String>, Option<String>) {
    let s = audio::load();
    (s.profiles.iter().map(|p| p.name.clone()).collect(), s.active_profile)
}

impl SoundMenu {
    pub fn new() -> Result<Self> {
        let mut m = Self {
            submenu: Submenu::new("Sound preset", true),
            items: Vec::new(),
            placeholder: MenuItem::new("No saved presets (open Sound)", false, None),
            shown: (Vec::new(), None),
        };
        m.submenu.append(&m.placeholder)?;
        m.refresh()?;
        Ok(m)
    }

    /// Rebuilds the items if the saved presets changed (e.g. in the window).
    pub fn refresh(&mut self) -> Result<()> {
        let now = current();
        let (names, active) = &now;
        if names != &self.shown.0 {
            for (item, _) in self.items.drain(..) {
                self.submenu.remove(&item)?;
            }
            if names.is_empty() {
                self.submenu.prepend(&self.placeholder)?;
            } else if self.shown.0.is_empty() {
                self.submenu.remove(&self.placeholder)?;
            }
            for name in names {
                let item = CheckMenuItem::new(name, true, false, None);
                self.submenu.append(&item)?;
                self.items.push((item, name.clone()));
            }
        }
        for (item, name) in &self.items {
            item.set_checked(active.as_deref() == Some(name.as_str()));
        }
        self.shown = now;
        Ok(())
    }

    /// Handles a click; returns true if it was one of this menu's items.
    pub fn handle(&mut self, id: &MenuId) -> bool {
        let Some(name) = self.items.iter().find(|(i, _)| i.id() == id).map(|(_, n)| n.clone()) else {
            return false;
        };
        let mut s = audio::load();
        if s.use_profile(&name) {
            if let Err(e) = audio::save_and_apply(&s) {
                crate::os::log_error(&format!("sound preset: {e:#}"));
            }
        }
        if let Err(e) = self.refresh() {
            crate::os::log_error(&format!("sound menu: {e:#}"));
        }
        true
    }
}
