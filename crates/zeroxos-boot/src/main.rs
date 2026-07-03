//! zeroxos bare-metal entry point (x86_64).
//!
//! This is the freestanding binary that links the `#![no_std]` kernel into a
//! bootable image. It is compiled only for `targets/x86_64-unknown-zeroxos.json`.
//!
//! ## Boot flow (current — milestone M2)
//! 1. `_start` is the ELF entry point (see `linker/x86_64.ld` `ENTRY(_start)`).
//! 2. Initialize the early kernel heap over a static `.bss` arena so that
//!    `alloc`-backed kernel structures work.
//! 3. Initialize the HAL, then boot the kernel.
//! 4. Halt.
//!
//! ## Not yet done (later milestones)
//! - **M7**: a proper multiboot2 header + assembly stub that sets up a real
//!   stack before jumping here, and wiring a serial-port logger so the kernel's
//!   boot log appears on the QEMU console. Until then this produces a valid,
//!   linkable ELF but is not expected to print anything.
//! - The early heap is a fixed static arena; once the bootloader memory map is
//!   parsed (M7), the heap is grown from buddy-owned physical frames.

#![no_std]
#![no_main]

use core::arch::asm;

/// Size of the early boot heap carved out of `.bss` (1 MiB). Enough for the
/// kernel's boot-time allocations before a real memory map exists.
const EARLY_HEAP_SIZE: usize = 1024 * 1024;

/// The early boot heap arena. Lives in `.bss`; the boot loader zeroes it.
static mut EARLY_HEAP: [u8; EARLY_HEAP_SIZE] = [0; EARLY_HEAP_SIZE];

/// Kernel entry point. The bootloader jumps here (M7 adds the multiboot2 header
/// and stack setup that precede this in a real boot).
///
/// # Safety
/// Called exactly once, by the bootloader, with the machine in the state the
/// multiboot2 spec guarantees.
#[no_mangle]
pub extern "C" fn _start() -> ! {
    // SAFETY: single-threaded, pre-scheduler context; `EARLY_HEAP` is a unique
    // static we hand exclusively to the allocator, once.
    unsafe {
        let base = core::ptr::addr_of_mut!(EARLY_HEAP) as usize;
        zerox_kernel::heap::init_kernel_heap(base, EARLY_HEAP_SIZE);
        // Install the GDT (+TSS) and IDT so the CPU has valid segments and
        // exception handlers before anything can fault.
        zerox_kernel::arch::x86_64::init();
        hal::init();
    }

    // Boot the kernel. On bare metal there is no logger installed yet (M7), so
    // this runs silently; a boot failure is surfaced by halting below.
    let mut kernel = zerox_kernel::Kernel::new();
    let _boot_result = kernel.boot();

    // Nothing left to schedule yet — halt the CPU forever.
    loop {
        // SAFETY: `hlt` with interrupts is a safe idle; no memory touched.
        unsafe { asm!("hlt", options(nomem, nostack, preserves_flags)) };
    }
}
