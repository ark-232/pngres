//! PNG chunk-level helpers for reading our private `mk*` ancillary chunks
//! without going through a full decode.

use byteorder::{BigEndian, ReadBytesExt};
use std::io::{Cursor, Read};

use crate::{Error, Result};

/// Our three private ancillary chunk types. PNG naming convention:
/// - byte 0 lowercase → ancillary (viewer may ignore)
/// - byte 1 lowercase → private (not registered)
/// - byte 2 uppercase → reserved (must be uppercase)
/// - byte 3 lowercase → unsafe to copy (if a tool edits pixels, drop it)
pub const MK_HD: [u8; 4] = *b"mkHd";
pub const MK_MF: [u8; 4] = *b"mkMf";
pub const MK_CK: [u8; 4] = *b"mkCk";

pub const PNG_SIG: [u8; 8] = [137, 80, 78, 71, 13, 10, 26, 10];

/// Iterate every chunk in `bytes`, without decoding IDAT pixel data.
/// Returns `(chunk_type, data)` for each chunk in document order.
pub fn iter_chunks(bytes: &[u8]) -> Result<Vec<([u8; 4], Vec<u8>)>> {
    if bytes.len() < 8 || bytes[..8] != PNG_SIG {
        return Err(Error::Corruption("bad PNG signature".into()));
    }
    let mut out = Vec::new();
    let mut cursor = Cursor::new(&bytes[8..]);
    loop {
        let len = match cursor.read_u32::<BigEndian>() {
            Ok(v) => v as usize,
            Err(_) => break,
        };
        let mut ctype = [0u8; 4];
        if cursor.read_exact(&mut ctype).is_err() {
            break;
        }
        let mut data = vec![0u8; len];
        cursor.read_exact(&mut data).map_err(|_| {
            Error::Corruption(format!("unexpected EOF reading chunk {:?}", ctype))
        })?;
        let got_crc = cursor
            .read_u32::<BigEndian>()
            .map_err(|_| Error::Corruption("missing chunk CRC".into()))?;
        let want_crc = crc32(ctype, &data);
        if got_crc != want_crc {
            return Err(Error::ChunkCrcMismatch);
        }
        let is_iend = ctype == *b"IEND";
        out.push((ctype, data));
        if is_iend {
            break;
        }
    }
    Ok(out)
}

/// Find the first chunk of the given type in `bytes`.
pub fn find_chunk(bytes: &[u8], want: [u8; 4]) -> Result<Option<Vec<u8>>> {
    if bytes.len() < 8 || bytes[..8] != PNG_SIG {
        return Err(Error::Corruption("bad PNG signature".into()));
    }
    let mut cursor = Cursor::new(&bytes[8..]);
    loop {
        let len = match cursor.read_u32::<BigEndian>() {
            Ok(v) => v as usize,
            Err(_) => return Ok(None),
        };
        let mut ctype = [0u8; 4];
        if cursor.read_exact(&mut ctype).is_err() {
            return Ok(None);
        }
        let pos_data_start = cursor.position();
        if ctype == want {
            let mut data = vec![0u8; len];
            cursor.read_exact(&mut data).map_err(|_| {
                Error::Corruption(format!("unexpected EOF reading chunk {:?}", ctype))
            })?;
            return Ok(Some(data));
        }
        // skip data + crc
        let skip = (len as u64) + 4;
        cursor.set_position(pos_data_start + skip);
        if ctype == *b"IEND" {
            return Ok(None);
        }
    }
}

pub fn crc32(chunk_type: [u8; 4], data: &[u8]) -> u32 {
    let mut hasher = crc32fast::Hasher::new();
    hasher.update(&chunk_type);
    hasher.update(data);
    hasher.finalize()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunk_type_bytes_ok() {
        assert_eq!(MK_HD, *b"mkHd");
        assert_eq!(MK_MF, *b"mkMf");
        assert_eq!(MK_CK, *b"mkCk");
    }

    #[test]
    fn iend_crc_known() {
        // CRC32 of "IEND" + no data = 0xAE426082
        assert_eq!(crc32(*b"IEND", &[]), 0xAE426082);
    }
}
