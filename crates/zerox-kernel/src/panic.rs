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
    // TODO(M3): dump `info` (message + location) over the serial port before
    // halting. For now we cannot print, so we just stop.
    let _ = info;
    loop {
        core::hint::spin_loop();
    }
}
