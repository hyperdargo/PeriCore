//! Checksums used by the Rongyuan ("RY") firmware family (Ajazz mice, many
//! Fantech/Ajazz keyboards). Taken from the vendor app:
//! `n[7] = 255 - (sum(n[0..7]) & 255)` for BIT7, same idea at byte 8 for BIT8.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // protocol variants kept for keyboards behind receivers
pub enum Checksum {
    /// byte[7] = 0xFF - (sum(byte[0..7]) & 0xFF)
    Bit7,
    /// byte[8] = 0xFF - (sum(byte[0..8]) & 0xFF)
    Bit8,
    None,
}

impl Checksum {
    /// Returns a copy of `payload` with the checksum byte filled in.
    pub fn apply(self, payload: &[u8]) -> Vec<u8> {
        let idx = match self {
            Checksum::Bit7 => 7,
            Checksum::Bit8 => 8,
            Checksum::None => return payload.to_vec(),
        };
        let mut out = payload.to_vec();
        if out.len() <= idx {
            out.resize(idx + 1, 0);
        }
        let sum: u32 = out[..idx].iter().map(|&b| b as u32).sum();
        out[idx] = 0xFF - (sum & 0xFF) as u8;
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bit7_matches_vendor_formula() {
        let out = Checksum::Bit7.apply(&[0x82, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(out[7], 0xFF - 0x82);
    }

    #[test]
    fn bit7_wraps_sum() {
        let out = Checksum::Bit7.apply(&[0xFF, 0xFF, 0x02, 0, 0, 0, 0, 0]);
        // sum = 0x200 -> &0xFF = 0 -> 0xFF
        assert_eq!(out[7], 0xFF);
    }

    #[test]
    fn bit8_uses_byte_eight() {
        let out = Checksum::Bit8.apply(&[1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(out.len(), 9);
        assert_eq!(out[8], 0xFF - 36);
        assert_eq!(&out[..8], &[1, 2, 3, 4, 5, 6, 7, 8]);
    }

    #[test]
    fn none_is_identity() {
        assert_eq!(Checksum::None.apply(&[1, 2]), vec![1, 2]);
    }

    #[test]
    fn short_payload_is_padded() {
        let out = Checksum::Bit7.apply(&[0xF1]);
        assert_eq!(out.len(), 8);
        assert_eq!(out[7], 0xFF - 0xF1);
    }
}
