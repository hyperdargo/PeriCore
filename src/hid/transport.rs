//! Feature-report transport for 65-byte (report id + 64 data bytes) vendor
//! interfaces. Kept behind a trait so protocol code can be unit-tested with a
//! fake device.

use super::checksum::Checksum;
use anyhow::{bail, Context, Result};
use std::time::Duration;

pub const REPORT_LEN: usize = 64;

/// Raw access to a HID feature-report channel.
pub trait FeatureIo: Send {
    /// `data[0]` is the report id.
    fn send_feature(&self, data: &[u8]) -> Result<()>;
    /// `buf[0]` must hold the report id on entry. Returns bytes read.
    fn get_feature(&self, buf: &mut [u8]) -> Result<usize>;
}

impl FeatureIo for hidapi::HidDevice {
    fn send_feature(&self, data: &[u8]) -> Result<()> {
        self.send_feature_report(data).context("send_feature_report failed")
    }
    fn get_feature(&self, buf: &mut [u8]) -> Result<usize> {
        self.get_feature_report(buf).context("get_feature_report failed")
    }
}

/// Which device sits behind the 2.4G receiver when talking through it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[allow(dead_code)] // protocol variants kept for keyboards behind receivers
pub enum Target {
    #[default]
    Direct,
    DangleKeyboard,
    DangleMouse,
}

/// Request/response channel used by the RY firmware family.
pub struct RyChannel {
    io: Box<dyn FeatureIo>,
    report_id: u8,
    reply_delay: Duration,
    target: Target,
}

impl RyChannel {
    pub fn new(io: Box<dyn FeatureIo>, target: Target) -> Self {
        Self { io, report_id: 0, reply_delay: Duration::from_millis(12), target }
    }

    #[cfg(test)]
    pub fn with_delay(io: Box<dyn FeatureIo>, reply_delay: Duration) -> Self {
        Self { io, report_id: 0, reply_delay, target: Target::Direct }
    }

    /// Like `query`, but overrides the routing target for one request.
    pub fn query_as(&self, payload: &[u8], checksum: Checksum, target: Target) -> Result<[u8; REPORT_LEN]> {
        let frame = self.frame_for(payload, checksum, target)?;
        self.io.send_feature(&frame)?;
        self.read_reply()
    }

    pub fn frame(&self, payload: &[u8], checksum: Checksum) -> Result<Vec<u8>> {
        self.frame_for(payload, checksum, self.target)
    }

    /// Builds the on-wire frame: report id followed by 64 payload bytes.
    fn frame_for(&self, payload: &[u8], checksum: Checksum, target: Target) -> Result<Vec<u8>> {
        let body = checksum.apply(&route(payload, target));
        if body.len() > REPORT_LEN {
            bail!("payload too long: {} bytes", body.len());
        }
        let mut frame = Vec::with_capacity(REPORT_LEN + 1);
        frame.push(self.report_id);
        frame.extend_from_slice(&body);
        frame.resize(REPORT_LEN + 1, 0);
        Ok(frame)
    }

    /// Write-only command (SET_*).
    pub fn write(&self, payload: &[u8], checksum: Checksum) -> Result<()> {
        let frame = self.frame(payload, checksum)?;
        self.io.send_feature(&frame)
    }

    /// Writes a command then reads the 64-byte reply (GET_*).
    pub fn query(&self, payload: &[u8], checksum: Checksum) -> Result<[u8; REPORT_LEN]> {
        self.write(payload, checksum)?;
        self.read_reply()
    }

    fn read_reply(&self) -> Result<[u8; REPORT_LEN]> {
        std::thread::sleep(self.reply_delay);
        let mut buf = [0u8; REPORT_LEN + 1];
        buf[0] = self.report_id;
        let n = self.io.get_feature(&mut buf)?;
        // A partial reply would be zero-filled and could later be written back
        // as a settings block, so anything but a full report is an error.
        if n != REPORT_LEN + 1 {
            bail!("short reply ({n} bytes, expected {})", REPORT_LEN + 1);
        }
        let mut out = [0u8; REPORT_LEN];
        // hidapi on Windows returns the report id as the first byte.
        out.copy_from_slice(&buf[1..]);
        Ok(out)
    }
}

/// Applies receiver routing to a payload. Until the receiver framing is
/// confirmed, every target sends the payload unchanged.
fn route(payload: &[u8], target: Target) -> Vec<u8> {
    match target {
        Target::Direct | Target::DangleKeyboard | Target::DangleMouse => payload.to_vec(),
    }
}

#[cfg(test)]
pub mod fake {
    use super::*;
    use std::sync::{Arc, Mutex};

    /// Records writes and replays a scripted reply.
    #[derive(Clone, Default)]
    pub struct FakeIo {
        pub sent: Arc<Mutex<Vec<Vec<u8>>>>,
        pub reply: Arc<Mutex<Vec<u8>>>,
    }

    impl FakeIo {
        pub fn replying(reply: &[u8]) -> Self {
            let f = Self::default();
            let mut r = vec![0u8];
            r.extend_from_slice(reply);
            r.resize(REPORT_LEN + 1, 0);
            *f.reply.lock().unwrap() = r;
            f
        }
        pub fn last_sent(&self) -> Vec<u8> {
            self.sent.lock().unwrap().last().cloned().unwrap_or_default()
        }
    }

    impl FeatureIo for FakeIo {
        fn send_feature(&self, data: &[u8]) -> Result<()> {
            self.sent.lock().unwrap().push(data.to_vec());
            Ok(())
        }
        fn get_feature(&self, buf: &mut [u8]) -> Result<usize> {
            let r = self.reply.lock().unwrap();
            let n = r.len().min(buf.len());
            buf[..n].copy_from_slice(&r[..n]);
            Ok(n)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fake::FakeIo;
    use super::*;

    fn chan(io: FakeIo) -> RyChannel {
        RyChannel::with_delay(Box::new(io), Duration::ZERO)
    }

    #[test]
    fn frame_is_65_bytes_with_report_id() {
        let c = chan(FakeIo::default());
        let f = c.frame(&[0xF1], Checksum::Bit7).unwrap();
        assert_eq!(f.len(), 65);
        assert_eq!(f[0], 0);
        assert_eq!(f[1], 0xF1);
        assert_eq!(f[8], 0xFF - 0xF1);
    }

    #[test]
    fn rejects_oversized_payload() {
        let c = chan(FakeIo::default());
        assert!(c.frame(&[0u8; 65], Checksum::None).is_err());
    }

    #[test]
    fn query_rejects_short_reply() {
        let io = FakeIo::default();
        *io.reply.lock().unwrap() = vec![0, 0xD3, 1, 2];
        assert!(chan(io).query(&[0xD3], Checksum::Bit7).is_err());
    }

    #[test]
    fn query_strips_report_id() {
        let io = FakeIo::replying(&[0xF1, 0x2D, 0x09]);
        let c = chan(io.clone());
        let r = c.query(&[0xF1], Checksum::Bit7).unwrap();
        assert_eq!(&r[..3], &[0xF1, 0x2D, 0x09]);
        assert_eq!(io.last_sent()[1], 0xF1);
    }
}
