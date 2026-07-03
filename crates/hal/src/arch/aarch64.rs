//! ARM64 bare-metal HAL implementation.
//!
//! On real aarch64 hardware this module contains:
//! - Generic Timer (CNTVCT_EL0) for high-resolution timing
//! - GIC v3 for interrupt dispatch and IPIs (SGI)
//! - TTBR0/TTBR1 manipulation for page tables, TLBI for invalidation
//! - DAIF manipulation for interrupt enable/disable
//! - WFI for cpu_halt
//! - PSCI for power management (CPU_SUSPEND, SYSTEM_OFF, SYSTEM_RESET)
//! - ARM pointer authentication (PAC) for code pointer signing
//!
//! Currently a placeholder — the host simulation covers all testing paths.

use crate::interrupt::{Handler, Irq, IrqConfig};
use crate::memory::{PageFlags, PhysAddr, VirtAddr};
use crate::power::{RebootReason, SleepState};

pub unsafe fn cpu_halt() {
    unsafe { core::arch::asm!("wfi", options(nostack, preserves_flags)); }
}

pub fn cpu_yield() {
    // On aarch64 bare, yield is a no-op.
}

pub unsafe fn invalidate_tlb() {
    unsafe { core::arch::asm!("tlbi vmalle1", options(nostack, preserves_flags)); }
}

pub unsafe fn enable_interrupt() {
    unsafe { core::arch::asm!("msr daifclr, #0xf", options(nostack, preserves_flags)); }
}

pub unsafe fn disable_interrupt() -> bool {
    let daif: u64;
    unsafe { core::arch::asm!("mrs {}, daif", out(reg) daif, options(nostack, preserves_flags)); }
    unsafe { core::arch::asm!("msr daifset, #0xf", options(nostack, preserves_flags)); }
    (daif & 0x3c0) != 0x3c0
}

pub fn read_cycle_counter() -> u64 {
    let v: u64;
    unsafe { core::arch::asm!("mrs {}, cntvct_el0", out(reg) v, options(nostack, preserves_flags)); }
    v
}

pub fn cpu_id() -> u32 {
    let v: u64;
    unsafe { core::arch::asm!("mrs {}, mpidr_el1", out(reg) v, options(nostack, preserves_flags)); }
    (v & 0xff) as u32
}

pub unsafe fn map_page(_virt: VirtAddr, _phys: PhysAddr, _flags: PageFlags) {
    unimplemented!("aarch64 bare-metal map_page — use 'host' feature for simulation")
}

pub unsafe fn unmap_page(_virt: VirtAddr) {
    unimplemented!("aarch64 bare-metal unmap_page — use 'host' feature for simulation")
}

pub fn allocate_dma(_size: usize, _align: usize) -> Option<crate::memory::DmaRegion> {
    unimplemented!("aarch64 bare-metal allocate_dma — use 'host' feature for simulation")
}

pub unsafe fn flush_cache(addr: VirtAddr, size: usize) {
    // DC CVAU — clean data cache by VA to PoU
    let mut a = addr.0 as usize;
    let end = a + size;
    while a < end {
        unsafe { core::arch::asm!("dc cvau, {}", in(reg) a, options(nostack, preserves_flags)); }
        a += 64; // cache line size
    }
    unsafe { core::arch::asm!("dsb ish", options(nostack, preserves_flags)); }
}

pub unsafe fn invalidate_cache(addr: VirtAddr, size: usize) {
    // DC IVAC — invalidate data cache by VA to PoC
    let mut a = addr.0 as usize;
    let end = a + size;
    while a < end {
        unsafe { core::arch::asm!("dc ivac, {}", in(reg) a, options(nostack, preserves_flags)); }
        a += 64;
    }
    unsafe { core::arch::asm!("dsb ish", options(nostack, preserves_flags)); }
}

pub unsafe fn register_irq(_irq: Irq, _config: IrqConfig, _handler: Handler) {
    unimplemented!("aarch64 bare-metal register_irq — use 'host' feature for simulation")
}

pub unsafe fn unregister_irq(_irq: Irq) {}
pub unsafe fn send_ipi(_cpu: u32, _vector: Irq) {}
pub unsafe fn ack_irq(_irq: Irq) {}
pub unsafe fn eoi(_irq: Irq) {}

pub fn read_time_ns() -> u64 {
    // CNTVCT_EL0 counts ticks; CNTFRQ_EL1 gives ticks per second.
    let count: u64;
    let freq: u64;
    unsafe {
        core::arch::asm!("mrs {}, cntvct_el0", out(reg) count, options(nostack, preserves_flags));
        core::arch::asm!("mrs {}, cntfrq_el0", out(reg) freq, options(nostack, preserves_flags));
    }
    if freq == 0 { return count; }
    count.saturating_mul(1_000_000_000) / freq
}

pub unsafe fn set_deadline_ns(_ns: u64) {}
pub unsafe fn cancel_deadline() {}
pub unsafe fn set_periodic(_hz: u32) {}

pub unsafe fn sleep(_state: SleepState) {
    cpu_halt();
}

pub unsafe fn hibernate() {
    unimplemented!("hibernate on aarch64 bare metal")
}

pub fn shutdown() -> ! {
    // PSCI SYSTEM_OFF — HVC call
    unsafe {
        core::arch::asm!(
            "hvc #0",
            in("x0") 0x84000008u64, // PSCI SYSTEM_OFF
            options(nostack, preserves_flags)
        );
    }
    loop { unsafe { core::arch::asm!("wfi"); } }
}

pub fn reboot(_reason: RebootReason) -> ! {
    unsafe {
        core::arch::asm!(
            "hvc #0",
            in("x0") 0x84000009u64, // PSCI SYSTEM_RESET
            options(nostack, preserves_flags)
        );
    }
    loop { unsafe { core::arch::asm!("wfi"); } }
}

pub fn set_cpu_frequency(_cpu: u32, _freq_khz: u32) {}
pub fn get_cpu_frequency(_cpu: u32) -> u32 { 2_000_000 }
