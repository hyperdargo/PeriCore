#![cfg_attr(not(test), windows_subsystem = "windows")]

mod audio;
mod battery_log;
mod devices;
mod hid;
mod install_layout;
mod os;
mod probe;
mod macros;
mod store;
mod tray;
mod ui;
mod uninstall;

/// Modes:
///   (none)    open settings window (also starts the tray)
///   --tray    tray battery indicator only (used for autostart)
///   --ui      settings window
///   --probe   print detected devices and read-only diagnostics
fn main() -> anyhow::Result<()> {
    os::install_panic_log();
    let arg = std::env::args().nth(1).unwrap_or_default();
    let result = run_mode(&arg);
    if let Err(e) = &result {
        os::log_error(&format!("{arg}: {e:#}"));
    }
    result
}

fn run_mode(arg: &str) -> anyhow::Result<()> {
    match arg {
        "--probe" => {
            attach_console();
            probe::run()
        }
        "--listen" => {
            attach_console();
            let a: Vec<String> = std::env::args().skip(2).collect();
            let hex = |s: Option<&String>, d: u16| s.and_then(|v| u16::from_str_radix(v, 16).ok()).unwrap_or(d);
            let secs = a.get(2).and_then(|v| v.parse().ok()).unwrap_or(10);
            probe::listen(hex(a.first(), 0x3151), hex(a.get(1), 0x5007), secs)
        }
        "--tray" => tray::run(),
        "--uninstall" => uninstall::run(std::env::args().any(|a| a == "--quiet")),
        // `--sound "Preset name"`: switch to a saved sound preset (for shortcuts/hotkeys).
        "--sound" => {
            attach_console();
            let name = std::env::args().nth(2).unwrap_or_default();
            let mut s = audio::load();
            if !s.use_profile(&name) {
                let names: Vec<&str> = s.profiles.iter().map(|p| p.name.as_str()).collect();
                anyhow::bail!("no saved sound preset \"{name}\" (saved: {})", names.join(", "));
            }
            audio::save_and_apply(&s)?;
            println!("Sound preset: {name}");
            Ok(())
        }
        "--dock-upload" => {
            attach_console();
            probe::dock_upload(std::env::args().nth(2).unwrap_or_default().into())
        }
        _ => ui::run(),
    }
}

/// GUI-subsystem exe: reattach to the parent console so `--probe` can print.
fn attach_console() {
    use windows_sys::Win32::System::Console::{AttachConsole, ATTACH_PARENT_PROCESS};
    // SAFETY: plain FFI call; failure (no parent console) is harmless.
    unsafe {
        AttachConsole(ATTACH_PARENT_PROCESS);
    }
}
