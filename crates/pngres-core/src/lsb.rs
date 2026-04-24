//! LSB embed/extract over a raw pixel byte slice.
//!
//! We treat the pixel buffer as a flat sequence of bytes (R, G, B, A, R, G, B, A, ...)
//! and pack `bits` bits of payload into each byte's low bits, MSB-first per
//! payload byte.

use crate::capacity::LsbBits;

/// Embed `payload` into the low `bits` of each byte in `pixels`.
/// Pixel bytes past `payload.len() * 8 / bits` are left untouched.
pub fn embed(pixels: &mut [u8], payload: &[u8], bits: LsbBits) {
    let required_bits = payload.len() * 8;
    let capacity_bits = pixels.len() * (bits.as_u8() as usize);
    assert!(
        required_bits <= capacity_bits,
        "lsb::embed: payload {} bits > capacity {} bits",
        required_bits,
        capacity_bits
    );

    let b = bits.as_u8() as usize;
    let mask = bits.mask();
    let mut payload_bits_consumed: usize = 0;

    for px in pixels.iter_mut() {
        if payload_bits_consumed >= required_bits {
            break;
        }
        // Gather the next `b` payload bits (MSB-first per payload byte)
        let mut chunk: u8 = 0;
        for _ in 0..b {
            let total_bit = payload_bits_consumed;
            let bit = if total_bit < required_bits {
                let byte_idx = total_bit / 8;
                let bit_in_byte = 7 - (total_bit % 8);
                (payload[byte_idx] >> bit_in_byte) & 1
            } else {
                0
            };
            chunk = (chunk << 1) | bit;
            payload_bits_consumed += 1;
        }
        *px = (*px & !mask) | (chunk & mask);
    }
}

/// Extract `payload_len` bytes from the low `bits` of each byte in `pixels`.
pub fn extract(pixels: &[u8], payload_len: usize, bits: LsbBits) -> Vec<u8> {
    let required_bits = payload_len * 8;
    let capacity_bits = pixels.len() * (bits.as_u8() as usize);
    assert!(
        required_bits <= capacity_bits,
        "lsb::extract: want {} bits but capacity is {} bits",
        required_bits,
        capacity_bits
    );

    let b = bits.as_u8() as usize;
    let mask = bits.mask();
    let mut out = vec![0u8; payload_len];
    let mut written_bits: usize = 0;

    for px in pixels.iter() {
        if written_bits >= required_bits {
            break;
        }
        let chunk = px & mask;
        for i in 0..b {
            if written_bits >= required_bits {
                break;
            }
            let bit = (chunk >> (b - 1 - i)) & 1;
            let byte_idx = written_bits / 8;
            let bit_in_byte = 7 - (written_bits % 8);
            out[byte_idx] |= bit << bit_in_byte;
            written_bits += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capacity::{payload_bytes_per_monke, LsbBits, COVER_BYTES, LSB_OVERHEAD};

    fn roundtrip(payload: &[u8], bits: LsbBits) {
        let mut pixels = vec![0xAAu8; COVER_BYTES];
        embed(&mut pixels, payload, bits);
        let out = extract(&pixels, payload.len(), bits);
        assert_eq!(&out[..], payload);
    }

    #[test]
    fn roundtrip_one_bit_small() {
        roundtrip(&[0xDE, 0xAD, 0xBE, 0xEF, 0xCA, 0xFE, 0xBA, 0xBE], LsbBits::One);
    }

    #[test]
    fn roundtrip_two_bits_medium() {
        let data: Vec<u8> = (0..10_000u32).map(|v| (v & 0xFF) as u8).collect();
        roundtrip(&data, LsbBits::Two);
    }

    #[test]
    fn roundtrip_four_bits_larger() {
        let data: Vec<u8> = (0..50_000u32).map(|v| ((v * 7) & 0xFF) as u8).collect();
        roundtrip(&data, LsbBits::Four);
    }

    #[test]
    fn empty_payload_no_writes() {
        let mut pixels = vec![0xAAu8; 100];
        let before = pixels.clone();
        embed(&mut pixels, &[], LsbBits::Two);
        assert_eq!(pixels, before);
        let out = extract(&pixels, 0, LsbBits::Two);
        assert!(out.is_empty());
    }

    #[test]
    fn max_capacity_each_depth() {
        for bits in [LsbBits::One, LsbBits::Two, LsbBits::Four] {
            let cap = payload_bytes_per_monke(bits) + LSB_OVERHEAD;
            let data: Vec<u8> = (0..cap).map(|i| (i & 0xFF) as u8).collect();
            let mut pixels = vec![0u8; COVER_BYTES];
            embed(&mut pixels, &data, bits);
            let out = extract(&pixels, data.len(), bits);
            assert_eq!(out, data);
        }
    }

    #[test]
    fn untouched_bytes_past_payload() {
        let mut pixels = vec![0xFFu8; 1024];
        embed(&mut pixels, b"hi", LsbBits::Two);
        // Only first 8 bytes (2 bytes * 8 bits / 2 bits per byte) were touched.
        assert!(pixels[8..].iter().all(|&b| b == 0xFF));
    }
}
