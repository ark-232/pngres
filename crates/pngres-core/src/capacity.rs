use std::fmt;

use serde::{Deserialize, Serialize};

pub const COVER_W: u32 = 650;
pub const COVER_H: u32 = 800;
pub const CHANNELS: usize = 4; // RGBA
pub const COVER_BYTES: usize = COVER_W as usize * COVER_H as usize * CHANNELS;

/// 64 byte header mirrored into LSBs at the start of the payload.
pub const LSB_HEADER_LEN: usize = 64;
/// 32 byte BLAKE3 payload hash mirrored into LSBs after the payload.
pub const LSB_FOOTER_LEN: usize = 32;
pub const LSB_OVERHEAD: usize = LSB_HEADER_LEN + LSB_FOOTER_LEN;

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(into = "u8", try_from = "u8")]
#[repr(u8)]
pub enum LsbBits {
    One = 1,
    Two = 2,
    Four = 4,
}

impl LsbBits {
    pub const fn as_u8(self) -> u8 {
        self as u8
    }

    pub const fn mask(self) -> u8 {
        match self {
            LsbBits::One => 0x01,
            LsbBits::Two => 0x03,
            LsbBits::Four => 0x0F,
        }
    }
}

impl fmt::Display for LsbBits {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_u8())
    }
}

impl From<LsbBits> for u8 {
    fn from(b: LsbBits) -> u8 {
        b.as_u8()
    }
}

impl TryFrom<u8> for LsbBits {
    type Error = crate::Error;
    fn try_from(v: u8) -> Result<Self, Self::Error> {
        match v {
            1 => Ok(LsbBits::One),
            2 => Ok(LsbBits::Two),
            4 => Ok(LsbBits::Four),
            other => Err(crate::Error::other(format!(
                "invalid LsbBits value: {other}"
            ))),
        }
    }
}

/// Raw LSB capacity in bytes for the cover image at the given bit depth.
pub const fn raw_capacity_bytes(bits: LsbBits) -> usize {
    let b = match bits {
        LsbBits::One => 1,
        LsbBits::Two => 2,
        LsbBits::Four => 4,
    };
    COVER_BYTES * b / 8
}

/// Bytes of user payload that fit in one monke at the given bit depth.
pub const fn payload_bytes_per_monke(bits: LsbBits) -> usize {
    raw_capacity_bytes(bits) - LSB_OVERHEAD
}

/// Number of monkes required for a file of `total_bytes`. A zero-byte file
/// still takes one monke (which stores an empty payload).
pub const fn monkes_needed(total_bytes: u64, bits: LsbBits) -> u64 {
    let per = payload_bytes_per_monke(bits) as u64;
    if total_bytes == 0 {
        1
    } else {
        total_bytes.div_ceil(per)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cover_geometry() {
        assert_eq!(COVER_BYTES, 650 * 800 * 4);
    }

    #[test]
    fn raw_capacity_per_depth() {
        assert_eq!(raw_capacity_bytes(LsbBits::One), 260_000);
        assert_eq!(raw_capacity_bytes(LsbBits::Two), 520_000);
        assert_eq!(raw_capacity_bytes(LsbBits::Four), 1_040_000);
    }

    #[test]
    fn payload_after_overhead() {
        assert_eq!(
            payload_bytes_per_monke(LsbBits::Two),
            520_000 - LSB_OVERHEAD
        );
    }

    #[test]
    fn monke_counts() {
        assert_eq!(monkes_needed(0, LsbBits::Two), 1);
        let per = payload_bytes_per_monke(LsbBits::Two) as u64;
        assert_eq!(monkes_needed(per, LsbBits::Two), 1);
        assert_eq!(monkes_needed(per + 1, LsbBits::Two), 2);
        assert_eq!(monkes_needed(per * 10, LsbBits::Two), 10);
    }

    #[test]
    fn masks() {
        assert_eq!(LsbBits::One.mask(), 0b0000_0001);
        assert_eq!(LsbBits::Two.mask(), 0b0000_0011);
        assert_eq!(LsbBits::Four.mask(), 0b0000_1111);
    }
}
