//! Panic handling for the bare-metal kernel.
//!
//! On bare metal a panic is unrecoverable: there is no parent process to unwind
//! into. We log the panic message (once serial logging exists, M3/M7) and then
//! halt the CPU forever. On the host build the standard library provides panic
//! machinery, so this handler is compiled out.
//!
//! The handler is intentionally arch-agnostic for now (a spin loop). Milestone
//! M3 replaces the body with a serial-port register/message dump before the
//! halt, and M7 wires it to the 0x3F8 serial console.

#[cfg(all(feature = "bare", not(test)))]
#[panic_handler]
fn on_panic(info: &core::panic::PanicInfo) -> ! {
    use core::fmt::Write;

    // The panicking task may have been holding the serial lock; reclaim it so we
    // can still report. Safe: we are panicking and (for now) single-threaded.
    unsafe { crate::serial::force_unlock() };
    let mut port = crate::serial::COM1_PORT.lock();
    let _ = write!(port, "\n\n*** KERNEL PANIC ***\n");
    if let Some(loc) = info.location() {
        let _ = write!(port, "  at {}:{}:{}\n", loc.file(), loc.line(), loc.column());
    }
    let _ = write!(port, "  {}\n", info.message());
    let _ = write!(port, "  system halted.\n");
    drop(port);

    // Halt forever with interrupts disabled.
    loop {
        unsafe { core::arch::asm!("cli; hlt", options(nomem, nostack)) };
    }
}
