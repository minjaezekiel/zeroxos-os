//! # hal — Hardware Abstraction Layer
//!
//! The HAL is the only place that knows whether you're running on ARM or x86.
//! Adding a new architecture (e.g. RISC-V) requires only a new HAL implementation
//! — the kernel remains unchanged.
//!
//! ## Modules
//! - [`cpu`] — CPU control surface (halt, yield, TLB invalidation, IRQ enable/disable, cycle counter)
//! - [`memory`] — page mapping, DMA, cache maintenance
//! - [`interrupt`] — IRQ registration and IPI dispatch
//! - [`timer`] — high-resolution timer (microsecond precision)
//! - [`power`] — sleep, hibernate, shutdown, reboot, CPU frequency
//! - [`arch`] — architecture-specific implementations (x86_64, aarch64)

// `no_std` on bare metal; the `host` backend (arch/host.rs) uses std, so keep
// std available when the `host` feature is on.
#![cfg_attr(not(feature = "host"), no_std)]
#![allow(dead_code)]

pub mod cpu;
pub mod memory;
pub mod interrupt;
pub mod timer;
pub mod power;
pub mod arch;

/// The architecture the HAL is currently targeting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arch {
    X86_64,
    Aarch64,
    /// Host simulation — used by `zerox-sim` to run the kernel as a userspace process.
    Host,
}

/// Detected architecture at compile time.
pub fn current_arch() -> Arch {
    #[cfg(all(target_arch = "x86_64", not(feature = "host")))]
    { Arch::X86_64 }
    #[cfg(all(target_arch = "aarch64", not(feature = "host")))]
    { Arch::Aarch64 }
    #[cfg(feature = "host")]
    { Arch::Host }
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64", feature = "host")))]
    { compile_error!("unsupported architecture"); }
}

/// HAL initialization. Must be called once at boot before any other HAL call.
///
/// # Safety
/// Must be called exactly once, before any other HAL function.
pub unsafe fn init() {
    arch::init();
}
