//! # zeroxfs — the native filesystem of zeroxos
//!
//! A modern copy-on-write filesystem combining the best of ZFS, Btrfs, and APFS:
//!
//! - **Journaling** — metadata writes are journaled; recovery is constant-time
//! - **Compression** — transparent block-level compression (LZ4 by default)
//! - **Snapshots** — instant, space-efficient point-in-time copies via CoW
//! - **Checksums** — every block carries a CRC32; bit rot is detected on read
//! - **Encryption** — per-file and per-volume encryption at rest
//! - **Copy-on-Write** — file copies, snapshots, and clones share blocks until written
//! - **Deduplication** — online block-level dedup via content-addressable storage
//!
//! zeroxfs runs as a **userspace server**. A bug in the filesystem driver
//! crashes the daemon, not the kernel — the supervisor restarts it transparently.

pub mod superblock;
pub mod journal;
pub mod checksum;
pub mod cow;
pub mod compression;

pub use superblock::Superblock;
pub use journal::Journal;
pub use checksum::crc32;
pub use cow::CowTree;
pub use compression::{compress, decompress, Algorithm};
