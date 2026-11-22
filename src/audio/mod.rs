//! System-wide sound presets and mic noise suppression via Equalizer APO.

pub mod config;
pub mod devices;
pub mod eapo;
pub mod response;

use config::Sound;

const STORE: &str = "sound";

pub fn load() -> Sound {
    crate::store::load(STORE).unwrap_or_default()
}

/// Saves the settings and, when Equalizer APO is linked, applies them.
pub fn save_and_apply(s: &Sound) -> anyhow::Result<()> {
    crate::store::save(STORE, s)?;
    if eapo::status() == eapo::Status::Ready {
        eapo::apply(&config::render(s, &devices::list(false)))?;
    }
    Ok(())
}
