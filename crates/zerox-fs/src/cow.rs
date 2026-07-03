//! zeroxfs copy-on-write B-tree — the core data structure.
//!
//! Files, directories, and snapshots all share the same CoW tree. When a
//! block is modified, only the path from root to that leaf is duplicated;
//! the rest of the tree is shared.

use alloc::vec;
use alloc::vec::Vec;

#[derive(Debug, Clone)]
pub struct CowNode {
    pub id: u64,
    pub parent: Option<u64>,
    pub keys: Vec<u64>,
    pub children: Vec<u64>,
    pub data: Vec<u8>,
    pub refcount: u32,
    pub checksum: u32,
}

impl CowNode {
    pub fn new_leaf(id: u64, data: Vec<u8>) -> Self {
        let checksum = crate::checksum::crc32(&data);
        Self {
            id,
            parent: None,
            keys: Vec::new(),
            children: Vec::new(),
            data,
            refcount: 1,
            checksum,
        }
    }
}

/// A CoW B-tree.
pub struct CowTree {
    pub root: u64,
    pub nodes: Vec<CowNode>,
    pub next_id: u64,
    pub snapshot_count: u32,
}

impl CowTree {
    pub fn new() -> Self {
        let root = CowNode::new_leaf(0, Vec::new());
        Self { root: 0, nodes: vec![root], next_id: 1, snapshot_count: 0 }
    }

    /// Create a snapshot — O(1) because the root is just refcount-bumped.
    pub fn snapshot(&mut self) -> u64 {
        self.snapshot_count += 1;
        let snap_root = self.root;
        if let Some(node) = self.nodes.iter_mut().find(|n| n.id == self.root) {
            node.refcount += 1;
        }
        log::info!("[fs:cow] snapshot #{} created (root={})", self.snapshot_count, snap_root);
        snap_root
    }

    /// Modify a leaf — duplicates the path from root to leaf (CoW).
    pub fn write(&mut self, leaf_id: u64, new_data: Vec<u8>) {
        // In a real impl: walk from root to leaf, duplicate each node on the path,
        // and update parent pointers. For the simulation we just update the leaf
        // and recompute its checksum.
        if let Some(node) = self.nodes.iter_mut().find(|n| n.id == leaf_id) {
            if node.refcount > 1 {
                // CoW: clone the node
                let new_id = self.next_id;
                self.next_id += 1;
                let mut new_node = node.clone();
                new_node.id = new_id;
                new_node.refcount = 1;
                new_node.data = new_data.clone();
                new_node.checksum = crate::checksum::crc32(&new_data);
                node.refcount -= 1;
                self.nodes.push(new_node);
                log::trace!("[fs:cow] CoW leaf {} -> {}", leaf_id, new_id);
            } else {
                node.data = new_data.clone();
                node.checksum = crate::checksum::crc32(&new_data);
            }
        }
    }

    pub fn verify(&self) -> bool {
        self.nodes.iter().all(|n| n.checksum == crate::checksum::crc32(&n.data))
    }

    pub fn node_count(&self) -> usize { self.nodes.len() }
}
