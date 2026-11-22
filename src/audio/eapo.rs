//! Finds Equalizer APO and writes PeriCore's settings into its config folder.
//!
//! PeriCore owns `pericore.txt` there. `config.txt` is replaced once (after a
//! backup) by a stub that includes it, so Equalizer APO's sample filters
//! don't stack on top of the user's choice. Equalizer APO watches that folder
//! and applies changes within a moment.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use winreg::enums::{HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_64KEY};
use winreg::RegKey;

pub const DOWNLOAD_URL: &str = "https://sourceforge.net/projects/equalizerapo/";
const OURS: &str = "pericore.txt";
const BACKUP: &str = "config-before-pericore.txt";
const INCLUDE: &str = "Include: pericore.txt";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    NotInstalled,
    /// Installed, but config.txt doesn't include PeriCore's file yet.
    NotLinked,
    Ready,
}

fn reg_path(name: &str) -> Option<PathBuf> {
    let key = RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey_with_flags(r"SOFTWARE\EqualizerAPO", KEY_READ | KEY_WOW64_64KEY)
        .ok()?;
    let v: String = key.get_value(name).ok()?;
    Some(PathBuf::from(v))
}

pub fn install_dir() -> Option<PathBuf> {
    reg_path("InstallPath").filter(|p| p.is_dir())
}

pub fn config_dir() -> Option<PathBuf> {
    reg_path("ConfigPath").or_else(|| install_dir().map(|d| d.join("config"))).filter(|p| p.is_dir())
}

fn linked(dir: &Path) -> bool {
    std::fs::read_to_string(dir.join("config.txt")).is_ok_and(|t| t.lines().any(|l| l.trim() == INCLUDE))
}

pub fn status() -> Status {
    match config_dir() {
        None => Status::NotInstalled,
        Some(d) if linked(&d) => Status::Ready,
        Some(_) => Status::NotLinked,
    }
}

fn write(path: &Path, text: &str) -> Result<()> {
    std::fs::write(path, text).with_context(|| {
        format!("can't write {} (Windows denied access to Equalizer APO's config folder)", path.display())
    })
}

/// Backs up config.txt and points it at PeriCore's file.
pub fn link() -> Result<()> {
    let dir = config_dir().context("Equalizer APO isn't installed")?;
    if linked(&dir) {
        return Ok(());
    }
    let config = dir.join("config.txt");
    let backup = dir.join(BACKUP);
    if config.exists() && !backup.exists() {
        std::fs::copy(&config, &backup).with_context(|| format!("backing up {}", config.display()))?;
    }
    if !dir.join(OURS).exists() {
        write(&dir.join(OURS), "# Written by PeriCore.\n")?;
    }
    write(
        &config,
        &format!("# Managed by PeriCore. Your earlier settings are saved in {BACKUP}.\n{INCLUDE}\n"),
    )
}

/// Replaces PeriCore's part of the configuration.
pub fn apply(text: &str) -> Result<()> {
    let dir = config_dir().context("Equalizer APO isn't installed")?;
    let ours = dir.join(OURS);
    if std::fs::read_to_string(&ours).is_ok_and(|t| t == text) {
        return Ok(());
    }
    write(&ours, text)
}

/// Opens Equalizer APO's Device Selector (it asks for admin rights itself).
pub fn open_configurator() {
    if let Some(dir) = install_dir() {
        crate::os::open_url(&dir.join("Configurator.exe").display().to_string());
    }
}
