use pngres_core::{
    encode_bytes, payload_bytes_per_monke, read_range, shard_path, CoverImage, EncodeOptions,
    LsbBits,
};
use std::fs;
use tempfile::tempdir;

fn write_herd(data: &[u8], bits: LsbBits, dir: &std::path::Path) -> pngres_core::Manifest {
    let cover = CoverImage::monke_default().unwrap();
    let opts = EncodeOptions {
        lsb_bits: bits,
        ..Default::default()
    };
    let (manifest, shards) = encode_bytes(data, &cover, opts).unwrap();
    for s in &shards {
        let p = shard_path(dir, s.index, shards.len() as u32);
        fs::write(&p, &s.bytes).unwrap();
    }
    manifest
}

#[test]
fn read_range_across_shards() {
    // Pick a size that spans ~3 shards.
    let per = payload_bytes_per_monke(LsbBits::Two);
    let total = per * 3 + 100;
    let data: Vec<u8> = (0..total).map(|i| ((i * 17) & 0xFF) as u8).collect();

    let dir = tempdir().unwrap();
    let num_shards = data.len().div_ceil(per) as u32;
    let manifest = write_herd(&data, LsbBits::Two, dir.path());
    assert_eq!(manifest.num_monkes, num_shards);

    let shard_for = |i: u32| shard_path(dir.path(), i, manifest.num_monkes);

    // Interior range crossing one boundary
    let range = (per as u64 - 50)..(per as u64 + 50);
    let out = read_range(&manifest, &shard_for, range.clone()).unwrap();
    assert_eq!(out.len() as u64, range.end - range.start);
    assert_eq!(&out[..], &data[range.start as usize..range.end as usize]);

    // Range from middle to end
    let range = (per as u64 * 2 + 5)..manifest.total_size;
    let out = read_range(&manifest, &shard_for, range.clone()).unwrap();
    assert_eq!(&out[..], &data[range.start as usize..range.end as usize]);

    // Zero-length range
    let out = read_range(&manifest, &shard_for, 10..10).unwrap();
    assert!(out.is_empty());

    // Past EOF — clipped to manifest.total_size, should return empty
    let out = read_range(&manifest, &shard_for, manifest.total_size..manifest.total_size + 100)
        .unwrap();
    assert!(out.is_empty());

    // Fully in first shard
    let out = read_range(&manifest, &shard_for, 0..100).unwrap();
    assert_eq!(&out[..], &data[0..100]);
}

#[test]
fn read_range_twenty_random_slices() {
    use std::hash::{BuildHasher, Hasher, RandomState};

    let per = payload_bytes_per_monke(LsbBits::Two);
    let total = per * 4 + 777;
    let data: Vec<u8> = (0..total).map(|i| ((i * 31) & 0xFF) as u8).collect();

    let dir = tempdir().unwrap();
    let manifest = write_herd(&data, LsbBits::Two, dir.path());
    let shard_for = |i: u32| shard_path(dir.path(), i, manifest.num_monkes);

    // Pseudo-random without pulling in rand
    let mut h = RandomState::new().build_hasher();
    for seed in 0..20u64 {
        h.write_u64(seed);
        let v = h.finish();
        let a = (v as usize) % data.len();
        let b = ((v >> 32) as usize) % (data.len() - a) + a;
        let out = read_range(&manifest, &shard_for, a as u64..b as u64).unwrap();
        assert_eq!(&out[..], &data[a..b], "range {}..{}", a, b);
    }
}

#[test]
fn missing_shard_errors_cleanly() {
    let per = payload_bytes_per_monke(LsbBits::Two);
    let data: Vec<u8> = (0..per * 3).map(|i| (i & 0xFF) as u8).collect();

    let dir = tempdir().unwrap();
    let manifest = write_herd(&data, LsbBits::Two, dir.path());
    let shard_for = |i: u32| shard_path(dir.path(), i, manifest.num_monkes);

    // Delete shard #1 and try reading across it.
    fs::remove_file(shard_for(1)).unwrap();
    let err = read_range(
        &manifest,
        &shard_for,
        (per as u64 + 10)..(per as u64 + 50),
    )
    .expect_err("should fail");
    match err {
        pngres_core::Error::ShardPathMissing(_) => {}
        other => panic!("unexpected error: {other}"),
    }
}
