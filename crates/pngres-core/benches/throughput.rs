use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use pngres_core::{decode_shard_bytes, encode_bytes, CoverImage, EncodeOptions, LsbBits};

fn sample_bytes(size: usize) -> Vec<u8> {
    (0..size).map(|i| ((i.wrapping_mul(2654435761)) & 0xFF) as u8).collect()
}

fn bench_encode(c: &mut Criterion) {
    let cover = CoverImage::monke_default().unwrap();
    let mut group = c.benchmark_group("encode");
    for (label, size) in [
        ("64_KiB", 64 * 1024),
        ("1_MiB", 1 * 1024 * 1024),
        ("10_MiB", 10 * 1024 * 1024),
    ] {
        let data = sample_bytes(size);
        group.throughput(Throughput::Bytes(size as u64));
        group.bench_with_input(BenchmarkId::new("2-bit", label), &data, |b, data| {
            b.iter(|| {
                let opts = EncodeOptions {
                    lsb_bits: LsbBits::Two,
                    ..Default::default()
                };
                let (_m, shards) = encode_bytes(black_box(data), &cover, opts).unwrap();
                black_box(shards.len())
            })
        });
    }
    group.finish();
}

fn bench_decode(c: &mut Criterion) {
    let cover = CoverImage::monke_default().unwrap();
    let mut group = c.benchmark_group("decode");
    for (label, size) in [
        ("64_KiB", 64 * 1024),
        ("1_MiB", 1 * 1024 * 1024),
        ("10_MiB", 10 * 1024 * 1024),
    ] {
        let data = sample_bytes(size);
        let opts = EncodeOptions {
            lsb_bits: LsbBits::Two,
            ..Default::default()
        };
        let (_m, shards) = encode_bytes(&data, &cover, opts).unwrap();
        group.throughput(Throughput::Bytes(size as u64));
        group.bench_with_input(BenchmarkId::new("2-bit", label), &shards, |b, shards| {
            b.iter(|| {
                let mut total = 0u64;
                for s in shards {
                    let d = decode_shard_bytes(black_box(&s.bytes)).unwrap();
                    total += d.payload.len() as u64;
                }
                black_box(total)
            })
        });
    }
    group.finish();
}

criterion_group!(name = benches; config = Criterion::default().sample_size(10); targets = bench_encode, bench_decode);
criterion_main!(benches);
