//! zeroxfs superblock — the on-disk anchor of a filesystem.

use alloc::vec::Vec;
use crate::checksum::crc32;

/// Magic number identifying a zeroxfs superblock.
pub const MAGIC: [u8; 8] = *b"ZEROXFS1";

/// On-disk superblock layout.
#[derive(Debug, Clone)]
pub struct Superblock {
    pub magic: [u8; 8],
    pub version: u32,
    pub block_size: u32,
    pub total_blocks: u64,
    pub free_blocks: u64,
    pub root_inode: u64,
    pub journal_start: u64,
    pub journal_blocks: u64,
    pub checksum: u32,
}

impl Superblock {
    pub fn new(block_size: u32, total_blocks: u64) -> Self {
        let mut sb = Self {
            magic: MAGIC,
            version: 1,
            block_size,
            total_blocks,
            free_blocks: total_blocks,
            root_inode: 1,
            journal_start: 1,
            journal_blocks: 1024,
            checksum: 0,
        };
        sb.checksum = sb.compute_checksum();
        sb
    }

    pub fn compute_checksum(&self) -> u32 {
        let bytes = self.bytes_no_checksum();
        crc32(&bytes)
    }

    fn bytes_no_checksum(&self) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(&self.magic);
        v.extend_from_slice(&self.version.to_le_bytes());
        v.extend_from_slice(&self.block_size.to_le_bytes());
        v.extend_from_slice(&self.total_blocks.to_le_bytes());
        v.extend_from_slice(&self.free_blocks.to_le_bytes());
        v.extend_from_slice(&self.root_inode.to_le_bytes());
        v.extend_from_slice(&self.journal_start.to_le_bytes());
        v.extend_from_slice(&self.journal_blocks.to_le_bytes());
        v
    }

    pub fn verify(&self) -> bool {
        self.magic == MAGIC && self.checksum == self.compute_checksum()
    }
}
