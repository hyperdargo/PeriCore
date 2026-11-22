//! Points the installer at the freshly built PeriCore.exe, reads PeriCore's
//! version, and embeds the icon plus an "asInvoker" manifest (a file named
//! *Setup* would otherwise trigger Windows' automatic admin prompt).

use std::path::PathBuf;

const MANIFEST: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <trustInfo xmlns="urn:schemas-microsoft-com:asm.v3">
    <security><requestedPrivileges>
      <requestedExecutionLevel level="asInvoker" uiAccess="false"/>
    </requestedPrivileges></security>
  </trustInfo>
  <application xmlns="urn:schemas-microsoft-com:asm.v3">
    <windowsSettings>
      <dpiAwareness xmlns="http://schemas.microsoft.com/SMI/2016/WindowsSettings">PerMonitorV2</dpiAwareness>
    </windowsSettings>
  </application>
</assembly>"#;

fn main() {
    let root = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("../..");
    let exe = root.join("target/release/PeriCore.exe");
    assert!(exe.exists(), "build PeriCore first: cargo build --release (missing {})", exe.display());
    println!("cargo:rerun-if-changed={}", exe.display());
    println!("cargo:rustc-env=PERICORE_EXE={}", exe.canonicalize().unwrap().display());

    let cargo = std::fs::read_to_string(root.join("Cargo.toml")).expect("read PeriCore Cargo.toml");
    let version = cargo
        .lines()
        .find_map(|l| l.strip_prefix("version = \"").and_then(|v| v.strip_suffix('"')))
        .expect("PeriCore version");
    println!("cargo:rerun-if-changed={}", root.join("Cargo.toml").display());
    println!("cargo:rustc-env=PERICORE_VERSION={version}");

    let mut res = winresource::WindowsResource::new();
    res.set_icon(root.join("assets/pericore.ico").to_str().unwrap())
        .set("ProductName", "PeriCore")
        .set("FileDescription", "PeriCore Setup")
        .set("OriginalFilename", "PeriCore-Setup.exe")
        .set("ProductVersion", version)
        .set_manifest(MANIFEST);
    if let Err(e) = res.compile() {
        println!("cargo:warning=resources not embedded: {e}");
    }
}
