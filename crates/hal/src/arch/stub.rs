//! Fallback stub implementation.
//!
//! Used when no architecture is selected (e.g. when building docs on a
//! non-x86, non-ARM host without the `host` feature). All operations panic.

use crate::interrupt::{Handler, Irq, IrqConfig};
use crate::memory::{PageFlags, PhysAddr, VirtAddr};
use crate::power::{RebootReason, SleepState};

fn noarch() -> ! { panic!("no HAL implementation for this target — enable 'host' feature") }

pub unsafe fn cpu_halt() { noarch() }
pub fn cpu_yield() { noarch() }
pub unsafe fn invalidate_tlb() { noarch() }
pub unsafe fn enable_interrupt() { noarch() }
pub unsafe fn disable_interrupt() -> bool { noarch() }
pub fn read_cycle_counter() -> u64 { noarch() }
pub fn cpu_id() -> u32 { noarch() }

pub unsafe fn map_page(_: VirtAddr, _: PhysAddr, _: PageFlags) { noarch() }
pub unsafe fn unmap_page(_: VirtAddr) { noarch() }
pub fn allocate_dma(_: usize, _: usize) -> Option<crate::memory::DmaRegion> { noarch() }
pub unsafe fn flush_cache(_: VirtAddr, _: usize) { noarch() }
pub unsafe fn invalidate_cache(_: VirtAddr, _: usize) { noarch() }

pub unsafe fn register_irq(_: Irq, _: IrqConfig, _: Handler) { noarch() }
pub unsafe fn unregister_irq(_: Irq) { noarch() }
pub unsafe fn send_ipi(_: u32, _: Irq) { noarch() }
pub unsafe fn ack_irq(_: Irq) { noarch() }
pub unsafe fn eoi(_: Irq) { noarch() }

pub fn read_time_ns() -> u64 { noarch() }
pub unsafe fn set_deadline_ns(_: u64) { noarch() }
pub unsafe fn cancel_deadline() { noarch() }
pub unsafe fn set_periodic(_: u32) { noarch() }

pub unsafe fn sleep(_: SleepState) { noarch() }
pub unsafe fn hibernate() { noarch() }
pub fn shutdown() -> ! { noarch() }
pub fn reboot(_: RebootReason) -> ! { noarch() }
pub fn set_cpu_frequency(_: u32, _: u32) { noarch() }
pub fn get_cpu_frequency(_: u32) -> u32 { noarch() }
