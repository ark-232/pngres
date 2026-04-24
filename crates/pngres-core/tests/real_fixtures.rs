//! Roundtrip each fixture file type through the full encode/decode pipeline.
//! Verifies bytes match AND SHA-256 match AND MIME is detected plausibly.

use pngres_core::{
    decode_shard_bytes, encode_bytes, hash_bytes, hash_to_hex, read_range, shard_path, CoverImage,
    EncodeOptions, LsbBits,
};
use std::fs;
use std::path::PathBuf;
use tempfile::tempdir;

fn fixtures_dir() -> PathBuf {
    // crates/pngres-core/tests/real_fixtures.rs -> ../../../tests/fixtures
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("tests")
        .join("fixtures")
}

fn cover() -> CoverImage {
    CoverImage::monke_default().unwrap()
}

fn roundtrip_fixture(name: &str, bits: LsbBits) {
    let path = fixtures_dir().join(name);
    assert!(path.exists(), "missing fixture: {}", path.display());
    let input = fs::read(&path).unwrap();

    let opts = EncodeOptions {
        lsb_bits: bits,
        filename: name.to_owned(),
        ..Default::default()
    };
    let (manifest, shards) = encode_bytes(&input, &cover(), opts).unwrap();

    // Manifest sanity
    assert_eq!(manifest.total_size, input.len() as u64);
    assert_eq!(shards.len() as u32, manifest.num_monkes);
    assert_eq!(manifest.file_hash, hash_bytes(&input));

    // Decode + reassemble via decode_shard_bytes
    let mut decoded: Vec<_> = shards
        .iter()
        .map(|s| decode_shard_bytes(&s.bytes).unwrap())
        .collect();
    decoded.sort_by_key(|s| s.header.chunk_index);
    let mut out = Vec::with_capacity(input.len());
    for d in &decoded {
        out.extend_from_slice(&d.payload);
    }
    assert_eq!(out.len(), input.len(), "size mismatch for {}", name);
    assert_eq!(
        hash_to_hex(&hash_bytes(&out)),
        hash_to_hex(&hash_bytes(&input)),
        "hash mismatch for {} at {}-bit",
        name,
        bits
    );
    assert_eq!(out, input, "byte mismatch for {} at {}-bit", name, bits);

    // Also verify random-access for this fixture when it is non-trivial.
    if input.len() >= 2 {
        let dir = tempdir().unwrap();
        for s in &shards {
            let p = shard_path(dir.path(), s.index, manifest.num_monkes);
            fs::write(&p, &s.bytes).unwrap();
        }
        let shard_for = |i: u32| shard_path(dir.path(), i, manifest.num_monkes);

        let mid = input.len() as u64 / 2;
        let want_end = (mid + 100).min(input.len() as u64);
        let slice = read_range(&manifest, &shard_for, mid..want_end).unwrap();
        assert_eq!(
            slice,
            &input[mid as usize..want_end as usize],
            "random-access mismatch for {}",
            name
        );
    }
}

#[test] fn txt() { roundtrip_fixture("hello.txt", LsbBits::Two); }
#[test] fn rs()  { roundtrip_fixture("lorem.rs", LsbBits::Two); }
#[test] fn json(){ roundtrip_fixture("sample.json", LsbBits::Two); }
#[test] fn empty(){ roundtrip_fixture("empty.bin", LsbBits::Two); }
#[test] fn one_byte(){ roundtrip_fixture("one_byte.bin", LsbBits::Two); }
#[test] fn png()  { roundtrip_fixture("tiny.png", LsbBits::Two); }
#[test] fn jpg()  { roundtrip_fixture("tiny.jpg", LsbBits::Two); }
#[test] fn pdf()  { roundtrip_fixture("tiny.pdf", LsbBits::Two); }
#[test] fn mp3()  { roundtrip_fixture("blip.mp3", LsbBits::Two); }
#[test] fn mp4()  { roundtrip_fixture("clip.mp4", LsbBits::Two); }

// Also test MP4 at every bit depth (it's the hero file).
#[test] fn mp4_one_bit()  { roundtrip_fixture("clip.mp4", LsbBits::One); }
#[test] fn mp4_four_bit() { roundtrip_fixture("clip.mp4", LsbBits::Four); }

// MIME detection sanity for binary formats
#[test]
fn mime_detection() {
    use pngres_core::encode_bytes;
    let cover = cover();
    let cases = [
        ("clip.mp4", "video/mp4"),
        ("blip.mp3", "audio/mpeg"),
        ("tiny.png", "image/png"),
        ("tiny.jpg", "image/jpeg"),
    ];
    for (name, want) in cases {
        let data = fs::read(fixtures_dir().join(name)).unwrap();
        let (m, _) = encode_bytes(&data, &cover, EncodeOptions::default()).unwrap();
        assert!(
            m.mime == want || m.mime.starts_with(want.split('/').next().unwrap()),
            "fixture {}: expected mime starts with {}, got {}",
            name, want, m.mime
        );
    }
}

#[test]
fn random_access_every_byte_range_of_mp4() {
    // Sample 25 random-ish byte ranges across clip.mp4, verify each.
    let data = fs::read(fixtures_dir().join("clip.mp4")).unwrap();
    let cover = cover();
    let (manifest, shards) = encode_bytes(&data, &cover, EncodeOptions::default()).unwrap();

    let dir = tempdir().unwrap();
    for s in &shards {
        let p = shard_path(dir.path(), s.index, manifest.num_monkes);
        fs::write(&p, &s.bytes).unwrap();
    }
    let shard_for = |i: u32| shard_path(dir.path(), i, manifest.num_monkes);

    let total = data.len() as u64;
    let anchors: [u64; 25] = [
        0, 1, 10, 100, 1024, 2048, 4096, 8192, 16_384, 32_768, 50_000, 65_536, 80_000,
        total / 4, total / 3, total / 2, total * 2 / 3, total * 3 / 4,
        total - 1024, total - 512, total - 100, total - 10, total - 2, total - 1, total,
    ];
    for (i, &start) in anchors.iter().enumerate() {
        let end = (start + 2048).min(total);
        if start >= end {
            continue;
        }
        let got = read_range(&manifest, &shard_for, start..end).unwrap();
        assert_eq!(
            got,
            &data[start as usize..end as usize],
            "range #{i} {}..{} mismatch",
            start,
            end
        );
    }
}
