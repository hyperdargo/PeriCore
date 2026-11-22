//! High-level operations for RY-protocol mice (wired or via 2.4G receiver).

pub mod buttons;
pub mod protocol;
pub mod screen;

use super::{db, Detected, Kind};
use crate::hid::{Checksum, RyChannel, Target};
use anyhow::{anyhow, Result};
use hidapi::HidApi;
use protocol::{cmd, BatteryInfo, MouseOption0, MouseOption1};

/// RY mice store up to 8 onboard profiles.
const MAX_PROFILES: u8 = 8;

fn sleep_ms(ms: u64) {
    std::thread::sleep(std::time::Duration::from_millis(ms));
}

pub struct RyDevice {
    chan: RyChannel,
    pub info: Detected,
    profile: u8,
}

impl RyDevice {
    pub fn open(api: &HidApi, info: &Detected) -> Result<Self> {
        let dev = api.open_path(&info.path)?;
        let target = match info.kind {
            Kind::Receiver => Target::DangleMouse,
            _ => Target::Direct,
        };
        Ok(Self { chan: RyChannel::new(Box::new(dev), target), info: info.clone(), profile: 0 })
    }

    /// For receivers: asks which model is paired and returns its catalogue name.
    pub fn identify(&self) -> Option<String> {
        if self.info.kind != Kind::Receiver {
            return Some(self.info.name.clone());
        }
        self.paired_entry().map(|e| e.display_name.clone())
    }

    fn paired_entry(&self) -> Option<&'static db::DbEntry> {
        let reply = self.chan.query_as(&[cmd::GET_DANGLE_ID], Checksum::Bit7, Target::Direct).ok()?;
        let id = protocol::parse_dangle_id(&reply)?;
        db::by_id(id as i64)
    }

    /// True for wired mice, and receivers whose catalogue entry is a mouse
    /// (or a generic receiver). Keyboards behind a receiver return false.
    pub fn is_mouse(&self) -> bool {
        match self.info.kind {
            Kind::Mouse => true,
            Kind::Keyboard | Kind::Controller => false,
            Kind::Receiver => self
                .paired_entry()
                .is_some_and(|e| e.kind == "mouse" || (e.kind == "dangle" && !e.name.contains("keyboard"))),
        }
    }

    /// Diagnostic read with BIT7 checksum.
    pub fn raw_query(&self, payload: &[u8]) -> Result<[u8; 64]> {
        self.chan.query(payload, Checksum::Bit7)
    }

    /// Polls receiver status until the paired mouse is awake and the
    /// previous write finished (about 3 s max, enough to wake it by moving it).
    fn wait_receiver(&self) -> Result<()> {
        let mut last = [0u8; 8];
        for _ in 0..30 {
            sleep_ms(100);
            let s = self.chan.query_as(&[cmd::RECEIVER_STATUS], Checksum::None, Target::Direct)?;
            if protocol::receiver_mouse_ready(&s) {
                sleep_ms(1);
                return Ok(());
            }
            last.copy_from_slice(&s[..8]);
        }
        Err(anyhow!("mouse is asleep or out of range - move it and retry (status {:02x?})", &last[..7]))
    }

    /// Sends a mouse command through the receiver (F6 05 selects the mouse).
    fn receiver_send(&self, payload: &[u8]) -> Result<()> {
        self.wait_receiver()?;
        self.chan.write_as(&[0xF6, 0x05], Checksum::None, Target::Direct)?;
        sleep_ms(10);
        self.chan.write_as(payload, Checksum::Bit7, Target::Direct)?;
        sleep_ms(5);
        Ok(())
    }

    /// GET: direct for wired mice; for receivers, send then fetch with FC.
    fn query_cmd(&self, payload: &[u8]) -> Result<[u8; 64]> {
        if self.info.kind != Kind::Receiver {
            return self.chan.query(payload, Checksum::Bit7);
        }
        self.receiver_send(payload)?;
        self.wait_receiver()?;
        sleep_ms(10);
        self.chan.write_as(&[0xFC], Checksum::None, Target::Direct)?;
        self.chan.read()
    }

    /// SET: direct for wired mice; for receivers, send and wait until the
    /// receiver reports the write finished.
    fn write_cmd(&self, payload: &[u8]) -> Result<()> {
        if self.info.kind != Kind::Receiver {
            return self.chan.write(payload, Checksum::Bit7);
        }
        self.receiver_send(payload)?;
        self.wait_receiver()
    }

    /// Raw receiver status (F7), for diagnostics.
    pub fn receiver_status(&self) -> Result<[u8; 64]> {
        self.chan.query_as(&[cmd::RECEIVER_STATUS], Checksum::None, Target::Direct)
    }

    /// Battery plus whether the mouse is asleep. A sleeping mouse behind a
    /// receiver (F7 r[4] != 0) only gives its last reported level.
    pub fn battery_status(&self) -> Result<(BatteryInfo, bool)> {
        if self.info.kind == Kind::Receiver {
            let reply = self.receiver_status()?;
            let b = protocol::parse_receiver_status(&reply, true)
                .ok_or_else(|| anyhow!("no battery in receiver status {:02x?}", &reply[..7]))?;
            return Ok((b, reply[4] != 0));
        }
        Ok((self.battery()?, false))
    }

    pub fn battery(&self) -> Result<BatteryInfo> {
        if self.info.kind == Kind::Receiver {
            let reply = self.chan.query_as(&[cmd::RECEIVER_STATUS], Checksum::None, Target::Direct)?;
            return protocol::parse_receiver_status(&reply, true)
                .ok_or_else(|| anyhow!("mouse offline or asleep (status {:02x?})", &reply[..7]));
        }
        let reply = self.chan.query(&[cmd::GET_BATTERY], Checksum::Bit7)?;
        protocol::parse_battery(&reply).ok_or_else(|| anyhow!("unexpected battery reply {:02x?}", &reply[..4]))
    }

    pub fn read_option0(&mut self) -> Result<MouseOption0> {
        let reply = self.query_cmd(&[cmd::MOUSE_GET_OPTION0])?;
        if reply[0] != cmd::MOUSE_GET_OPTION0 {
            return Err(anyhow!("option0: bad echo {:02x}", reply[0]));
        }
        if reply[8] >= MAX_PROFILES {
            return Err(anyhow!("option0: implausible profile index {}", reply[8]));
        }
        self.profile = reply[8];
        Ok(MouseOption0(reply))
    }

    pub fn write_option0(&self, o: &MouseOption0) -> Result<()> {
        self.write_cmd(&o.to_set_payload())
    }

    pub fn read_option1(&self) -> Result<MouseOption1> {
        let reply = self.query_cmd(&[cmd::MOUSE_GET_OPTION1, self.profile])?;
        if reply[0] != cmd::MOUSE_GET_OPTION1 {
            return Err(anyhow!("option1: bad echo {:02x}", reply[0]));
        }
        Ok(MouseOption1(reply))
    }

    pub fn write_option1(&self, o: &MouseOption1) -> Result<()> {
        self.write_cmd(&o.to_set_payload(self.profile))
    }


    /// Reads the 16-slot button table of the active profile.
    pub fn read_buttons(&self) -> Result<[[u8; 4]; buttons::SLOTS]> {
        let reply = self.query_cmd(&[buttons::GET_KEYS, self.profile])?;
        Ok(buttons::parse_table(&reply))
    }

    /// Writes one button slot in the active profile.
    pub fn write_button(&self, slot: usize, action: buttons::ButtonAction) -> Result<()> {
        self.write_cmd(&buttons::set_payload(self.profile, slot, action))
    }

    /// Uploads a macro to onboard index `index` (pages of 56 bytes, then the
    /// 500 ms settle time the vendor app uses).
    pub fn write_macro(&self, index: u8, m: &crate::macros::Macro) -> Result<()> {
        if !m.fits_ry() {
            return Err(anyhow!("macro is too long for the mouse ({} bytes)", m.ry_len()));
        }
        for page in crate::macros::ry_pages(index, m) {
            self.write_cmd(&page)?;
        }
        sleep_ms(500);
        Ok(())
    }
}
