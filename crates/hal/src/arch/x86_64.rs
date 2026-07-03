//! x86_64 bare-metal HAL implementation.
//!
//! On real x86_64 hardware this module contains:
//! - TSC + RDTSC for the cycle counter and high-resolution timer
//! - HPET for wall-clock and periodic scheduling ticks
//! - Local APIC + IO-APIC for interrupt dispatch and IPIs
//! - CR3 manipulation for page tables, INVLPG for TLB flushes
//! - STI/CLI for interrupt enable/disable
//! - HLT for cpu_halt
//! - ACPI for power management (S-states, frequency scaling)
//!
//! This is the file you would link into a `target_os = "none"` build.
//! Currently a placeholder that panics — the host simulation covers all
//! testing and demonstration paths.

use crate::interrupt::{Handler, Irq, IrqConfig};
use crate::memory::{PageFlags, PhysAddr, VirtAddr};
use crate::power::{RebootReason, SleepState};

pub unsafe fn cpu_halt() {
    unsafe { core::arch::asm!("hlt", options(nostack, preserves_flags)); }
}

pub fn cpu_yield() {
    // On x86_64 bare, yield is a no-op (the scheduler handles context switches).
}

pub unsafe fn invalidate_tlb() {
    unsafe { core::arch::asm!("invlpg [{}]", in(reg) 0usize, options(nostack, preserves_flags)); }
}

pub unsafe fn enable_interrupt() {
    unsafe { core::arch::asm!("sti", options(nostack, preserves_flags)); }
}

pub unsafe fn disable_interrupt() -> bool {
    let flags: u64;
    unsafe { core::arch::asm!("pushfq", "pop {}", out(reg) flags, options(nostack, preserves_flags)); }
    unsafe { core::arch::asm!("cli", options(nostack, preserves_flags)); }
    (flags & 0x200) != 0
}

pub fn read_cycle_counter() -> u64 {
    let lo: u32;
    let hi: u32;
    unsafe { core::arch::asm!("rdtsc", out("eax") lo, out("edx") hi, options(nostack, preserves_flags)); }
    ((hi as u64) << 32) | (lo as u64)
}

pub fn cpu_id() -> u32 {
    // Read from local APIC ID register — placeholder returns 0.
    0
}

pub unsafe fn map_page(_virt: VirtAddr, _phys: PhysAddr, _flags: PageFlags) {
    // Would manipulate CR3 page tables here.
    unimplemented!("x86_64 bare-metal map_page — use 'host' feature for simulation")
}

pub unsafe fn unmap_page(_virt: VirtAddr) {
    unimplemented!("x86_64 bare-metal unmap_page — use 'host' feature for simulation")
}

pub fn allocate_dma(_size: usize, _align: usize) -> Option<crate::memory::DmaRegion> {
    unimplemented!("x86_64 bare-metal allocate_dma — use 'host' feature for simulation")
}

pub unsafe fn flush_cache(_addr: VirtAddr, _size: usize) {
    // x86_64 is cache-coherent for DMA — typically a no-op.
}

pub unsafe fn invalidate_cache(_addr: VirtAddr, _size: usize) {
    // x86_64 is cache-coherent for DMA — typically a no-op.
}

pub unsafe fn register_irq(_irq: Irq, _config: IrqConfig, _handler: Handler) {
    unimplemented!("x86_64 bare-metal register_irq — use 'host' feature for simulation")
}

pub unsafe fn unregister_irq(_irq: Irq) {}
pub unsafe fn send_ipi(_cpu: u32, _vector: Irq) {}
pub unsafe fn ack_irq(_irq: Irq) {}
pub unsafe fn eoi(_irq: Irq) {}

pub fn read_time_ns() -> u64 {
    // TSC-based — in real impl, would calibrate against HPET.
    read_cycle_counter()
}

pub unsafe fn set_deadline_ns(_ns: u64) {}
pub unsafe fn cancel_deadline() {}
pub unsafe fn set_periodic(_hz: u32) {}

pub unsafe fn sleep(_state: SleepState) {
    cpu_halt();
}

pub unsafe fn hibernate() {
    unimplemented!("hibernate on x86_64 bare metal")
}

pub fn shutdown() -> ! {
    // ACPI shutdown — write 0x2000 to 0x604 (QEMU) or 0xB004 (older ACPI)
    unsafe { core::arch::asm!("outw", in("dx") 0x604u16, in("ax") 0x2000u16); }
    loop { unsafe { core::arch::asm!("hlt"); } }
}

pub fn reboot(_reason: RebootReason) -> ! {
    // Keyboard controller reset
    unsafe { core::arch::asm!("outb", in("dx") 0x64u16, in("al") 0xFEu8); }
    loop { unsafe { core::arch::asm!("hlt"); } }
}

pub fn set_cpu_frequency(_cpu: u32, _freq_khz: u32) {
    // Would write to MSR or ACPI _PSS package.
}

pub fn get_cpu_frequency(_cpu: u32) -> u32 {
    3_000_000
}
