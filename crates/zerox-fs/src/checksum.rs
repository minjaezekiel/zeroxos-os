//! zeroxfs checksums — CRC32 for block integrity.

/// Compute CRC32 of the given bytes (IEEE 802.3 polynomial).
pub fn crc32(data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFFFFFF;
    for &b in data {
        crc ^= b as u32;
        for _ in 0..8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ 0xEDB88320;
            } else {
                crc >>= 1;
            }
        }
    }
    !crc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc32_known_values() {
        // Standard CRC32 test vectors
        assert_eq!(crc32(b""), 0);
        assert_eq!(crc32(b"a"), 0xE8B7BE43);
        assert_eq!(crc32(b"hello"), 0x3610A686);
        assert_eq!(crc32(b"123456789"), 0xCBF43926);
    }
}
