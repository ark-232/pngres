use std::io::Write;
use std::ops::Range;
use std::path::{Path, PathBuf};

use crate::capacity::{LsbBits, LSB_FOOTER_LEN, LSB_HEADER_LEN};
use crate::chunk::{find_chunk, MK_HD, MK_MF};
use crate::hashing::{hash_bytes, hash_to_hex, Hash};
use crate::lsb;
use crate::manifest::{Manifest, MkHeader, MK_HEADER_LEN};
use crate::{Error, Result};

pub struct DecodedShard {
    pub header: MkHeader,
    pub payload: Vec<u8>,
    pub payload_hash: Hash,
}

pub fn read_manifest_from_shard<P: AsRef<Path>>(path: P) -> Result<Manifest> {
    let bytes = std::fs::read(path)?;
    read_manifest_from_shard_bytes(&bytes)
}

pub fn read_manifest_from_shard_bytes(bytes: &[u8]) -> Result<Manifest> {
    let mkmf =
        find_chunk(bytes, MK_MF)?.ok_or_else(|| Error::Corruption("no mkMf chunk".into()))?;
    Manifest::from_json_bytes(&mkmf)
}

pub fn decode_shard_bytes(bytes: &[u8]) -> Result<DecodedShard> {
    // 1. Try to read the mkHd ancillary chunk (fast path).
    let mkhd_chunk = find_chunk(bytes, MK_HD)?;
    let pixels = decode_png_pixels(bytes)?;

    let header = if let Some(hb) = mkhd_chunk {
        MkHeader::from_bytes(&hb)?
    } else {
        // Fallback: the mkHd is also mirrored into the first 64 bytes of the
        // LSB payload. We don't know the bit depth up front, so try each.
        let mut found: Option<MkHeader> = None;
        for b in [LsbBits::Two, LsbBits::One, LsbBits::Four] {
            let mirror = lsb::extract(&pixels, MK_HEADER_LEN, b);
            if let Ok(h) = MkHeader::from_bytes(&mirror) {
                if h.lsb_bits == b {
                    found = Some(h);
                    break;
                }
            }
        }
        found.ok_or_else(|| {
            Error::Corruption("mkHd missing and LSB mirror unreadable".into())
        })?
    };

    let total_lsb_bytes =
        LSB_HEADER_LEN + header.payload_length as usize + LSB_FOOTER_LEN;
    let lsb_buf = lsb::extract(&pixels, total_lsb_bytes, header.lsb_bits);

    // Verify the mirror matches the header we trusted (cheap tamper check).
    let mirror_hdr = MkHeader::from_bytes(&lsb_buf[..MK_HEADER_LEN])?;
    if mirror_hdr != header {
        return Err(Error::Corruption(
            "ancillary mkHd disagrees with LSB mirror".into(),
        ));
    }

    let payload_start = LSB_HEADER_LEN;
    let payload_end = payload_start + header.payload_length as usize;
    let payload = lsb_buf[payload_start..payload_end].to_vec();

    let mut footer_hash: Hash = [0u8; 32];
    footer_hash.copy_from_slice(&lsb_buf[payload_end..payload_end + LSB_FOOTER_LEN]);
    let got = hash_bytes(&payload);
    if got != footer_hash {
        return Err(Error::HashMismatch {
            expected: hash_to_hex(&footer_hash),
            got: hash_to_hex(&got),
        });
    }

    Ok(DecodedShard {
        header,
        payload,
        payload_hash: footer_hash,
    })
}

fn decode_png_pixels(bytes: &[u8]) -> Result<Vec<u8>> {
    let decoder = png::Decoder::new(bytes);
    let mut reader = decoder.read_info()?;
    let mut buf = vec![0u8; reader.output_buffer_size()];
    reader.next_frame(&mut buf)?;
    Ok(buf)
}

/// Reassemble every shard in `shard_paths` into `out`, verifying every hash.
/// Shards may be in any order; they're sorted by chunk_index.
pub fn reassemble<P: AsRef<Path>, W: Write>(
    shard_paths: &[P],
    out: &mut W,
) -> Result<Manifest> {
    if shard_paths.is_empty() {
        return Err(Error::Corruption("no shards to reassemble".into()));
    }
    let manifest = read_manifest_from_shard(&shard_paths[0])?;

    let mut decoded: Vec<DecodedShard> = Vec::with_capacity(shard_paths.len());
    for p in shard_paths {
        let bytes = std::fs::read(p)?;
        decoded.push(decode_shard_bytes(&bytes)?);
    }
    decoded.sort_by_key(|s| s.header.chunk_index);

    if decoded.len() as u32 != manifest.num_monkes {
        return Err(Error::MissingShard {
            index: decoded.len() as u32,
            total: manifest.num_monkes,
        });
    }
    for (expected, s) in decoded.iter().enumerate() {
        if s.header.chunk_index as usize != expected {
            return Err(Error::MissingShard {
                index: expected as u32,
                total: manifest.num_monkes,
            });
        }
        if s.header.file_id != manifest.file_id {
            return Err(Error::Corruption(format!(
                "shard {} has a different file_id",
                s.header.chunk_index
            )));
        }
    }

    let mut file_hasher = blake3::Hasher::new();
    for s in &decoded {
        out.write_all(&s.payload)?;
        file_hasher.update(&s.payload);
    }
    let got: Hash = *file_hasher.finalize().as_bytes();
    if got != manifest.file_hash {
        return Err(Error::HashMismatch {
            expected: hash_to_hex(&manifest.file_hash),
            got: hash_to_hex(&got),
        });
    }
    Ok(manifest)
}

/// Random access: return exactly the bytes in `range` from the file represented
/// by `manifest`, touching only the shards required to cover the range.
/// `shard_for(index)` returns the filesystem path of the shard at that index.
pub fn read_range(
    manifest: &Manifest,
    shard_for: &dyn Fn(u32) -> PathBuf,
    range: Range<u64>,
) -> Result<Vec<u8>> {
    if range.start >= manifest.total_size || range.start >= range.end {
        return Ok(Vec::new());
    }
    let end = range.end.min(manifest.total_size);
    let per = manifest.payload_per_monke as u64;
    let first = (range.start / per) as u32;
    let last = ((end - 1) / per) as u32;

    let mut out = Vec::with_capacity((end - range.start) as usize);
    for idx in first..=last {
        let shard_path = shard_for(idx);
        if !shard_path.exists() {
            return Err(Error::ShardPathMissing(shard_path));
        }
        let bytes = std::fs::read(&shard_path)?;
        let shard = decode_shard_bytes(&bytes)?;
        if shard.header.chunk_index != idx {
            return Err(Error::Corruption(format!(
                "shard at {} claims chunk_index {}",
                shard_path.display(),
                shard.header.chunk_index
            )));
        }
        let shard_start = (idx as u64) * per;
        let shard_end = shard_start + shard.payload.len() as u64;
        let slice_start = range.start.max(shard_start) - shard_start;
        let slice_end = end.min(shard_end) - shard_start;
        out.extend_from_slice(&shard.payload[slice_start as usize..slice_end as usize]);
    }
    Ok(out)
}
