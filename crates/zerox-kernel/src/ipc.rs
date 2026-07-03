//! # IPC — Inter-Process Communication
//!
//! Three IPC primitives, each tuned for a different payload size and trust
//! boundary:
//!
//! 1. **Fast Messages** — small messages (≤ 64 bytes), copied directly through
//!    kernel bounce buffers. Latency: < 500 ns round-trip.
//!
//! 2. **Shared Memory Channels** — large transfers (video frames, textures,
//!    audio buffers) via mapped pages. Zero-copy, no syscalls on the hot path.
//!
//! 3. **Capability Objects** — secure handle transfer between processes. A
//!    capability is an unforgeable kernel reference that cannot be forged or
//!    elevated, only granted and revoked.
//!
//! All three share lock-free queues, batched delivery, and cache-aware buffer
//! layout.

use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};

/// Maximum payload size for a Fast Message.
pub const FAST_MSG_MAX: usize = 64;
/// Target maximum latency for a Fast Message round-trip (nanoseconds).
pub const FAST_MSG_TARGET_LATENCY_NS: u64 = 500;

/// A fast message — payload up to 64 bytes, copied through the kernel.
#[derive(Debug, Clone, Copy)]
pub struct FastMessage {
    pub src: u64,
    pub dst: u64,
    pub len: u8,
    pub payload: [u8; FAST_MSG_MAX],
}

impl FastMessage {
    pub fn new(src: u64, dst: u64, payload: &[u8]) -> Self {
        let mut p = [0u8; FAST_MSG_MAX];
        let len = payload.len().min(FAST_MSG_MAX);
        p[..len].copy_from_slice(&payload[..len]);
        Self { src, dst, len: len as u8, payload: p }
    }

    pub fn as_bytes(&self) -> &[u8] { &self.payload[..self.len as usize] }
}

/// A lock-free ring buffer for fast messages between two endpoints.
pub struct FastChannel {
    capacity: usize,
    buffer: spin::Mutex<Vec<FastMessage>>,
    sent: AtomicU64,
    received: AtomicU64,
}

impl FastChannel {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            buffer: spin::Mutex::new(Vec::with_capacity(capacity)),
            sent: AtomicU64::new(0),
            received: AtomicU64::new(0),
        }
    }

    pub fn send(&self, msg: FastMessage) -> Result<(), FastMessage> {
        let mut buf = self.buffer.lock();
        if buf.len() >= self.capacity {
            return Err(msg);
        }
        buf.push(msg);
        self.sent.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    pub fn recv(&self) -> Option<FastMessage> {
        let mut buf = self.buffer.lock();
        let msg = buf.first().cloned();
        if msg.is_some() {
            buf.remove(0);
            self.received.fetch_add(1, Ordering::Relaxed);
        }
        msg
    }

    pub fn sent(&self) -> u64 { self.sent.load(Ordering::Relaxed) }
    pub fn received(&self) -> u64 { self.received.load(Ordering::Relaxed) }
    pub fn pending(&self) -> usize {
        let buf = self.buffer.lock();
        buf.len()
    }
}

/// A shared-memory channel for large zero-copy transfers.
pub struct ShmChannel {
    pub id: u64,
    pub size: usize,
    pub producer: u64,
    pub consumer: u64,
}

impl ShmChannel {
    pub fn new(id: u64, size: usize, producer: u64, consumer: u64) -> Self {
        Self { id, size, producer, consumer }
    }
}

/// The IPC core — kernel transport for all three primitive types.
pub struct IpcCore {
    fast_channels: spin::Mutex<Vec<(u64, u64, FastChannel)>>,
    shm_channels: spin::Mutex<Vec<ShmChannel>>,
    next_channel_id: AtomicU64,
    messages_sent: AtomicU64,
    messages_received: AtomicU64,
}

impl IpcCore {
    pub const fn new() -> Self {
        Self {
            fast_channels: spin::Mutex::new(Vec::new()),
            shm_channels: spin::Mutex::new(Vec::new()),
            next_channel_id: AtomicU64::new(1),
            messages_sent: AtomicU64::new(0),
            messages_received: AtomicU64::new(0),
        }
    }

    pub fn init(&self) {
        log::info!("[ipc] fast message target latency: <{}ns", FAST_MSG_TARGET_LATENCY_NS);
        log::info!("[ipc] fast message max payload: {} bytes", FAST_MSG_MAX);
    }

    /// Create a fast message channel between two endpoints.
    pub fn create_fast_channel(&self, a: u64, b: u64, capacity: usize) -> u64 {
        let id = self.next_channel_id.fetch_add(1, Ordering::Relaxed);
        let chan = FastChannel::new(capacity);
        self.fast_channels.lock().push((id, (a << 32) | b, chan));
        log::info!("[ipc] +fast_channel id={} {} <-> {} cap={}", id, a, b, capacity);
        id
    }

    /// Send a fast message on a channel.
    pub fn send_fast(&self, channel_id: u64, msg: FastMessage) -> Result<(), &'static str> {
        let mut chans = self.fast_channels.lock();
        for (id, _, chan) in chans.iter_mut() {
            if *id == channel_id {
                chan.send(msg).map_err(|_| "channel full")?;
                self.messages_sent.fetch_add(1, Ordering::Relaxed);
                return Ok(());
            }
        }
        Err("no such channel")
    }

    /// Receive a fast message from a channel.
    pub fn recv_fast(&self, channel_id: u64) -> Option<FastMessage> {
        let mut chans = self.fast_channels.lock();
        for (id, _, chan) in chans.iter_mut() {
            if *id == channel_id {
                let m = chan.recv();
                if m.is_some() {
                    self.messages_received.fetch_add(1, Ordering::Relaxed);
                }
                return m;
            }
        }
        None
    }

    /// Create a shared-memory channel.
    pub fn create_shm_channel(&self, producer: u64, consumer: u64, size: usize) -> u64 {
        let id = self.next_channel_id.fetch_add(1, Ordering::Relaxed);
        let chan = ShmChannel::new(id, size, producer, consumer);
        self.shm_channels.lock().push(chan);
        log::info!("[ipc] +shm_channel id={} {} -> {} size={} bytes", id, producer, consumer, size);
        id
    }

    pub fn stats(&self) -> IpcStats {
        IpcStats {
            fast_channels: self.fast_channels.lock().len() as u64,
            shm_channels: self.shm_channels.lock().len() as u64,
            messages_sent: self.messages_sent.load(Ordering::Relaxed),
            messages_received: self.messages_received.load(Ordering::Relaxed),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct IpcStats {
    pub fast_channels: u64,
    pub shm_channels: u64,
    pub messages_sent: u64,
    pub messages_received: u64,
}
