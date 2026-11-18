//! Embeds the PeriCore icon and version info into PeriCore.exe.

fn main() {
    println!("cargo:rerun-if-changed=assets/pericore.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/pericore.ico")
            .set("ProductName", "PeriCore")
            .set("FileDescription", "PeriCore - mouse & keyboard hub")
            .set("OriginalFilename", "PeriCore.exe");
        if let Err(e) = res.compile() {
            println!("cargo:warning=icon not embedded: {e}");
        }
    }
}
