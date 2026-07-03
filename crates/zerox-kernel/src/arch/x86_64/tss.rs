//! x86_64 Task State Segment (TSS).
//!
//! In long mode the TSS no longer holds a task's register state. It survives for
//! two jobs the CPU still delegates to it:
//!
//! - **`rsp0`** — the stack pointer loaded when the CPU takes an interrupt or a
//!   syscall that raises privilege from ring 3 to ring 0. Without a valid
//!   `rsp0`, a fault in user mode would try to push onto the user stack.
//! - **The IST (Interrupt Stack Table)** — up to 7 known-good stacks selected by
//!   an IDT entry's `ist` field, so that critical faults (double fault, NMI)
//!   always land on a fresh, valid stack even if the current one is corrupt.
//!
//! The layout below is fixed by the architecture (AMD64 Vol 2 §12.2.5).

/// The x86_64 Task State Segment. `#[repr(C, packed)]` is mandatory — the CPU
/// reads these fields at exact byte offsets.
#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
pub struct TaskStateSegment {
    _reserved0: u32,
    /// Ring 0/1/2 stack pointers (`rsp0` is index 0).
    pub privilege_stacks: [u64; 3],
    _reserved1: u64,
    /// The 7 Interrupt Stack Table pointers (`ist1`..`ist7`).
    pub interrupt_stacks: [u64; 7],
    _reserved2: u64,
    _reserved3: u16,
    /// Offset of the I/O permission bitmap. Set past the TSS to disable it.
    pub iomap_base: u16,
}

impl TaskStateSegment {
    /// A zeroed TSS with the I/O bitmap disabled (base past the end of the TSS).
    pub const fn new() -> Self {
        TaskStateSegment {
            _reserved0: 0,
            privilege_stacks: [0; 3],
            _reserved1: 0,
            interrupt_stacks: [0; 7],
            _reserved2: 0,
            _reserved3: 0,
            iomap_base: core::mem::size_of::<TaskStateSegment>() as u16,
        }
    }

    /// Set the ring-0 stack (`rsp0`) used on privilege escalation.
    pub fn set_kernel_stack(&mut self, rsp0: u64) {
        self.privilege_stacks[0] = rsp0;
    }

    /// Install an IST stack (`index` 0..7 maps to `ist1`..`ist7`).
    pub fn set_ist(&mut self, index: usize, stack_top: u64) {
        self.interrupt_stacks[index] = stack_top;
    }
}

impl Default for TaskStateSegment {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::mem::{align_of, size_of};

    #[test]
    fn tss_is_the_architectural_size() {
        // The 64-bit TSS is exactly 104 bytes.
        assert_eq!(size_of::<TaskStateSegment>(), 104);
    }

    #[test]
    fn iomap_base_defaults_past_the_tss() {
        let tss = TaskStateSegment::new();
        let base = tss.iomap_base;
        assert_eq!(base as usize, size_of::<TaskStateSegment>());
    }

    #[test]
    fn packed_layout_has_no_padding_alignment() {
        // repr(C, packed) → alignment 1, so field offsets are exactly as declared.
        assert_eq!(align_of::<TaskStateSegment>(), 1);
    }

    #[test]
    fn stacks_round_trip() {
        let mut tss = TaskStateSegment::new();
        tss.set_kernel_stack(0x1000);
        tss.set_ist(0, 0x2000);
        // Copy the (packed) arrays out before asserting to avoid taking a
        // reference to an unaligned packed field.
        let priv_stacks = tss.privilege_stacks;
        let irq_stacks = tss.interrupt_stacks;
        assert_eq!(priv_stacks[0], 0x1000);
        assert_eq!(irq_stacks[0], 0x2000);
    }
}
