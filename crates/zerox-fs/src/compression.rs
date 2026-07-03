//! zeroxfs compression — transparent block-level compression.

use alloc::vec::Vec;

/// Compression algorithm.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Algorithm {
    None,
    Lz4,
    Zstd,
}

impl Default for Algorithm {
    fn default() -> Self { Algorithm::Lz4 }
}

/// Compress `src` using the given algorithm. Returns the compressed bytes.
///
/// This is a placeholder implementation that just stores the algorithm tag
/// and length prefix — a real impl would call into lz4 / zstd.
pub fn compress(src: &[u8], algo: Algorithm) -> Vec<u8> {
    let mut out = Vec::new();
    out.push(match algo {
        Algorithm::None => 0u8,
        Algorithm::Lz4 => 1,
        Algorithm::Zstd => 2,
    });
    out.extend_from_slice(&(src.len() as u32).to_le_bytes());
    // For the simulation, store the data as-is (real impl would compress).
    out.extend_from_slice(src);
    out
}

/// Decompress `src` back into the original bytes.
pub fn decompress(src: &[u8]) -> Option<Vec<u8>> {
    if src.len() < 5 { return None; }
    let _algo = src[0];
    let len = u32::from_le_bytes([src[1], src[2], src[3], src[4]]) as usize;
    if src.len() < 5 + len { return None; }
    Some(src[5..5 + len].to_vec())
}
