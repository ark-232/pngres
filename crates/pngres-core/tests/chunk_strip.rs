//! Verify the decoder still works if the `mkHd` ancillary chunk is stripped,
//! via the LSB-mirror fallback path.

use byteorder::{BigEndian, ReadBytesExt};
use pngres_core::{decode_shard_bytes, encode_bytes, CoverImage, EncodeOptions, LsbBits, MK_HD};
use std::io::{Cursor, Read};

/// Return a copy of the PNG bytes with every chunk of type `drop_type` removed.
/// Keeps all other chunks and CRCs intact.
fn strip_chunk(png_bytes: &[u8], drop_type: [u8; 4]) -> Vec<u8> {
    assert_eq!(&png_bytes[..8], &[137, 80, 78, 71, 13, 10, 26, 10]);
    let mut out = Vec::with_capacity(png_bytes.len());
    out.extend_from_slice(&png_bytes[..8]);
    let mut cur = Cursor::new(&png_bytes[8..]);
    loop {
        let len = match cur.read_u32::<BigEndian>() {
            Ok(v) => v as usize,
            Err(_) => break,
        };
        let mut ctype = [0u8; 4];
        if cur.read_exact(&mut ctype).is_err() {
            break;
        }
        let mut data = vec![0u8; len];
        cur.read_exact(&mut data).unwrap();
        let crc = cur.read_u32::<BigEndian>().unwrap();
        if ctype != drop_type {
            out.extend_from_slice(&(len as u32).to_be_bytes());
            out.extend_from_slice(&ctype);
            out.extend_from_slice(&data);
            out.extend_from_slice(&crc.to_be_bytes());
        }
        if ctype == *b"IEND" {
            break;
        }
    }
    out
}

#[test]
fn mkhd_strip_decodes_via_lsb_mirror() {
    let cover = CoverImage::monke_default().unwrap();
    let data: Vec<u8> = (0..5000u32).map(|v| (v & 0xFF) as u8).collect();

    for bits in [LsbBits::One, LsbBits::Two, LsbBits::Four] {
        let opts = EncodeOptions {
            lsb_bits: bits,
            ..Default::default()
        };
        let (_manifest, shards) = encode_bytes(&data, &cover, opts).unwrap();
        let first = &shards[0];

        let stripped = strip_chunk(&first.bytes, MK_HD);

        // Sanity: the stripped PNG should no longer contain the mkHd chunk
        // (search for the 4-byte type identifier).
        assert!(
            !stripped.windows(4).any(|w| w == MK_HD),
            "mkHd unexpectedly present after strip ({} bits)",
            bits
        );

        let decoded = decode_shard_bytes(&stripped)
            .unwrap_or_else(|e| panic!("decode without mkHd failed ({} bits): {e}", bits));
        assert_eq!(decoded.header.chunk_index, 0);
        assert_eq!(decoded.header.lsb_bits, bits);
        assert_eq!(
            decoded.payload.len() as u32,
            decoded.header.payload_length,
            "payload length mismatch at {} bits",
            bits
        );
    }
}
