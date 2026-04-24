use std::fs::File;
use std::io::{BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use uuid::Uuid;

use crate::capacity::{
    payload_bytes_per_monke, LsbBits, LSB_FOOTER_LEN, LSB_HEADER_LEN,
};
use crate::chunk::{MK_CK, MK_HD, MK_MF};
use crate::cover::CoverImage;
use crate::hashing::{hash_bytes, Hash};
use crate::lsb;
use crate::manifest::{Manifest, MkHeader, FORMAT_VERSION};
use crate::Result;

#[derive(Clone, Debug)]
pub struct EncodeOptions {
    pub lsb_bits: LsbBits,
    pub compression: png::Compression,
    pub filename: String,
    pub mime: Option<String>,
}

impl Default for EncodeOptions {
    fn default() -> Self {
        Self {
            lsb_bits: LsbBits::Two,
            compression: png::Compression::Fast,
            filename: "unnamed".into(),
            mime: None,
        }
    }
}

pub struct Shard {
    pub index: u32,
    pub bytes: Vec<u8>,
    pub hash: Hash,
}

/// Encode the full contents of `reader` into a herd of shards. Holds the file
/// in memory — the v1 path. Good for anything that fits in RAM (multi-GB on
/// modern machines). A streaming variant can be added later if needed.
pub fn encode_reader<R: Read>(
    mut reader: R,
    cover: &CoverImage,
    opts: EncodeOptions,
) -> Result<(Manifest, Vec<Shard>)> {
    let mut input = Vec::new();
    reader.read_to_end(&mut input)?;
    encode_bytes(&input, cover, opts)
}

pub fn encode_bytes(
    input: &[u8],
    cover: &CoverImage,
    opts: EncodeOptions,
) -> Result<(Manifest, Vec<Shard>)> {
    let file_id = Uuid::now_v7();
    let per_u32 = payload_bytes_per_monke(opts.lsb_bits) as u32;
    let per = per_u32 as usize;
    let total_size = input.len() as u64;

    let num_monkes: u32 = if input.is_empty() {
        1
    } else {
        input.len().div_ceil(per) as u32
    };

    let mime = opts.mime.clone().unwrap_or_else(|| {
        infer::get(input)
            .map(|k| k.mime_type().to_owned())
            .unwrap_or_else(|| "application/octet-stream".to_owned())
    });

    let file_hash = hash_bytes(input);
    let created_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    // Slice payloads up front so we can hash + build the manifest first.
    let shard_payloads: Vec<&[u8]> = if input.is_empty() {
        vec![&[]]
    } else {
        (0..num_monkes)
            .map(|i| {
                let off = (i as usize) * per;
                let end = (off + per).min(input.len());
                &input[off..end]
            })
            .collect()
    };
    let shard_hashes: Vec<Hash> = shard_payloads.iter().map(|s| hash_bytes(s)).collect();

    let manifest = Manifest {
        file_id,
        filename: opts.filename.clone(),
        mime: mime.clone(),
        total_size,
        num_monkes,
        lsb_bits: opts.lsb_bits,
        payload_per_monke: per_u32,
        file_hash,
        shard_hashes: shard_hashes.clone(),
        created_at,
        format_version: FORMAT_VERSION,
    };
    let manifest_json = manifest.to_json_bytes()?;

    let mut shards: Vec<Shard> = Vec::with_capacity(num_monkes as usize);

    #[cfg(feature = "rayon")]
    let iter: Vec<(u32, MkHeader, Hash, Vec<u8>)> = {
        use rayon::prelude::*;
        (0..num_monkes)
            .into_par_iter()
            .map(|i| build_shard_inputs(i, &shard_payloads, &shard_hashes, file_id, num_monkes, total_size, per_u32, opts.lsb_bits))
            .collect()
    };
    #[cfg(not(feature = "rayon"))]
    let iter: Vec<(u32, MkHeader, Hash, Vec<u8>)> = (0..num_monkes)
        .map(|i| build_shard_inputs(i, &shard_payloads, &shard_hashes, file_id, num_monkes, total_size, per_u32, opts.lsb_bits))
        .collect();

    #[cfg(feature = "rayon")]
    let rendered: Vec<Result<(u32, Vec<u8>, Hash)>> = {
        use rayon::prelude::*;
        iter.into_par_iter()
            .map(|(idx, header, shard_hash, lsb_buf)| {
                let mut pixels: Vec<u8> = cover.raw_rgba.to_vec();
                lsb::embed(&mut pixels, &lsb_buf, opts.lsb_bits);
                let header_bytes = header.to_bytes();
                let png_bytes = encode_png_with_chunks(
                    &pixels,
                    &header_bytes,
                    &manifest_json,
                    &shard_hash,
                    opts.compression,
                )?;
                Ok((idx, png_bytes, shard_hash))
            })
            .collect()
    };
    #[cfg(not(feature = "rayon"))]
    let rendered: Vec<Result<(u32, Vec<u8>, Hash)>> = iter
        .into_iter()
        .map(|(idx, header, shard_hash, lsb_buf)| {
            let mut pixels: Vec<u8> = cover.raw_rgba.to_vec();
            lsb::embed(&mut pixels, &lsb_buf, opts.lsb_bits);
            let header_bytes = header.to_bytes();
            let png_bytes = encode_png_with_chunks(
                &pixels,
                &header_bytes,
                &manifest_json,
                &shard_hash,
                opts.compression,
            )?;
            Ok((idx, png_bytes, shard_hash))
        })
        .collect();

    for res in rendered {
        let (idx, png_bytes, shard_hash) = res?;
        shards.push(Shard {
            index: idx,
            bytes: png_bytes,
            hash: shard_hash,
        });
    }
    shards.sort_by_key(|s| s.index);

    Ok((manifest, shards))
}

fn build_shard_inputs(
    i: u32,
    shard_payloads: &[&[u8]],
    shard_hashes: &[Hash],
    file_id: Uuid,
    num_monkes: u32,
    total_size: u64,
    per: u32,
    lsb_bits: LsbBits,
) -> (u32, MkHeader, Hash, Vec<u8>) {
    let payload = shard_payloads[i as usize];
    let shard_hash = shard_hashes[i as usize];
    let header = MkHeader {
        format_version: FORMAT_VERSION,
        lsb_bits,
        flags: 0,
        file_id,
        chunk_index: i,
        total_chunks: num_monkes,
        byte_offset: (i as u64) * per as u64,
        payload_length: payload.len() as u32,
        total_file_size: total_size,
    };
    let header_bytes = header.to_bytes();
    let mut lsb_buf =
        Vec::with_capacity(LSB_HEADER_LEN + payload.len() + LSB_FOOTER_LEN);
    lsb_buf.extend_from_slice(&header_bytes);
    lsb_buf.extend_from_slice(payload);
    lsb_buf.extend_from_slice(&shard_hash);
    (i, header, shard_hash, lsb_buf)
}

fn encode_png_with_chunks(
    rgba: &[u8],
    mkhd: &[u8],
    mkmf: &[u8],
    mkck: &[u8; 32],
    compression: png::Compression,
) -> Result<Vec<u8>> {
    use crate::capacity::{COVER_H, COVER_W};

    let mut out = Vec::with_capacity(rgba.len() / 2 + 4096);
    {
        let mut encoder = png::Encoder::new(&mut out, COVER_W, COVER_H);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_compression(compression);

        let mut writer = encoder.write_header()?;
        writer.write_chunk(png::chunk::ChunkType(MK_HD), mkhd)?;
        writer.write_chunk(png::chunk::ChunkType(MK_MF), mkmf)?;
        writer.write_image_data(rgba)?;
        writer.write_chunk(png::chunk::ChunkType(MK_CK), mkck)?;
    }
    Ok(out)
}

pub fn encode_file_to_dir<P1: AsRef<Path>, P2: AsRef<Path>>(
    input: P1,
    out_dir: P2,
    cover: &CoverImage,
    mut opts: EncodeOptions,
) -> Result<Manifest> {
    let input_path = input.as_ref();
    if opts.filename == "unnamed" {
        if let Some(name) = input_path.file_name().and_then(|s| s.to_str()) {
            opts.filename = name.to_owned();
        }
    }
    let file = File::open(input_path)?;
    let (manifest, shards) = encode_reader(BufReader::new(file), cover, opts)?;

    std::fs::create_dir_all(out_dir.as_ref())?;
    for shard in &shards {
        let path = shard_path(out_dir.as_ref(), shard.index, manifest.num_monkes);
        let mut f = File::create(&path)?;
        f.write_all(&shard.bytes)?;
        f.flush()?;
    }
    // Convenience sidecar (not used by the decoder).
    let manifest_path = out_dir.as_ref().join("manifest.json");
    std::fs::write(&manifest_path, manifest.to_json_pretty()?)?;
    Ok(manifest)
}

pub fn shard_path(dir: &Path, index: u32, total: u32) -> PathBuf {
    let width = total.saturating_sub(1).to_string().len().max(3);
    dir.join(format!("monke_{:0width$}.png", index + 1, width = width))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shard_path_widths() {
        let dir = Path::new("/tmp/h");
        assert_eq!(shard_path(dir, 0, 7), PathBuf::from("/tmp/h/monke_001.png"));
        assert_eq!(
            shard_path(dir, 41, 9999),
            PathBuf::from("/tmp/h/monke_0042.png")
        );
    }
}
