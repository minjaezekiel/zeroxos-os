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
use core::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use super::paging;

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

// --- Page-table runtime glue -----------------------------------------------
//
// The reusable walking logic lives in `paging`; here we bind it to the running
// CPU: the active PML4 comes from CR3, physical frames are reached through a
// direct map, and intermediate tables are allocated from a frame source the
// kernel registers at boot. The direct-map base and the frame-allocator hooks
// are wired up by the boot code (M7) once the memory map is known.

/// Base added to a physical address to reach it virtually (physical direct map).
/// Defaults to 0 (identity map), valid for the early boot low-memory identity
/// mapping the loader/boot stub establishes.
static DIRECT_MAP_BASE: AtomicU64 = AtomicU64::new(0);
/// `fn() -> u64` returning a fresh physical frame (0 = allocation failed).
static FRAME_ALLOC_FN: AtomicUsize = AtomicUsize::new(0);
/// `fn(u64)` returning a frame to the allocator.
static FRAME_FREE_FN: AtomicUsize = AtomicUsize::new(0);

/// Set the physical direct-map base used to reach page tables (call at boot).
pub fn set_direct_map_base(base: u64) {
    DIRECT_MAP_BASE.store(base, Ordering::Relaxed);
}

/// Register the physical-frame allocator the page-table code uses for
/// intermediate tables (call at boot, before any `map_page`).
pub fn set_frame_allocator(alloc: fn() -> u64, free: fn(u64)) {
    FRAME_ALLOC_FN.store(alloc as usize, Ordering::Relaxed);
    FRAME_FREE_FN.store(free as usize, Ordering::Relaxed);
}

struct DirectMapper;
// SAFETY: on bare metal every physical frame is reachable at phys + base.
unsafe impl paging::PhysMapper for DirectMapper {
    unsafe fn table_at(&self, phys: u64) -> *mut paging::PageTable {
        (phys + DIRECT_MAP_BASE.load(Ordering::Relaxed)) as *mut paging::PageTable
    }
}

struct GlobalFrames;
impl paging::FrameAllocator for GlobalFrames {
    fn alloc_zeroed(&mut self) -> Option<u64> {
        let f = FRAME_ALLOC_FN.load(Ordering::Relaxed);
        if f == 0 {
            return None;
        }
        // SAFETY: the stored value is a valid `fn() -> u64` set by the kernel.
        let alloc: fn() -> u64 = unsafe { core::mem::transmute(f) };
        let phys = alloc();
        if phys == 0 {
            return None;
        }
        // Zero the fresh frame through the direct map.
        let base = DIRECT_MAP_BASE.load(Ordering::Relaxed);
        unsafe { core::ptr::write_bytes((phys + base) as *mut u8, 0, paging::PAGE_SIZE as usize) };
        Some(phys)
    }
    fn free(&mut self, phys: u64) {
        let f = FRAME_FREE_FN.load(Ordering::Relaxed);
        if f != 0 {
            let free: fn(u64) = unsafe { core::mem::transmute(f) };
            free(phys);
        }
    }
}

/// Read the active top-level page table (PML4) physical address from CR3.
fn read_cr3() -> u64 {
    let cr3: u64;
    unsafe { core::arch::asm!("mov {}, cr3", out(reg) cr3, options(nomem, nostack, preserves_flags)) };
    cr3 & paging::PTE_ADDR_MASK
}

/// Flush a single TLB entry for `virt`.
unsafe fn flush_tlb(virt: u64) {
    unsafe { core::arch::asm!("invlpg [{}]", in(reg) virt, options(nostack, preserves_flags)) };
}

pub unsafe fn map_page(virt: VirtAddr, phys: PhysAddr, flags: PageFlags) {
    let root = read_cr3();
    // A mapping failure here is a kernel bug (double-map / OOM); the caller's
    // contract says `virt` is free. We ignore the Result rather than panic on
    // the early boot path; higher layers validate inputs.
    let _ = unsafe { paging::map_page_in(root, virt, phys, flags, &DirectMapper, &mut GlobalFrames) };
    unsafe { flush_tlb(virt.0) };
}

pub unsafe fn unmap_page(virt: VirtAddr) {
    let root = read_cr3();
    let _ = unsafe { paging::unmap_page_in(root, virt, &DirectMapper) };
    unsafe { flush_tlb(virt.0) };
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
    unsafe {
        core::arch::asm!("out dx, ax", in("dx") 0x604u16, in("ax") 0x2000u16,
            options(nomem, nostack, preserves_flags));
    }
    loop { unsafe { core::arch::asm!("hlt", options(nomem, nostack, preserves_flags)); } }
}

pub fn reboot(_reason: RebootReason) -> ! {
    // Keyboard controller reset (pulse the CPU reset line via port 0x64).
    unsafe {
        core::arch::asm!("out dx, al", in("dx") 0x64u16, in("al") 0xFEu8,
            options(nomem, nostack, preserves_flags));
    }
    loop { unsafe { core::arch::asm!("hlt", options(nomem, nostack, preserves_flags)); } }
}

pub fn set_cpu_frequency(_cpu: u32, _freq_khz: u32) {
    // Would write to MSR or ACPI _PSS package.
}

pub fn get_cpu_frequency(_cpu: u32) -> u32 {
    3_000_000
}
