pub const HASH_LEN: usize = 32;
pub type Hash = [u8; HASH_LEN];

pub fn hash_bytes(bytes: &[u8]) -> Hash {
    *blake3::hash(bytes).as_bytes()
}

pub fn hash_to_hex(hash: &Hash) -> String {
    use std::fmt::Write;
    let mut s = String::with_capacity(HASH_LEN * 2);
    for b in hash {
        let _ = write!(s, "{:02x}", b);
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_roundtrip_length() {
        let h = hash_bytes(b"monke");
        let hex = hash_to_hex(&h);
        assert_eq!(hex.len(), 64);
    }

    #[test]
    fn blake3_deterministic() {
        assert_eq!(hash_bytes(b"monke"), hash_bytes(b"monke"));
        assert_ne!(hash_bytes(b"monke"), hash_bytes(b"banana"));
    }
}
