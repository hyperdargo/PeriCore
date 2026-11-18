//! `--probe`: lists supported devices and runs read-only queries only.

use crate::devices::{self, fantech::Maxfit61, ry::RyDevice, Family};
use anyhow::Result;
use hidapi::HidApi;

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02X}")).collect::<Vec<_>>().join(" ")
}

pub fn run() -> Result<()> {
    let api = HidApi::new()?;
    let found = devices::discover(&api);
    println!("{} supported interface(s)", found.len());
    for d in &found {
        println!("\n== {} [{:?} {:?}] {:04X}:{:04X}\n   {}", d.name, d.family, d.kind, d.vid, d.pid, d.path.to_string_lossy());
        match d.family {
            Family::Ry => probe_ry(&api, d),
            Family::FantechMaxfit61 => probe_fantech(&api, d),
        }
    }
    Ok(())
}

/// `--listen VID PID SECS`: prints input reports from every vendor-page
/// interface of a device. Passive: nothing is sent.
pub fn listen(vid: u16, pid: u16, secs: u64) -> Result<()> {
    let api = HidApi::new()?;
    let devs: Vec<_> = api
        .device_list()
        .filter(|d| d.vendor_id() == vid && d.product_id() == pid && d.usage_page() >= 0xFF00)
        .filter_map(|d| {
            let label = format!("if{} page {:04X} usage {:02X}", d.interface_number(), d.usage_page(), d.usage());
            api.open_path(d.path()).ok().map(|h| (label, h))
        })
        .collect();
    println!("listening on {} interface(s) for {secs}s", devs.len());
    let end = std::time::Instant::now() + std::time::Duration::from_secs(secs);
    let mut buf = [0u8; 65];
    while std::time::Instant::now() < end {
        for (label, h) in &devs {
            if let Ok(n) = h.read_timeout(&mut buf, 20) {
                if n > 0 {
                    println!("[{label}] {}", hex(&buf[..n]));
                }
            }
        }
    }
    Ok(())
}

fn probe_ry(api: &HidApi, d: &devices::Detected) {
    let mut dev = match RyDevice::open(api, d) {
        Ok(v) => v,
        Err(e) => return println!("   open failed: {e:#}"),
    };
    println!("   identify: {:?}", dev.identify());
    let probes: &[(&str, &[u8])] = if d.kind == devices::Kind::Receiver {
        &[]
    } else {
        &[("rev", &[0x80]), ("battery", &[0x82]), ("profile", &[0x85])]
    };
    for &(label, payload) in probes {
        match dev.raw_query(payload) {
            Ok(r) => println!("   {label:8} -> {}", hex(&r[..16])),
            Err(e) => println!("   {label:8} -> ERR {e:#}"),
        }
    }
    if d.kind == devices::Kind::Receiver {
        match dev.battery() {
            Ok(b) => println!("   receiver battery -> {}% {:?}", b.percent, b.state),
            Err(e) => println!("   receiver battery -> ERR {e:#}"),
        }
        if !dev.is_mouse() {
            return;
        }
    }
    match dev.read_option0() {
        Ok(o) => println!("   option0  -> {}\n      rate={:?} debounce={} light={:?}", hex(&o.0), o.poll_rate(), o.debounce_ms(), o.lighting()),
        Err(e) => println!("   option0  -> ERR {e:#}"),
    }
    match dev.read_option1() {
        Ok(o) => println!("   option1  -> {}\n      dpi={:?} current={}", hex(&o.0), o.stages(), o.current_stage()),
        Err(e) => println!("   option1  -> ERR {e:#}"),
    }
}

fn probe_fantech(api: &HidApi, d: &devices::Detected) {
    match Maxfit61::open(api, d) {
        Ok(k) => {
            println!("   identify: {:?}", k.identify());
            println!("   light caps: {:?}", k.light_caps());
            if let Ok(r) = k.request(&crate::devices::fantech::protocol::simple(0x0A)) {
                println!("   caps raw: {}", hex(&r));
            }
        }
        Err(e) => println!("   open failed: {e:#}"),
    }
}
