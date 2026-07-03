//! CPU control surface — halt, yield, TLB, interrupts, cycle counter.

/// Halt the CPU until the next interrupt. Saves power on idle cores.
///
/// # Safety
/// Must only be called from a kernel context where it is safe to wait.
pub unsafe fn halt() { crate::arch::cpu_halt(); }

/// Yield the current CPU to the scheduler. On multi-core systems this is a hint;
/// on single-core systems it triggers a context switch.
pub fn yield_cpu() { crate::arch::cpu_yield(); }

/// Invalidate the TLB. Called after page table changes.
///
/// # Safety
/// Misuse can leave the CPU in an inconsistent state.
pub unsafe fn invalidate_tlb() { crate::arch::invalidate_tlb(); }

/// Enable CPU interrupts.
///
/// # Safety
/// Enabling interrupts in an unsafe context can cause reentrancy bugs.
pub unsafe fn enable_interrupt() { crate::arch::enable_interrupt(); }

/// Disable CPU interrupts. Returns the previous interrupt state so it can be restored.
pub unsafe fn disable_interrupt() -> bool { crate::arch::disable_interrupt() }

/// Read the architecture's cycle counter (TSC on x86, PMCCNTR on ARM).
/// Used for high-resolution timing and profiler sampling.
pub fn read_cycle_counter() -> u64 { crate::arch::read_cycle_counter() }

/// Read the CPU's unique identifier (for telemetry).
pub fn cpu_id() -> u32 { crate::arch::cpu_id() }
