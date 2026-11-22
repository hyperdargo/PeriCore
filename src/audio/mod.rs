//! System-wide sound presets and mic noise suppression via Equalizer APO.

pub mod config;
pub mod devices;
pub mod eapo;
pub mod fx_install;
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

const DEVICE_RESULT: &str = "eapo-device-result";

/// True for a `{xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx}` endpoint id.
fn valid_id(id: &str) -> bool {
    let inner = id.strip_prefix('{').and_then(|s| s.strip_suffix('}')).unwrap_or("");
    inner.len() == 36 && inner.chars().enumerate().all(|(i, c)| if [8, 13, 18, 23].contains(&i) { c == '-' } else { c.is_ascii_hexdigit() })
}

/// Elevated helper (`--eapo-device on|off render:{id} capture:{id} ...`):
/// turns Equalizer APO on or off for each endpoint and records the outcome.
pub fn run_device_helper(args: &[String]) -> anyhow::Result<()> {
    let on = match args.first().map(String::as_str) {
        Some("on") => true,
        Some("off") => false,
        _ => anyhow::bail!("expected on or off"),
    };
    let mut errors = Vec::new();
    for t in &args[1..] {
        let (capture, id) = match t.split_once(':') {
            Some(("render", id)) => (false, id),
            Some(("capture", id)) => (true, id),
            _ => anyhow::bail!("bad device {t}"),
        };
        if !valid_id(id) {
            anyhow::bail!("bad device id {id}");
        }
        let r = if on { fx_install::enable(capture, id) } else { fx_install::disable(capture, id) };
        if let Err(e) = r {
            errors.push(format!("{}: {e:#}", if capture { "microphone" } else { "playback" }));
        }
    }
    let text = errors.join("; ");
    crate::store::save(DEVICE_RESULT, &text)?;
    if errors.is_empty() { Ok(()) } else { anyhow::bail!(text) }
}

/// Asks for admin rights (UAC) and turns Equalizer APO on for these endpoints
/// (`(capture, id)`). Windows applies it after a restart.
pub fn enable_devices(targets: &[(bool, String)]) -> anyhow::Result<()> {
    let mut args = vec!["--eapo-device".to_string(), "on".to_string()];
    args.extend(targets.iter().map(|(c, id)| format!("{}:{id}", if *c { "capture" } else { "render" })));
    let _ = crate::store::save(DEVICE_RESULT, &String::new());
    let code = crate::os::run_self_elevated(&args)?;
    if code != 0 {
        let msg: String = crate::store::load(DEVICE_RESULT).unwrap_or_default();
        anyhow::bail!("couldn't turn Equalizer APO on: {}", if msg.is_empty() { "unknown error (see PeriCore's error log)".into() } else { msg });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::valid_id;

    #[test]
    fn endpoint_ids_are_checked() {
        assert!(valid_id("{3ca94c89-93ac-4f20-9059-7c320330217a}"));
        assert!(!valid_id("{3ca94c89-93ac-4f20-9059-7c320330217a}/../x"));
        assert!(!valid_id("3ca94c89-93ac-4f20-9059-7c320330217a"));
    }
}
