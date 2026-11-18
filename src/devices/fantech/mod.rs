//! Fantech MAXFIT61 driver (interrupt-report protocol on interface MI_02).

pub mod protocol;

use super::Detected;
use anyhow::{anyhow, Result};
use hidapi::{DeviceInfo, HidApi, HidDevice};
use protocol::{KbLighting, LightCaps, LEN};
use std::time::{Duration, Instant};

pub const NAME: &str = "Fantech MAXFIT61";
const VID: u16 = 0x0416;
const PID: u16 = 0xC345;

/// The vendor app accepts the interface whose path contains `mi_02`.
pub fn matches(d: &DeviceInfo) -> bool {
    d.vendor_id() == VID
        && d.product_id() == PID
        && (d.interface_number() == 2 || d.path().to_string_lossy().to_ascii_lowercase().contains("mi_02"))
}

pub struct Maxfit61 {
    dev: HidDevice,
    pub info: Detected,
    report_id: u8,
}

/// The vendor app switches to report id 06 when the interface path is a `col06` collection.
fn report_id_for(path: &str) -> u8 {
    if path.to_ascii_lowercase().contains("col06") {
        0x06
    } else {
        protocol::REPORT_ID
    }
}

impl Maxfit61 {
    pub fn open(api: &HidApi, info: &Detected) -> Result<Self> {
        let dev = api.open_path(&info.path)?;
        dev.set_blocking_mode(false)?;
        let report_id = report_id_for(&info.path.to_string_lossy());
        Ok(Self { dev, info: info.clone(), report_id })
    }

    fn send(&self, packet: &[u8; LEN]) -> Result<()> {
        let mut p = *packet;
        p[0] = self.report_id;
        self.dev.write(&p)?;
        Ok(())
    }

    /// Sends a request and waits for a reply whose opcode matches.
    pub(crate) fn request(&self, packet: &[u8; LEN]) -> Result<[u8; LEN]> {
        self.drain();
        self.send(packet)?;
        let deadline = Instant::now() + Duration::from_millis(400);
        let mut buf = [0u8; LEN];
        while Instant::now() < deadline {
            let n = self.dev.read_timeout(&mut buf, 50)?;
            if n >= 2 && buf[1] == packet[1] {
                return Ok(buf);
            }
        }
        Err(anyhow!("no reply to opcode {:02X}", packet[1]))
    }

    fn drain(&self) {
        let mut buf = [0u8; LEN];
        while matches!(self.dev.read_timeout(&mut buf, 0), Ok(n) if n > 0) {}
    }

    pub fn identify(&self) -> Option<String> {
        let r = self.request(&protocol::simple(protocol::op::IDENTIFY)).ok()?;
        protocol::parse_identify(&r)
    }

    pub fn light_caps(&self) -> Option<LightCaps> {
        let r = self.request(&protocol::simple(protocol::op::LIGHT_CAPS)).ok()?;
        protocol::parse_caps(&r)
    }

    pub fn set_lighting(&self, l: &KbLighting) -> Result<()> {
        self.send(&l.packet())
    }

    pub fn factory_reset(&self) -> Result<()> {
        let mut p = protocol::simple(protocol::op::RESET);
        p[6] = 1;
        self.send(&p)
    }
}
