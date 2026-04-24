use pngres_core::{decode_shard_bytes, encode_bytes, CoverImage, EncodeOptions, LsbBits};
use proptest::prelude::*;

fn reassemble_in_memory(shards: &[pngres_core::Shard]) -> Vec<u8> {
    let mut decoded: Vec<_> = shards
        .iter()
        .map(|s| decode_shard_bytes(&s.bytes).unwrap())
        .collect();
    decoded.sort_by_key(|s| s.header.chunk_index);
    let mut out = Vec::new();
    for d in decoded {
        out.extend_from_slice(&d.payload);
    }
    out
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 16,          // keep fast; encode is ~50ms per shard in debug
        max_shrink_iters: 64,
        ..ProptestConfig::default()
    })]

    #[test]
    fn arbitrary_bytes_roundtrip_two_bits(bytes in proptest::collection::vec(any::<u8>(), 0..32_000)) {
        let cover = CoverImage::monke_default().unwrap();
        let (_m, shards) = encode_bytes(&bytes, &cover, EncodeOptions {
            lsb_bits: LsbBits::Two,
            ..Default::default()
        }).unwrap();
        let out = reassemble_in_memory(&shards);
        prop_assert_eq!(out, bytes);
    }

    #[test]
    fn arbitrary_bytes_roundtrip_one_bit(bytes in proptest::collection::vec(any::<u8>(), 0..10_000)) {
        let cover = CoverImage::monke_default().unwrap();
        let (_m, shards) = encode_bytes(&bytes, &cover, EncodeOptions {
            lsb_bits: LsbBits::One,
            ..Default::default()
        }).unwrap();
        let out = reassemble_in_memory(&shards);
        prop_assert_eq!(out, bytes);
    }

    #[test]
    fn arbitrary_bytes_roundtrip_four_bits(bytes in proptest::collection::vec(any::<u8>(), 0..60_000)) {
        let cover = CoverImage::monke_default().unwrap();
        let (_m, shards) = encode_bytes(&bytes, &cover, EncodeOptions {
            lsb_bits: LsbBits::Four,
            ..Default::default()
        }).unwrap();
        let out = reassemble_in_memory(&shards);
        prop_assert_eq!(out, bytes);
    }
}
