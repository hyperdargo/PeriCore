//! Small JSON settings files under %APPDATA%\PeriCore, for state the devices
//! can't report back (e.g. the keyboard keymap and per-key colours).

use serde::{de::DeserializeOwned, Serialize};
use std::path::PathBuf;

fn path(name: &str) -> Option<PathBuf> {
    let dir = PathBuf::from(std::env::var_os("APPDATA")?).join("PeriCore");
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir.join(format!("{name}.json")))
}

/// Loads a saved value, or `None` if missing or unreadable.
pub fn load<T: DeserializeOwned>(name: &str) -> Option<T> {
    let text = std::fs::read_to_string(path(name)?).ok()?;
    serde_json::from_str(&text).ok()
}

pub fn save<T: Serialize>(name: &str, value: &T) -> anyhow::Result<()> {
    let p = path(name).ok_or_else(|| anyhow::anyhow!("APPDATA is not set"))?;
    let tmp = p.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(value)?)?;
    std::fs::rename(tmp, p)?;
    Ok(())
}
