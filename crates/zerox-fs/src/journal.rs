//! zeroxfs journal — write-ahead log for metadata consistency.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JournalOp {
    Begin,
    Commit,
    CreateInode,
    UnlinkInode,
    WriteBlock,
    FreeBlock,
    XattrSet,
    SnapshotCreate,
}

#[derive(Debug, Clone)]
pub struct JournalEntry {
    pub seq: u64,
    pub op: JournalOp,
    pub target: u64,
    pub data: Vec<u8>,
}

/// The journal — a circular buffer of entries on disk.
pub struct Journal {
    pub entries: Vec<JournalEntry>,
    pub next_seq: u64,
    pub committed_seq: u64,
}

impl Journal {
    pub fn new() -> Self {
        Self { entries: Vec::new(), next_seq: 1, committed_seq: 0 }
    }

    /// Begin a transaction. Returns the transaction's sequence number.
    pub fn begin(&mut self) -> u64 {
        let seq = self.next_seq;
        self.next_seq += 1;
        self.entries.push(JournalEntry { seq, op: JournalOp::Begin, target: 0, data: Vec::new() });
        seq
    }

    /// Append an operation to the current transaction.
    pub fn append(&mut self, txn: u64, op: JournalOp, target: u64, data: Vec<u8>) {
        self.entries.push(JournalEntry { seq: txn, op, target, data });
    }

    /// Commit a transaction. Until commit, the operations are not visible.
    pub fn commit(&mut self, txn: u64) {
        self.entries.push(JournalEntry { seq: txn, op: JournalOp::Commit, target: 0, data: Vec::new() });
        self.committed_seq = txn;
        log::trace!("[fs:journal] committed txn {}", txn);
    }

    /// Replay the journal after a crash. Returns the number of txns replayed.
    pub fn replay(&mut self) -> usize {
        let mut count = 0;
        let mut current_txn = None;
        let mut txn_entries: Vec<usize> = Vec::new();
        for (i, e) in self.entries.iter().enumerate() {
            match e.op {
                JournalOp::Begin => {
                    current_txn = Some(e.seq);
                    txn_entries.clear();
                    txn_entries.push(i);
                }
                JournalOp::Commit => {
                    if current_txn == Some(e.seq) {
                        // Replay this transaction's entries.
                        for &idx in &txn_entries {
                            let _entry = &self.entries[idx];
                            // Apply entry to disk structures...
                        }
                        count += 1;
                        current_txn = None;
                    }
                }
                _ => {
                    if current_txn.is_some() { txn_entries.push(i); }
                }
            }
        }
        log::info!("[fs:journal] replayed {} transactions", count);
        count
    }

    pub fn entry_count(&self) -> usize { self.entries.len() }
}
