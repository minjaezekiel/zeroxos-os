//! Architecture-specific kernel code.
//!
//! Only the HAL is meant to be arch-aware in general, but a few pieces are
//! irreducibly CPU-specific and live *in the kernel* because they manipulate
//! kernel data structures directly: the descriptor tables (GDT/IDT/TSS), the
//! context switch, and the syscall entry trampoline. Each is isolated here
//! behind `#[cfg(target_arch = ...)]` so the generic kernel stays portable.

#[cfg(target_arch = "x86_64")]
pub mod x86_64;
