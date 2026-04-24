//! pngres-core — encode/decode arbitrary files as herds of monke.png shards.
//!
//! A shard is a valid PNG visually near-identical to the cover image, carrying
//! (a) payload bytes in pixel LSBs and (b) a metadata mirror in private
//! ancillary chunks (`mkHd`/`mkMf`/`mkCk`).

pub mod capacity;
pub mod chunk;
pub mod cover;
pub mod decode;
pub mod encode;
pub mod error;
pub mod hashing;
pub mod lsb;
pub mod manifest;

pub use capacity::{
    monkes_needed, payload_bytes_per_monke, raw_capacity_bytes, LsbBits, CHANNELS, COVER_BYTES,
    COVER_H, COVER_W, LSB_FOOTER_LEN, LSB_HEADER_LEN, LSB_OVERHEAD,
};
pub use chunk::{MK_CK, MK_HD, MK_MF, PNG_SIG};
pub use cover::{CoverImage, MONKE_PNG};
pub use decode::{
    decode_shard_bytes, read_manifest_from_shard, read_manifest_from_shard_bytes, read_range,
    reassemble, DecodedShard,
};
pub use encode::{
    encode_bytes, encode_file_to_dir, encode_reader, shard_path, EncodeOptions, Shard,
};
pub use error::{Error, Result};
pub use hashing::{hash_bytes, hash_to_hex, Hash, HASH_LEN};
pub use manifest::{Manifest, MkHeader, FORMAT_VERSION, MK_HEADER_LEN};
