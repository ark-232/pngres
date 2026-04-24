use std::path::PathBuf;

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("I/O: {0}")]
    Io(#[from] std::io::Error),

    #[error("PNG decode: {0}")]
    PngDecode(#[from] png::DecodingError),

    #[error("PNG encode: {0}")]
    PngEncode(#[from] png::EncodingError),

    #[error("unsupported cover image: {0}")]
    UnsupportedCover(String),

    #[error("payload exceeds per-monke capacity: {bytes} B > {capacity} B at {bits} bits")]
    CapacityExceeded {
        bytes: usize,
        capacity: usize,
        bits: u8,
    },

    #[error("corruption: {0}")]
    Corruption(String),

    #[error("chunk CRC mismatch")]
    ChunkCrcMismatch,

    #[error("payload hash mismatch (expected {expected}, got {got})")]
    HashMismatch { expected: String, got: String },

    #[error("missing shard at chunk index {index} of {total}")]
    MissingShard { index: u32, total: u32 },

    #[error("unexpected format version {0}")]
    VersionUnsupported(u8),

    #[error("mkHd magic mismatch")]
    BadMagic,

    #[error("shard path not found: {0}")]
    ShardPathMissing(PathBuf),

    #[error("manifest: {0}")]
    Manifest(String),

    #[error("{0}")]
    Other(String),
}

impl Error {
    pub fn other(msg: impl Into<String>) -> Self {
        Error::Other(msg.into())
    }
}
