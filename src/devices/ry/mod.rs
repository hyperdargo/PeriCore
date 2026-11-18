//! High-level operations for RY-protocol mice (wired or via 2.4G receiver).

pub mod protocol;

use super::{db, Detected, Kind};
use crate::hid::{Checksum, RyChannel, Target};
use anyhow::{anyhow, Result};
use hidapi::HidApi;
use protocol::{cmd, BatteryInfo, MouseOption0, MouseOption1};

/// RY mice store up to 8 onboard profiles.
const MAX_PROFILES: u8 = 8;

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
            Kind::Keyboard => false,
            Kind::Receiver => self
                .paired_entry()
                .is_some_and(|e| e.kind == "mouse" || (e.kind == "dangle" && !e.name.contains("keyboard"))),
        }
    }

    /// Diagnostic read with BIT7 checksum.
    pub fn raw_query(&self, payload: &[u8]) -> Result<[u8; 64]> {
        self.chan.query(payload, Checksum::Bit7)
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
        let reply = self.chan.query(&[cmd::MOUSE_GET_OPTION0], Checksum::Bit7)?;
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
        self.chan.write(&o.to_set_payload(), Checksum::Bit7)
    }

    pub fn read_option1(&self) -> Result<MouseOption1> {
        let reply = self.chan.query(&[cmd::MOUSE_GET_OPTION1, self.profile], Checksum::Bit7)?;
        if reply[0] != cmd::MOUSE_GET_OPTION1 {
            return Err(anyhow!("option1: bad echo {:02x}", reply[0]));
        }
        Ok(MouseOption1(reply))
    }

    pub fn write_option1(&self, o: &MouseOption1) -> Result<()> {
        self.chan.write(&o.to_set_payload(self.profile), Checksum::Bit7)
    }

}
