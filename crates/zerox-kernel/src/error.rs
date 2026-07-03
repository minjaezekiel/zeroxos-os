//! Kernel error types.
//!
//! The kernel is `#![no_std]`, so we cannot use `std::error::Error` or
//! `thiserror` (v1 requires std). Errors are a plain `core`-only enum with a
//! hand-written [`core::fmt::Display`]. Fallible kernel operations return
//! [`KernelResult`]; `panic!` is reserved for genuinely unrecoverable states
//! (and always carries a message).

use core::fmt;

/// The result type used across fallible kernel operations.
pub type KernelResult<T> = Result<T, KernelError>;

/// Errors that can occur during kernel operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum KernelError {
    /// The memory manager failed to initialize (no usable physical memory
    /// was registered with the buddy allocator).
    MemoryInit,
    /// A physical page/frame allocation failed (out of memory).
    OutOfMemory,
    /// The scheduler failed to initialize.
    SchedulerInit,
    /// A subsystem was used before the kernel finished booting.
    NotBooted,
    /// A capability check denied the operation.
    PermissionDenied,
    /// A requested feature is not yet implemented on this build/arch.
    Unsupported,
}

impl fmt::Display for KernelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let msg = match self {
            KernelError::MemoryInit => "memory manager failed to initialize (no usable RAM)",
            KernelError::OutOfMemory => "out of memory",
            KernelError::SchedulerInit => "scheduler failed to initialize",
            KernelError::NotBooted => "kernel subsystem used before boot completed",
            KernelError::PermissionDenied => "capability check denied the operation",
            KernelError::Unsupported => "operation not supported on this build/architecture",
        };
        f.write_str(msg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_is_human_readable() {
        assert_eq!(
            alloc::format!("{}", KernelError::OutOfMemory),
            "out of memory"
        );
    }

    #[test]
    fn errors_are_comparable() {
        assert_eq!(KernelError::MemoryInit, KernelError::MemoryInit);
        assert_ne!(KernelError::MemoryInit, KernelError::OutOfMemory);
    }
}
