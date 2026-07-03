//! x86_64 Global Descriptor Table (GDT).
//!
//! In long mode the GDT no longer does real segmentation, but it is still
//! required: the CPU needs valid code/data segment descriptors, and — crucially
//! — a Task State Segment (TSS) descriptor so that interrupts and syscalls can
//! switch to a known-good kernel stack (`rsp0` / the IST entries).
//!
//! We install a minimal, conventional layout:
//!
//! | Selector | Entry            | Purpose                          |
//! |----------|------------------|----------------------------------|
//! | `0x00`   | null             | required                         |
//! | `0x08`   | kernel code      | CS in ring 0                     |
//! | `0x10`   | kernel data      | SS/DS in ring 0                  |
//! | `0x18`   | user data        | SS/DS in ring 3                  |
//! | `0x20`   | user code        | CS in ring 3                     |
//! | `0x28`   | TSS (low)        | 16-byte system descriptor        |
//! | `0x30`   | TSS (high)       | (upper half of the TSS desc)     |
//!
//! **User-data precedes user-code deliberately**: `sysret` derives the user
//! `SS` from `STAR[63:48] + 8` and `CS` from `STAR[63:48] + 16`, so with the
//! base pointing at kernel-data (`0x10`) the layout yields `SS = 0x18` (user
//! data) and `CS = 0x20` (user code). See `syscall::init_syscalls`.
//!
//! The *encoding* of every descriptor is pure arithmetic and is unit-tested on
//! the host; only [`Gdt::load`] executes privileged instructions and so is
//! compiled for the bare-metal build.

use super::tss::TaskStateSegment;

/// Segment selectors (offsets into the GDT). The low 2 bits are the RPL.
pub const KERNEL_CODE: u16 = 0x08;
pub const KERNEL_DATA: u16 = 0x10;
pub const USER_DATA: u16 = 0x18 | 3; // RPL = 3
pub const USER_CODE: u16 = 0x20 | 3; // RPL = 3
pub const TSS_SELECTOR: u16 = 0x28;
/// Base selector `sysret` uses (`SS = base+8` = user data, `CS = base+16` = user code).
pub const STAR_SYSRET_BASE: u16 = KERNEL_DATA;
/// Base selector `syscall` uses (`CS = base` = kernel code, `SS = base+8` = kernel data).
pub const STAR_SYSCALL_BASE: u16 = KERNEL_CODE;

/// A single 8-byte GDT descriptor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
pub struct SegmentDescriptor(pub u64);

// Access-byte bits.
const ACCESS_PRESENT: u64 = 1 << 7;
const ACCESS_DPL3: u64 = 3 << 5;
const ACCESS_TYPE: u64 = 1 << 4; // 1 = code/data (not a system segment)
const ACCESS_EXEC: u64 = 1 << 3; // 1 = code
const ACCESS_RW: u64 = 1 << 1; // readable code / writable data
// Flags nibble (high 4 bits of byte 6).
const FLAG_GRANULARITY: u64 = 1 << 7; // limit in 4 KiB units
const FLAG_DB: u64 = 1 << 6; // 32-bit segment (data / 32-bit code)
const FLAG_LONG: u64 = 1 << 5; // 64-bit code segment

impl SegmentDescriptor {
    pub const NULL: SegmentDescriptor = SegmentDescriptor(0);

    /// Build a descriptor from an access byte and a flags nibble, with the
    /// conventional flat base=0 / limit=0xFFFFF used in long mode.
    const fn flat(access: u64, flags: u64) -> Self {
        let limit_low = 0xFFFF;
        let limit_high = 0xF; // high nibble of the 20-bit limit
        SegmentDescriptor(
            limit_low
                | (access << 40)
                | (limit_high << 48)
                | (flags << 48)
                // base = 0, so base_low/mid/high contribute nothing
        )
    }

    pub const fn kernel_code() -> Self {
        Self::flat(
            ACCESS_PRESENT | ACCESS_TYPE | ACCESS_EXEC | ACCESS_RW,
            FLAG_GRANULARITY | FLAG_LONG,
        )
    }
    pub const fn kernel_data() -> Self {
        Self::flat(
            ACCESS_PRESENT | ACCESS_TYPE | ACCESS_RW,
            FLAG_GRANULARITY | FLAG_DB,
        )
    }
    pub const fn user_code() -> Self {
        Self::flat(
            ACCESS_PRESENT | ACCESS_DPL3 | ACCESS_TYPE | ACCESS_EXEC | ACCESS_RW,
            FLAG_GRANULARITY | FLAG_LONG,
        )
    }
    pub const fn user_data() -> Self {
        Self::flat(
            ACCESS_PRESENT | ACCESS_DPL3 | ACCESS_TYPE | ACCESS_RW,
            FLAG_GRANULARITY | FLAG_DB,
        )
    }
}

/// A 16-byte TSS system descriptor, split into its two 8-byte halves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TssDescriptor {
    pub low: u64,
    pub high: u64,
}

impl TssDescriptor {
    /// Type nibble `0b1001` = available 64-bit TSS.
    const TYPE_TSS_AVAIL: u64 = 0x9;

    /// Encode a TSS descriptor for a TSS at `base` with byte length `limit`.
    pub const fn new(base: u64, limit: u32) -> Self {
        let limit = limit as u64;
        let low = (limit & 0xFFFF)
            | ((base & 0xFF_FFFF) << 16)
            | (Self::TYPE_TSS_AVAIL << 40)
            | (ACCESS_PRESENT << 40)
            | (((limit >> 16) & 0xF) << 48)
            | (((base >> 24) & 0xFF) << 56);
        let high = (base >> 32) & 0xFFFF_FFFF;
        TssDescriptor { low, high }
    }
}

/// The kernel GDT: 5 segment descriptors plus a 16-byte TSS descriptor.
#[repr(C, align(16))]
pub struct Gdt {
    entries: [u64; 7],
    len: usize,
}

/// The pointer loaded by `lgdt`.
#[repr(C, packed)]
pub struct GdtPointer {
    pub limit: u16,
    pub base: u64,
}

impl Gdt {
    /// Construct the standard kernel GDT around the given TSS.
    pub fn new(tss: &'static TaskStateSegment) -> Self {
        let tss_desc = TssDescriptor::new(
            tss as *const _ as u64,
            (core::mem::size_of::<TaskStateSegment>() - 1) as u32,
        );
        Gdt {
            entries: [
                SegmentDescriptor::NULL.0,
                SegmentDescriptor::kernel_code().0,
                SegmentDescriptor::kernel_data().0,
                // User data precedes user code so `sysret` derives the correct
                // selectors (see module docs / syscall::init_syscalls).
                SegmentDescriptor::user_data().0,
                SegmentDescriptor::user_code().0,
                tss_desc.low,
                tss_desc.high,
            ],
            len: 7,
        }
    }

    fn pointer(&self) -> GdtPointer {
        GdtPointer {
            limit: (self.len * core::mem::size_of::<u64>() - 1) as u16,
            base: self.entries.as_ptr() as u64,
        }
    }

    /// Load this GDT (`lgdt`), reload the segment registers, and load the TSS
    /// (`ltr`).
    ///
    /// # Safety
    /// The GDT must outlive its use (store it in a `static`; the CPU keeps only
    /// the base pointer). Must run in ring 0.
    #[cfg(feature = "bare")]
    pub unsafe fn load(&self) {
        let ptr = self.pointer();
        unsafe {
            core::arch::asm!("lgdt [{}]", in(reg) &ptr, options(readonly, nostack, preserves_flags));
            super::reload_segments(KERNEL_CODE, KERNEL_DATA);
            core::arch::asm!("ltr {0:x}", in(reg) TSS_SELECTOR, options(nostack, preserves_flags));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The canonical long-mode flat descriptor values (see AMD64 Vol 2 §4.8).
    #[test]
    fn kernel_code_descriptor_is_canonical() {
        assert_eq!(SegmentDescriptor::kernel_code().0, 0x00AF_9A00_0000_FFFF);
    }
    #[test]
    fn kernel_data_descriptor_is_canonical() {
        assert_eq!(SegmentDescriptor::kernel_data().0, 0x00CF_9200_0000_FFFF);
    }
    #[test]
    fn user_code_descriptor_is_canonical() {
        assert_eq!(SegmentDescriptor::user_code().0, 0x00AF_FA00_0000_FFFF);
    }
    #[test]
    fn user_data_descriptor_is_canonical() {
        assert_eq!(SegmentDescriptor::user_data().0, 0x00CF_F200_0000_FFFF);
    }

    #[test]
    fn tss_descriptor_encodes_base_and_limit() {
        // Base 0xDEAD_BEEF_1234, limit 0x67 (104-byte TSS - 1).
        let d = TssDescriptor::new(0xDEAD_BEEF_1234, 0x67);
        // limit low
        assert_eq!(d.low & 0xFFFF, 0x67);
        // base[23:0] in bits [16..40]
        assert_eq!((d.low >> 16) & 0xFF_FFFF, 0xEF_1234);
        // present bit + type 0x9 in the access byte (bits [40..48])
        assert_eq!((d.low >> 40) & 0xFF, 0x89);
        // base[31:24] (byte 3 of 0x..BE_EF_1234) in bits [56..64]
        assert_eq!((d.low >> 56) & 0xFF, 0xBE);
        // base[63:32] in the high half
        assert_eq!(d.high, 0xDEAD);
    }

    #[test]
    fn user_selectors_carry_rpl3() {
        assert_eq!(USER_CODE & 3, 3);
        assert_eq!(USER_DATA & 3, 3);
    }
}
