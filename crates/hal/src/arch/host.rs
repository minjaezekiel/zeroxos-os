//! Host-simulation HAL implementation.
//!
//! When the `host` feature is enabled, the HAL runs as a userspace library.
//! This is used by `zerox-sim` to boot the kernel inside a normal process on
//! a developer's machine, without needing a hypervisor or actual hardware.

use crate::interrupt::{Handler, Irq, IrqConfig};
use crate::memory::{PageFlags, PhysAddr, VirtAddr};
use crate::power::{RebootReason, SleepState};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

static START: OnceLock<Instant> = OnceLock::new();
static CYCLE_COUNTER: AtomicU64 = AtomicU64::new(0);

fn start() -> Instant {
    *START.get_or_init(Instant::now)
}

pub unsafe fn cpu_halt() {
    std::thread::yield_now();
}

pub fn cpu_yield() {
    std::thread::yield_now();
}

pub unsafe fn invalidate_tlb() {}
pub unsafe fn enable_interrupt() {}
pub unsafe fn disable_interrupt() -> bool { false }

pub fn read_cycle_counter() -> u64 {
    CYCLE_COUNTER.fetch_add(1, Ordering::Relaxed) + 1
}

pub fn cpu_id() -> u32 { 0 }

pub unsafe fn map_page(_virt: VirtAddr, _phys: PhysAddr, _flags: PageFlags) {}
pub unsafe fn unmap_page(_virt: VirtAddr) {}

pub fn allocate_dma(size: usize, _align: usize) -> Option<crate::memory::DmaRegion> {
    use std::alloc::{alloc, Layout};
    let layout = Layout::from_size_align(size, 4096).ok()?;
    unsafe {
        let ptr = alloc(layout);
        if ptr.is_null() { return None; }
        let nonnull = std::ptr::NonNull::new_unchecked(ptr);
        Some(crate::memory::DmaRegion {
            phys: PhysAddr(ptr as u64),
            virt: nonnull,
            size,
            is_cache_coherent: true,
        })
    }
}

pub unsafe fn flush_cache(_addr: VirtAddr, _size: usize) {}
pub unsafe fn invalidate_cache(_addr: VirtAddr, _size: usize) {}

pub unsafe fn register_irq(_irq: Irq, _config: IrqConfig, _handler: Handler) {}
pub unsafe fn unregister_irq(_irq: Irq) {}
pub unsafe fn send_ipi(_cpu: u32, _vector: Irq) {}
pub unsafe fn ack_irq(_irq: Irq) {}
pub unsafe fn eoi(_irq: Irq) {}

pub fn read_time_ns() -> u64 {
    start().elapsed().as_nanos() as u64
}

pub unsafe fn set_deadline_ns(_ns: u64) {}
pub unsafe fn cancel_deadline() {}
pub unsafe fn set_periodic(_hz: u32) {}

pub unsafe fn sleep(state: SleepState) {
    let ms = match state {
        SleepState::Idle => 1,
        SleepState::Suspend => 100,
        SleepState::Deep => 1000,
    };
    std::thread::sleep(std::time::Duration::from_millis(ms));
}

pub unsafe fn hibernate() {
    eprintln!("[hal:host] hibernate requested — simulating");
}

pub fn shutdown() -> ! {
    eprintln!("[hal:host] shutdown");
    std::process::exit(0);
}

pub fn reboot(reason: RebootReason) -> ! {
    eprintln!("[hal:host] reboot ({:?})", reason);
    std::process::exit(0);
}

pub fn set_cpu_frequency(_cpu: u32, freq_khz: u32) {
    eprintln!("[hal:host] set_cpu_frequency({}_kHz)", freq_khz);
}

pub fn get_cpu_frequency(_cpu: u32) -> u32 {
    3_000_000 // 3 GHz
}
