//! Timer abstraction — unified microsecond-precision timer API.
//!
//! ARM64 uses the architectural Generic Timer; x86_64 prefers the TSC for fast
//! reads and HPET for wall-clock, with the APIC timer driving scheduler ticks.

/// Read the current monotonic time in nanoseconds.
pub fn read_time_ns() -> u64 {
    crate::arch::read_time_ns()
}

/// Read the current monotonic time in microseconds.
pub fn read_time_us() -> u64 {
    read_time_ns() / 1000
}

/// Read the current monotonic time in milliseconds.
pub fn read_time_ms() -> u64 {
    read_time_ns() / 1_000_000
}

/// Set a one-shot deadline in nanoseconds from now. When the deadline fires,
/// the timer IRQ is raised.
///
/// # Safety
/// The timer handler must be registered before calling this.
pub unsafe fn set_deadline_ns(ns: u64) {
    crate::arch::set_deadline_ns(ns);
}

/// Cancel a pending deadline.
pub unsafe fn cancel_deadline() {
    crate::arch::cancel_deadline();
}

/// Configure a periodic timer at the given frequency (Hz). Used by the
/// scheduler's tick source.
///
/// # Safety
/// The timer handler must be registered before calling this.
pub unsafe fn set_periodic(hz: u32) {
    crate::arch::set_periodic(hz);
}
