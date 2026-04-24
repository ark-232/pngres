use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};
use serde::{Deserialize, Serialize};
use std::io::{Cursor, Read, Write};
use uuid::Uuid;

use crate::capacity::LsbBits;
use crate::hashing::Hash;
use crate::{Error, Result};

pub const FORMAT_VERSION: u8 = 1;
pub const MAGIC: &[u8; 8] = b"PNGRES01";
pub const MK_HEADER_LEN: usize = 64;

/// Fixed 64-byte per-shard header. Written both as the `mkHd` ancillary PNG
/// chunk and mirrored into the first 64 bytes of the LSB payload.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MkHeader {
    pub format_version: u8,
    pub lsb_bits: LsbBits,
    pub flags: u16,
    pub file_id: Uuid,
    pub chunk_index: u32,
    pub total_chunks: u32,
    pub byte_offset: u64,
    pub payload_length: u32,
    pub total_file_size: u64,
}

impl MkHeader {
    pub fn to_bytes(&self) -> [u8; MK_HEADER_LEN] {
        let mut buf = [0u8; MK_HEADER_LEN];
        {
            let mut cur = Cursor::new(&mut buf[..]);
            cur.write_all(MAGIC).unwrap();
            cur.write_u8(self.format_version).unwrap();
            cur.write_u8(self.lsb_bits.as_u8()).unwrap();
            cur.write_u16::<BigEndian>(self.flags).unwrap();
            cur.write_all(self.file_id.as_bytes()).unwrap();
            cur.write_u32::<BigEndian>(self.chunk_index).unwrap();
            cur.write_u32::<BigEndian>(self.total_chunks).unwrap();
            cur.write_u64::<BigEndian>(self.byte_offset).unwrap();
            cur.write_u32::<BigEndian>(self.payload_length).unwrap();
            cur.write_u64::<BigEndian>(self.total_file_size).unwrap();
        }
        // CRC over bytes 0..52
        let crc = {
            let mut h = crc32fast::Hasher::new();
            h.update(&buf[..52]);
            h.finalize()
        };
        buf[56..60].copy_from_slice(&crc.to_be_bytes());
        // bytes 60..64 reserved (zero)
        buf
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.len() < MK_HEADER_LEN {
            return Err(Error::Corruption(format!(
                "mkHd too short: {} < {}",
                bytes.len(),
                MK_HEADER_LEN
            )));
        }
        if &bytes[..8] != MAGIC {
            return Err(Error::BadMagic);
        }
        let expected_crc = {
            let mut h = crc32fast::Hasher::new();
            h.update(&bytes[..52]);
            h.finalize()
        };
        let got_crc = u32::from_be_bytes([bytes[56], bytes[57], bytes[58], bytes[59]]);
        if expected_crc != got_crc {
            return Err(Error::ChunkCrcMismatch);
        }
        let mut cur = Cursor::new(&bytes[8..]);
        let format_version = cur.read_u8()?;
        if format_version != FORMAT_VERSION {
            return Err(Error::VersionUnsupported(format_version));
        }
        let lsb_bits_raw = cur.read_u8()?;
        let lsb_bits = LsbBits::try_from(lsb_bits_raw)?;
        let flags = cur.read_u16::<BigEndian>()?;
        let mut file_id_bytes = [0u8; 16];
        cur.read_exact(&mut file_id_bytes)?;
        let file_id = Uuid::from_bytes(file_id_bytes);
        let chunk_index = cur.read_u32::<BigEndian>()?;
        let total_chunks = cur.read_u32::<BigEndian>()?;
        let byte_offset = cur.read_u64::<BigEndian>()?;
        let payload_length = cur.read_u32::<BigEndian>()?;
        let total_file_size = cur.read_u64::<BigEndian>()?;
        Ok(MkHeader {
            format_version,
            lsb_bits,
            flags,
            file_id,
            chunk_index,
            total_chunks,
            byte_offset,
            payload_length,
            total_file_size,
        })
    }
}

/// Top-level, per-file manifest. Duplicated into every shard's `mkMf` chunk
/// as JSON so any single monke can bootstrap the rest.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Manifest {
    pub file_id: Uuid,
    pub filename: String,
    pub mime: String,
    pub total_size: u64,
    pub num_monkes: u32,
    pub lsb_bits: LsbBits,
    pub payload_per_monke: u32,
    pub file_hash: Hash,
    pub shard_hashes: Vec<Hash>,
    pub created_at: u64,
    pub format_version: u8,
}

impl Manifest {
    pub fn to_json_bytes(&self) -> Result<Vec<u8>> {
        serde_json::to_vec(self).map_err(|e| Error::Manifest(e.to_string()))
    }

    pub fn to_json_string(&self) -> Result<String> {
        serde_json::to_string(self).map_err(|e| Error::Manifest(e.to_string()))
    }

    pub fn to_json_pretty(&self) -> Result<String> {
        serde_json::to_string_pretty(self).map_err(|e| Error::Manifest(e.to_string()))
    }

    pub fn from_json_bytes(bytes: &[u8]) -> Result<Self> {
        serde_json::from_slice(bytes).map_err(|e| Error::Manifest(e.to_string()))
    }

    pub fn from_json_str(s: &str) -> Result<Self> {
        serde_json::from_str(s).map_err(|e| Error::Manifest(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mkhd_roundtrip() {
        let hdr = MkHeader {
            format_version: FORMAT_VERSION,
            lsb_bits: LsbBits::Two,
            flags: 0,
            file_id: Uuid::now_v7(),
            chunk_index: 7,
            total_chunks: 42,
            byte_offset: 7 * 500_000,
            payload_length: 500_000,
            total_file_size: 42 * 500_000 - 1,
        };
        let bytes = hdr.to_bytes();
        let back = MkHeader::from_bytes(&bytes).unwrap();
        assert_eq!(hdr, back);
    }

    #[test]
    fn mkhd_bad_magic() {
        let mut bytes = [0u8; MK_HEADER_LEN];
        bytes[..8].copy_from_slice(b"NOTMONKE");
        assert!(matches!(MkHeader::from_bytes(&bytes), Err(Error::BadMagic)));
    }

    #[test]
    fn mkhd_crc_detects_tamper() {
        let hdr = MkHeader {
            format_version: FORMAT_VERSION,
            lsb_bits: LsbBits::One,
            flags: 0,
            file_id: Uuid::now_v7(),
            chunk_index: 0,
            total_chunks: 1,
            byte_offset: 0,
            payload_length: 10,
            total_file_size: 10,
        };
        let mut bytes = hdr.to_bytes();
        bytes[10] ^= 0xFF; // tamper within the CRC'd region
        assert!(matches!(
            MkHeader::from_bytes(&bytes),
            Err(Error::ChunkCrcMismatch)
        ));
    }

    #[test]
    fn manifest_json_roundtrip() {
        let m = Manifest {
            file_id: Uuid::now_v7(),
            filename: "hello.txt".into(),
            mime: "text/plain".into(),
            total_size: 100,
            num_monkes: 1,
            lsb_bits: LsbBits::Two,
            payload_per_monke: 100,
            file_hash: [3u8; 32],
            shard_hashes: vec![[4u8; 32]],
            created_at: 1_700_000_000,
            format_version: FORMAT_VERSION,
        };
        let json = m.to_json_bytes().unwrap();
        let back = Manifest::from_json_bytes(&json).unwrap();
        assert_eq!(m.filename, back.filename);
        assert_eq!(m.lsb_bits, back.lsb_bits);
        assert_eq!(m.shard_hashes, back.shard_hashes);
    }
}
