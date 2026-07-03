//! Architecture-specific implementations.
//!
//! The `host` feature makes the HAL run as a userspace process — this is how
//! `zerox-sim` runs the kernel on a development machine without a hypervisor
//! or actual hardware.

#[cfg(feature = "host")]
mod host;
#[cfg(feature = "host")]
pub use host::*;

#[cfg(all(not(feature = "host"), target_arch = "x86_64"))]
mod x86_64;
#[cfg(all(not(feature = "host"), target_arch = "x86_64"))]
pub use x86_64::*;

#[cfg(all(not(feature = "host"), target_arch = "aarch64"))]
mod aarch64;
#[cfg(all(not(feature = "host"), target_arch = "aarch64"))]
pub use aarch64::*;

// Fallback when neither feature nor target arch matches — emit stubs so docs build.
#[cfg(not(any(feature = "host", all(target_arch = "x86_64"), all(target_arch = "aarch64"))))]
mod stub;
#[cfg(not(any(feature = "host", all(target_arch = "x86_64"), all(target_arch = "aarch64"))))]
pub use stub::*;

pub fn init() {
    // Architecture-specific init (e.g. configure APIC, GIC, timer calibration).
    // On host, this is a no-op.
}
