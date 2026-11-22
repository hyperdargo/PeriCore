//! Turns Equalizer APO on or off for one audio endpoint, the way its own
//! Device Selector does (which crashes on some Windows 11 builds).
//!
//! Windows loads an endpoint's effects from the CLSIDs in its
//! `MMDevices\Audio\{Render|Capture}\{id}\FxProperties` key. Equalizer APO's
//! pre-mix class goes into the stream slot (SFX or LFX) and its post-mix class
//! into the mode/endpoint slot (MFX, EFX or GFX); its DLL then reads
//! `HKLM\SOFTWARE\EqualizerAPO\Child APOs\{id}` for the driver's own effects
//! to chain and for the original values (restored when turned off).
//!
//! Endpoints that already carry the driver's own effects are refused, so we
//! never have to chain or remove them. Needs admin rights; Windows applies the
//! change after a restart.

use anyhow::{bail, Context, Result};
use winreg::enums::{HKEY_LOCAL_MACHINE, KEY_READ, KEY_SET_VALUE, KEY_WOW64_64KEY};
use winreg::RegKey;

pub const PRE_MIX: &str = "{EACD2258-FCAC-4FF4-B36D-419E924A6D79}";
pub const POST_MIX: &str = "{EC1CC9CE-FAED-4822-828A-82A81A6F018F}";
const ENDPOINTS: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\MMDevices\Audio";
const CHILD_APOS: &str = r"SOFTWARE\EqualizerAPO\Child APOs";
const NULL_CLSID: &str = "{00000000-0000-0000-0000-000000000000}";
/// AUDIO_SIGNALPROCESSINGMODE_DEFAULT, listed as a slot's supported mode.
const DEFAULT_MODE: &str = "{C18E2F7E-933D-4965-B7D1-1EEF228D2AF3}";
/// PKEY_DeviceInterface "combined device" flag (Bluetooth on Windows 11).
const COMBINED: &str = "{b3f8fa53-0004-438e-9003-51a46e139bfc},41";
/// Equalizer APO's own record layout version.
const RECORD_VERSION: &str = "2";
/// Our note of which mode-list values we added, so "off" can remove them.
const ADDED_MODES: &str = "PeriCoreAddedModes";

fn fx(n: u8) -> String {
    format!("{{d04e05a6-594b-4fb6-a80d-01af5eed7d1d}},{n}")
}
fn modes(n: u8) -> String {
    format!("{{d3993a3f-99c2-4402-b5ec-a92a0367664b}},{n}")
}

const LFX: u8 = 1;
const GFX: u8 = 2;
const SFX: u8 = 5;
const MFX: u8 = 6;
const EFX: u8 = 7;
const SLOTS: [u8; 5] = [LFX, GFX, SFX, MFX, EFX];
/// Multi-effect lists (13/14/15) also mean "uses the newer SFX/MFX/EFX model".
const MULTI: [u8; 3] = [13, 14, 15];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    LfxGfx,
    SfxMfx,
    SfxEfx,
}

/// What an endpoint's FxProperties held before we touched it.
#[derive(Debug, Default, Clone)]
pub struct Before {
    /// (slot number, CLSID) for each slot value present.
    pub slots: Vec<(u8, String)>,
    /// Multi-effect list values present (13/14/15).
    pub multi: bool,
    /// Supported-mode lists present (5/6/7 under the modes key).
    pub mode_lists: Vec<u8>,
    pub combined: bool,
}

impl Before {
    fn has(&self, slot: u8) -> bool {
        self.slots.iter().any(|(n, _)| *n == slot)
    }
}

fn is_eapo(clsid: &str) -> bool {
    [PRE_MIX, POST_MIX].iter().any(|c| c.eq_ignore_ascii_case(clsid))
}

fn is_empty(clsid: &str) -> bool {
    clsid.trim().is_empty() || clsid.eq_ignore_ascii_case(NULL_CLSID)
}

/// Same choice as Equalizer APO's Device Selector for Windows 8.1 and later.
pub fn mode(b: &Before) -> Mode {
    let newer = [SFX, MFX, EFX].iter().any(|s| b.has(*s)) || b.multi;
    if (b.has(LFX) || b.has(GFX)) && !newer {
        Mode::LfxGfx
    } else if b.combined {
        Mode::SfxMfx
    } else {
        Mode::SfxEfx
    }
}

/// The FxProperties changes for "on": values to set, values to delete, and
/// supported-mode lists to add.
#[derive(Debug, PartialEq, Eq)]
pub struct Plan {
    pub set: Vec<(String, &'static str)>,
    pub delete: Vec<String>,
    pub add_modes: Vec<u8>,
}

pub fn plan(b: &Before, capture: bool) -> Result<Plan> {
    if let Some((n, c)) = b.slots.iter().find(|(_, c)| !is_empty(c) && !is_eapo(c)) {
        bail!("this device has its own driver effects ({c} in slot {n}); PeriCore leaves those alone");
    }
    let (pre, post, drop): (u8, u8, &[u8]) = match mode(b) {
        Mode::LfxGfx => (LFX, GFX, &[SFX, MFX, EFX]),
        Mode::SfxMfx => (SFX, MFX, &[LFX, GFX]),
        Mode::SfxEfx => (SFX, EFX, &[LFX, GFX]),
    };
    let mut set = vec![(fx(pre), PRE_MIX)];
    let mut needed_modes = vec![pre];
    if !capture {
        set.push((fx(post), POST_MIX));
        needed_modes.push(post);
    }
    let lfx_gfx = mode(b) == Mode::LfxGfx;
    let delete = drop.iter().filter(|s| b.has(**s)).map(|s| fx(*s)).collect();
    let add_modes = if lfx_gfx { Vec::new() } else { needed_modes.into_iter().filter(|m| !b.mode_lists.contains(m)).collect() };
    Ok(Plan { set, delete, add_modes })
}

fn endpoint_path(capture: bool, id: &str) -> String {
    format!(r"{ENDPOINTS}\{}\{id}", if capture { "Capture" } else { "Render" })
}

fn read_before(capture: bool, id: &str) -> Result<Before> {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let base = endpoint_path(capture, id);
    let fxk = hklm
        .open_subkey_with_flags(format!(r"{base}\FxProperties"), KEY_READ | KEY_WOW64_64KEY)
        .context("this device has no effects settings (FxProperties) yet; use Equalizer APO's Device Selector for it")?;
    let text = |name: &str| fxk.get_value::<String, _>(name).ok();
    let props = hklm.open_subkey_with_flags(format!(r"{base}\Properties"), KEY_READ | KEY_WOW64_64KEY).ok();
    Ok(Before {
        slots: SLOTS.iter().filter_map(|s| text(&fx(*s)).map(|v| (*s, v))).collect(),
        multi: MULTI.iter().any(|m| fxk.get_raw_value(fx(*m)).is_ok()),
        mode_lists: [SFX, MFX, EFX].into_iter().filter(|m| fxk.get_raw_value(modes(*m)).is_ok()).collect(),
        combined: props.is_some_and(|p| p.get_raw_value(COMBINED).is_ok()),
    })
}

/// Turns Equalizer APO on for an endpoint. Safe to repeat.
pub fn enable(capture: bool, id: &str) -> Result<()> {
    let before = read_before(capture, id)?;
    if before.slots.iter().any(|(_, c)| is_eapo(c)) {
        return Ok(()); // already on
    }
    let plan = plan(&before, capture)?;
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);

    // Record the originals first, in Equalizer APO's own format.
    let (record, _) = hklm
        .create_subkey_with_flags(format!(r"{CHILD_APOS}\{id}"), KEY_SET_VALUE | KEY_WOW64_64KEY)
        .context("can't write Equalizer APO's device record (run as administrator)")?;
    for s in SLOTS {
        let original = before.slots.iter().find(|(n, _)| *n == s).map_or("!VALUE", |(_, c)| c.as_str());
        record.set_value(fx(s), &original)?;
    }
    record.set_value("PreMixChild", &"")?;
    record.set_value("PostMixChild", &"")?;
    record.set_value("AllowSilentBufferModification", &"false")?;
    let added: Vec<String> = plan.add_modes.iter().map(u8::to_string).collect();
    record.set_value(ADDED_MODES, &added.join(","))?;
    record.set_value("Version", &RECORD_VERSION)?;

    let fxk = hklm
        .open_subkey_with_flags(format!(r"{}\FxProperties", endpoint_path(capture, id)), KEY_SET_VALUE | KEY_WOW64_64KEY)
        .context("Windows refused to change this device's effects (run as administrator)")?;
    for m in &plan.add_modes {
        fxk.set_value(modes(*m), &vec![DEFAULT_MODE.to_string()])?;
    }
    for name in &plan.delete {
        fxk.delete_value(name)?;
    }
    for (name, clsid) in &plan.set {
        fxk.set_value(name, clsid)?;
    }
    Ok(())
}

/// Turns Equalizer APO off for an endpoint and restores the original values.
pub fn disable(capture: bool, id: &str) -> Result<()> {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let record_path = format!(r"{CHILD_APOS}\{id}");
    let record = hklm
        .open_subkey_with_flags(&record_path, KEY_READ | KEY_WOW64_64KEY)
        .context("Equalizer APO isn't on for this device")?;
    let fxk = hklm
        .open_subkey_with_flags(format!(r"{}\FxProperties", endpoint_path(capture, id)), KEY_SET_VALUE | KEY_WOW64_64KEY)
        .context("Windows refused to change this device's effects (run as administrator)")?;
    for s in SLOTS {
        match record.get_value::<String, _>(fx(s)).ok().as_deref() {
            Some("!VALUE") | Some("!KEY") => {
                let _ = fxk.delete_value(fx(s));
            }
            Some(v) if !v.is_empty() => fxk.set_value(fx(s), &v)?,
            _ => {}
        }
    }
    let added: String = record.get_value(ADDED_MODES).unwrap_or_default();
    for m in added.split(',').filter_map(|m| m.parse::<u8>().ok()) {
        let _ = fxk.delete_value(modes(m));
    }
    drop(record);
    let _ = hklm.delete_subkey_all(&record_path);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(p: &Plan) -> Vec<&str> {
        p.set.iter().map(|(n, _)| n.as_str()).collect()
    }

    #[test]
    fn empty_usb_headset_uses_sfx_efx() {
        // Like a USB headset: only supported-mode lists for SFX and MFX.
        let b = Before { mode_lists: vec![SFX, MFX], ..Default::default() };
        assert_eq!(mode(&b), Mode::SfxEfx);
        let p = plan(&b, false).unwrap();
        assert_eq!(names(&p), [fx(SFX).as_str(), fx(EFX).as_str()]);
        assert_eq!(p.set[0].1, PRE_MIX);
        assert_eq!(p.set[1].1, POST_MIX);
        assert_eq!(p.add_modes, [EFX]);
        assert!(p.delete.is_empty());
    }

    #[test]
    fn capture_gets_pre_mix_only() {
        let b = Before { mode_lists: vec![SFX, MFX], ..Default::default() };
        let p = plan(&b, true).unwrap();
        assert_eq!(p.set, [(fx(SFX), PRE_MIX)]);
        assert!(p.add_modes.is_empty());
    }

    #[test]
    fn modes_follow_device_selector() {
        let null = NULL_CLSID.to_string();
        let lfx = Before { slots: vec![(LFX, null.clone())], ..Default::default() };
        assert_eq!(mode(&lfx), Mode::LfxGfx);
        let p = plan(&lfx, false).unwrap();
        assert_eq!(names(&p), [fx(LFX).as_str(), fx(GFX).as_str()]);
        assert!(p.add_modes.is_empty());
        let bt = Before { combined: true, ..Default::default() };
        assert_eq!(mode(&bt), Mode::SfxMfx);
        let mixed = Before { slots: vec![(LFX, null.clone()), (SFX, null)], ..Default::default() };
        let p = plan(&mixed, false).unwrap();
        assert_eq!(p.delete, [fx(LFX)]);
    }

    #[test]
    fn driver_effects_are_left_alone() {
        let b = Before { slots: vec![(SFX, "{12345678-0000-0000-0000-000000000001}".into())], ..Default::default() };
        assert!(plan(&b, false).is_err());
    }
}
