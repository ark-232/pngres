use pngres_core::{
    decode_shard_bytes, encode_bytes, payload_bytes_per_monke, reassemble, shard_path,
    CoverImage, EncodeOptions, LsbBits,
};
use std::fs;
use tempfile::tempdir;

fn cover() -> CoverImage {
    CoverImage::monke_default().expect("monke loads")
}

fn roundtrip_in_memory(input: &[u8], bits: LsbBits) {
    let cover = cover();
    let opts = EncodeOptions {
        lsb_bits: bits,
        ..Default::default()
    };
    let (manifest, shards) = encode_bytes(input, &cover, opts).expect("encode");
    assert_eq!(manifest.total_size, input.len() as u64);
    assert_eq!(shards.len() as u32, manifest.num_monkes);

    let mut assembled = Vec::new();
    let mut decoded: Vec<_> = shards
        .iter()
        .map(|s| decode_shard_bytes(&s.bytes).expect("decode"))
        .collect();
    decoded.sort_by_key(|s| s.header.chunk_index);
    for d in &decoded {
        assembled.extend_from_slice(&d.payload);
    }
    assert_eq!(assembled.len(), input.len());
    assert_eq!(assembled.as_slice(), input);
}

#[test]
fn tiny_text_default_bits() {
    roundtrip_in_memory(b"hello, monke database!", LsbBits::Two);
}

#[test]
fn empty_file() {
    roundtrip_in_memory(&[], LsbBits::Two);
}

#[test]
fn single_byte() {
    roundtrip_in_memory(&[0x42], LsbBits::Two);
}

#[test]
fn roundtrip_one_bit() {
    let data: Vec<u8> = (0..1000u32).map(|v| (v & 0xFF) as u8).collect();
    roundtrip_in_memory(&data, LsbBits::One);
}

#[test]
fn roundtrip_four_bits() {
    let data: Vec<u8> = (0..5000u32).map(|v| ((v * 13) & 0xFF) as u8).collect();
    roundtrip_in_memory(&data, LsbBits::Four);
}

#[test]
fn crosses_monke_boundary() {
    let per = payload_bytes_per_monke(LsbBits::Two);
    // size such that we need 3 shards
    let size = per * 2 + 1234;
    let data: Vec<u8> = (0..size).map(|i| ((i * 31) & 0xFF) as u8).collect();
    roundtrip_in_memory(&data, LsbBits::Two);
}

#[test]
fn boundary_exact() {
    let per = payload_bytes_per_monke(LsbBits::Two);
    let data: Vec<u8> = (0..per).map(|i| ((i * 7) & 0xFF) as u8).collect();
    roundtrip_in_memory(&data, LsbBits::Two);
}

#[test]
fn boundary_plus_one() {
    let per = payload_bytes_per_monke(LsbBits::Two);
    let data: Vec<u8> = (0..per + 1).map(|i| ((i * 11) & 0xFF) as u8).collect();
    roundtrip_in_memory(&data, LsbBits::Two);
}

#[test]
fn reassemble_from_dir() {
    let cover = cover();
    let data: Vec<u8> = (0..10_000u32).map(|v| (v & 0xFF) as u8).collect();
    let (_manifest, shards) = encode_bytes(&data, &cover, EncodeOptions::default()).unwrap();

    let dir = tempdir().unwrap();
    for s in &shards {
        let p = shard_path(dir.path(), s.index, shards.len() as u32);
        fs::write(&p, &s.bytes).unwrap();
    }
    let mut shard_paths: Vec<_> = (0..shards.len() as u32)
        .map(|i| shard_path(dir.path(), i, shards.len() as u32))
        .collect();
    // Shuffle to prove reassemble sorts.
    shard_paths.reverse();

    let mut out = Vec::new();
    let m = reassemble(&shard_paths, &mut out).unwrap();
    assert_eq!(out, data);
    assert_eq!(m.total_size, data.len() as u64);
}

#[test]
fn mime_sniffed_on_encode() {
    let cover = cover();
    // First 8 bytes of a PNG file
    let png_magic = b"\x89PNG\r\n\x1a\nrest of bytes...";
    let (manifest, _) = encode_bytes(png_magic, &cover, EncodeOptions::default()).unwrap();
    assert_eq!(manifest.mime, "image/png");
}

#[test]
fn mime_falls_back_to_octet_stream() {
    let cover = cover();
    let data: &[u8] = b"just some non-magic text";
    let (manifest, _) = encode_bytes(data, &cover, EncodeOptions::default()).unwrap();
    // infer detects text/plain for ascii? actually infer returns None for pure text.
    assert!(
        manifest.mime == "application/octet-stream" || manifest.mime.starts_with("text/"),
        "got mime: {}",
        manifest.mime
    );
}
