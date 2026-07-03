//! # Security manager — capabilities, not root
//!
//! Every application receives explicit, revocable permissions. There is no
//! superuser. The kernel enforces capability checks at every system call
//! entry point.
//!
//! Capabilities are unforgeable kernel handles — they cannot be elevated or
//! stolen, only granted by the user or inherited from a parent process.

use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};

/// The set of capabilities an application can hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Capability {
    Camera,
    Filesystem { path_hash: u64, access: FileAccess },
    Network { bind_allowed: bool },
    Bluetooth,
    Gpu,
    Audio,
    InputDevices,
    SystemPower,
    Driver { name_hash: u64 },
    /// Raw kernel access (only granted to trusted system processes)
    KernelAdmin,
}

/// File access mode for the Filesystem capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FileAccess {
    Read,
    Write,
    ReadWrite,
    Execute,
}

/// A capability token — unforgeable handle backed by the kernel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CapabilityToken(pub u64);

/// A process's capability set.
#[derive(Debug, Clone, Default)]
pub struct CapabilitySet {
    pub capabilities: Vec<Capability>,
}

impl CapabilitySet {
    pub fn new() -> Self { Self { capabilities: Vec::new() } }

    pub fn grant(&mut self, cap: Capability) {
        if !self.capabilities.contains(&cap) {
            self.capabilities.push(cap);
        }
    }

    pub fn revoke(&mut self, cap: Capability) {
        self.capabilities.retain(|c| *c != cap);
    }

    pub fn has(&self, cap: Capability) -> bool {
        self.capabilities.contains(&cap)
    }

    pub fn check(&self, cap: Capability) -> Result<(), SecurityError> {
        if self.has(cap) { Ok(()) } else { Err(SecurityError::MissingCapability) }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecurityError {
    MissingCapability,
    InvalidToken,
    TokenRevoked,
    PermissionDenied,
}

/// The security manager — issues, tracks, and revokes capabilities.
pub struct SecurityManager {
    next_token: AtomicU64,
    issued: spin::Mutex<Vec<(CapabilityToken, u64, Capability)>>, // (token, pid, cap)
}

impl SecurityManager {
    pub const fn new() -> Self {
        Self {
            next_token: AtomicU64::new(1),
            issued: spin::Mutex::new(Vec::new()),
        }
    }

    pub fn init(&self) {
        log::info!("[sec] capability-based security initialized (no root)");
    }

    /// Issue a capability token to a process.
    pub fn grant(&self, pid: u64, cap: Capability) -> CapabilityToken {
        let token = CapabilityToken(self.next_token.fetch_add(1, Ordering::Relaxed));
        self.issued.lock().push((token, pid, cap));
        log::info!("[sec] +cap {:?} -> pid={}", cap, pid);
        token
    }

    /// Revoke a capability token.
    pub fn revoke(&self, token: CapabilityToken) {
        let mut issued = self.issued.lock();
        if let Some(idx) = issued.iter().position(|(t, _, _)| *t == token) {
            let (_, pid, cap) = issued.remove(idx);
            log::info!("[sec] -cap {:?} from pid={}", cap, pid);
        }
    }

    /// Check if a process holds a given capability.
    pub fn check(&self, pid: u64, cap: Capability) -> Result<(), SecurityError> {
        let issued = self.issued.lock();
        for (_, p, c) in issued.iter() {
            if *p == pid && *c == cap {
                return Ok(());
            }
        }
        Err(SecurityError::MissingCapability)
    }

    /// Verify that a token is valid for the given process.
    pub fn verify_token(&self, token: CapabilityToken, pid: u64) -> Result<Capability, SecurityError> {
        let issued = self.issued.lock();
        for (t, p, c) in issued.iter() {
            if *t == token && *p == pid {
                return Ok(*c);
            }
        }
        Err(SecurityError::InvalidToken)
    }

    pub fn capability_count(&self) -> usize {
        self.issued.lock().len()
    }
}
