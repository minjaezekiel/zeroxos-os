//! COM1 serial console (16550 UART) + a `log` backend.
//!
//! On bare metal we have no screen driver at boot, but every PC/QEMU exposes a
//! 16550-compatible UART at I/O port `0x3F8` (COM1). This is the kernel's first
//! output device: the boot log, panics, and CPU-exception dumps all go here, and
//! QEMU's `-serial stdio` forwards it to the terminal.
//!
//! Bare-metal only — the host build logs through `env_logger` instead.

use core::fmt::{self, Write};
use spin::Mutex;

/// COM1 base I/O port.
const COM1: u16 = 0x3F8;

/// Write a byte to an I/O port.
///
/// # Safety
/// Port I/O touches hardware; the caller must own the port.
unsafe fn outb(port: u16, value: u8) {
    unsafe {
        core::arch::asm!("out dx, al", in("dx") port, in("al") value,
            options(nomem, nostack, preserves_flags));
    }
}

/// Read a byte from an I/O port.
///
/// # Safety
/// See [`outb`].
unsafe fn inb(port: u16) -> u8 {
    let value: u8;
    unsafe {
        core::arch::asm!("in al, dx", out("al") value, in("dx") port,
            options(nomem, nostack, preserves_flags));
    }
    value
}

/// A 16550 UART on a given I/O base.
pub struct SerialPort {
    base: u16,
}

impl SerialPort {
    pub const fn new(base: u16) -> Self {
        SerialPort { base }
    }

    /// Program the UART: 38400 baud, 8N1, FIFO on.
    pub fn init(&mut self) {
        unsafe {
            outb(self.base + 1, 0x00); // disable interrupts
            outb(self.base + 3, 0x80); // enable DLAB (set baud divisor)
            outb(self.base + 0, 0x03); // divisor low = 3 → 38400 baud
            outb(self.base + 1, 0x00); // divisor high = 0
            outb(self.base + 3, 0x03); // 8 bits, no parity, one stop bit
            outb(self.base + 2, 0xC7); // enable + clear FIFO, 14-byte threshold
            outb(self.base + 4, 0x0B); // RTS/DSR set, OUT2 (needed for IRQs)
        }
    }

    /// True once the transmit holding register is empty.
    fn can_send(&self) -> bool {
        unsafe { inb(self.base + 5) & 0x20 != 0 }
    }

    /// Write one raw byte (blocking until the UART is ready).
    fn put(&mut self, byte: u8) {
        while !self.can_send() {
            core::hint::spin_loop();
        }
        unsafe { outb(self.base, byte) };
    }

    /// Write a byte, translating `\n` → `\r\n` for terminals.
    pub fn write_byte(&mut self, byte: u8) {
        if byte == b'\n' {
            self.put(b'\r');
        }
        self.put(byte);
    }
}

impl Write for SerialPort {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for byte in s.bytes() {
            self.write_byte(byte);
        }
        Ok(())
    }
}

/// The global COM1 port, guarded by a spinlock.
pub static COM1_PORT: Mutex<SerialPort> = Mutex::new(SerialPort::new(COM1));

/// Initialize COM1. Call once, early in boot.
pub fn init() {
    COM1_PORT.lock().init();
}

/// Format-print to COM1 (used by the `serial_print!` macros).
pub fn _print(args: fmt::Arguments) {
    let _ = COM1_PORT.lock().write_fmt(args);
}

/// Print to the serial console over COM1 without a trailing newline.
#[macro_export]
macro_rules! serial_print {
    ($($arg:tt)*) => { $crate::serial::_print(format_args!($($arg)*)) };
}

/// Print a line to the serial console over COM1.
#[macro_export]
macro_rules! serial_println {
    () => { $crate::serial_print!("\n") };
    ($($arg:tt)*) => { $crate::serial::_print(format_args!("{}\n", format_args!($($arg)*))) };
}

/// A `log::Log` backend that writes records to COM1.
struct SerialLogger;

impl log::Log for SerialLogger {
    fn enabled(&self, _metadata: &log::Metadata) -> bool {
        true
    }
    fn log(&self, record: &log::Record) {
        // A record may originate mid-line, but each log line is self-contained.
        let _ = writeln!(COM1_PORT.lock(), "[{:>5}] {}", record.level(), record.args());
    }
    fn flush(&self) {}
}

static LOGGER: SerialLogger = SerialLogger;

/// Initialize the serial port and route the `log` crate to it. Idempotent-safe:
/// a second `set_logger` simply fails and is ignored.
pub fn init_logger(level: log::LevelFilter) {
    init();
    let _ = log::set_logger(&LOGGER);
    log::set_max_level(level);
}

/// Recover the COM1 lock after a panic (the holder is never coming back) so the
/// panic handler can still print.
///
/// # Safety
/// Only call when the system is already panicking and single-threaded.
pub unsafe fn force_unlock() {
    unsafe { COM1_PORT.force_unlock() };
}
