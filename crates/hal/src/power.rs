//! Power management — sleep, hibernate, shutdown, reboot, frequency scaling.

/// Power state to enter on `sleep`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SleepState {
    /// CPU idle (C1 on x86, WFI on ARM). Wake on any interrupt.
    Idle,
    /// Shallow sleep — most devices stay powered.
    Suspend,
    /// Deep sleep — only wake sources remain powered.
    Deep,
}

/// Reboot reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RebootReason {
    User,
    Cold,
    Watchdog,
    Panic,
}

/// Enter a sleep state. The CPU wakes on the next interrupt.
///
/// # Safety
/// Must ensure devices are in a quiesced state before deep sleep.
pub unsafe fn sleep(state: SleepState) {
    crate::arch::sleep(state);
}

/// Hibernate: write RAM to disk and power off. Restore on next boot.
///
/// # Safety
/// Filesystem and device state must be flushed first.
pub unsafe fn hibernate() {
    crate::arch::hibernate();
}

/// Power off the system cleanly.
pub fn shutdown() -> ! {
    crate::arch::shutdown();
}

/// Reboot the system.
pub fn reboot(reason: RebootReason) -> ! {
    crate::arch::reboot(reason);
}

/// Set the CPU frequency for the given core. Used by the AI-tuned power manager.
///
/// `freq_khz` is the target frequency in kilohertz.
pub fn set_cpu_frequency(cpu: u32, freq_khz: u32) {
    crate::arch::set_cpu_frequency(cpu, freq_khz);
}

/// Get the current CPU frequency in kilohertz.
pub fn get_cpu_frequency(cpu: u32) -> u32 {
    crate::arch::get_cpu_frequency(cpu)
}
