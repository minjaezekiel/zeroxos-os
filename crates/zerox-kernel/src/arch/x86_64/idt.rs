//! x86_64 Interrupt Descriptor Table (IDT).
//!
//! The IDT maps each of the 256 interrupt vectors to a handler. Vectors 0–31
//! are CPU exceptions (divide error, page fault, double fault, …); 32–255 are
//! available for hardware IRQs and software interrupts.
//!
//! Each entry is a 16-byte gate descriptor. The bit layout (offset split across
//! three fields, selector, IST index, type/attribute byte) is pure arithmetic
//! and is unit-tested on the host. The naked ISR stubs, the common dispatch
//! trampoline, and `lidt` are privileged and compiled only for bare metal.

/// Number of IDT vectors.
pub const IDT_LEN: usize = 256;

/// A 16-byte IDT gate descriptor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C, packed)]
pub struct IdtEntry {
    offset_low: u16,
    selector: u16,
    ist: u8,
    type_attr: u8,
    offset_mid: u16,
    offset_high: u32,
    reserved: u32,
}

/// Gate type nibble: 64-bit interrupt gate (clears IF on entry).
pub const GATE_INTERRUPT: u8 = 0xE;
/// Gate type nibble: 64-bit trap gate (leaves IF unchanged).
pub const GATE_TRAP: u8 = 0xF;

impl IdtEntry {
    /// An empty, not-present entry.
    pub const fn missing() -> Self {
        IdtEntry {
            offset_low: 0,
            selector: 0,
            ist: 0,
            type_attr: 0,
            offset_mid: 0,
            offset_high: 0,
            reserved: 0,
        }
    }

    /// Encode a handler entry.
    ///
    /// - `handler` — absolute virtual address of the ISR.
    /// - `selector` — code segment selector to load (kernel code).
    /// - `ist` — IST index 0..=7 (0 = don't switch stacks).
    /// - `gate` — [`GATE_INTERRUPT`] or [`GATE_TRAP`].
    /// - `dpl` — descriptor privilege level (0 = kernel-only, 3 = usable via `int` from ring 3).
    pub const fn new(handler: u64, selector: u16, ist: u8, gate: u8, dpl: u8) -> Self {
        IdtEntry {
            offset_low: (handler & 0xFFFF) as u16,
            selector,
            ist: ist & 0x7,
            // present | DPL | 0 | gate-type
            type_attr: 0x80 | ((dpl & 0x3) << 5) | (gate & 0xF),
            offset_mid: ((handler >> 16) & 0xFFFF) as u16,
            offset_high: ((handler >> 32) & 0xFFFF_FFFF) as u32,
            reserved: 0,
        }
    }

    /// Reassemble the 64-bit handler offset (mainly for tests).
    pub fn offset(&self) -> u64 {
        (self.offset_low as u64)
            | ((self.offset_mid as u64) << 16)
            | ((self.offset_high as u64) << 32)
    }
}

/// The pointer loaded by `lidt`.
#[repr(C, packed)]
pub struct IdtPointer {
    pub limit: u16,
    pub base: u64,
}

/// The 256-entry interrupt descriptor table.
#[repr(C, align(16))]
pub struct Idt {
    entries: [IdtEntry; IDT_LEN],
}

impl Idt {
    pub const fn new() -> Self {
        Idt { entries: [IdtEntry::missing(); IDT_LEN] }
    }

    /// Set the handler for `vector`.
    pub fn set_handler(&mut self, vector: usize, entry: IdtEntry) {
        self.entries[vector] = entry;
    }

    fn pointer(&self) -> IdtPointer {
        IdtPointer {
            limit: (core::mem::size_of::<[IdtEntry; IDT_LEN]>() - 1) as u16,
            base: self.entries.as_ptr() as u64,
        }
    }
}

impl Default for Idt {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Bare-metal: ISR stubs, dispatch, and installation.
// ---------------------------------------------------------------------------

/// The register/interrupt frame the common stub builds on the stack and passes
/// to the Rust dispatcher. Field order matches the push order in `isr_common`.
#[cfg(feature = "bare")]
#[derive(Debug)]
#[repr(C)]
pub struct InterruptFrame {
    pub r15: u64, pub r14: u64, pub r13: u64, pub r12: u64,
    pub r11: u64, pub r10: u64, pub r9: u64, pub r8: u64,
    pub rbp: u64, pub rdi: u64, pub rsi: u64,
    pub rdx: u64, pub rcx: u64, pub rbx: u64, pub rax: u64,
    /// Pushed by our stub: the vector number.
    pub vector: u64,
    /// Error code (real one from the CPU, or 0 pushed by our stub).
    pub error_code: u64,
    // Pushed by the CPU on interrupt entry:
    pub rip: u64,
    pub cs: u64,
    pub rflags: u64,
    pub rsp: u64,
    pub ss: u64,
}

/// Rust interrupt/exception dispatcher. Called by `isr_common` with a pointer to
/// the [`InterruptFrame`].
///
/// For milestone M3 this records the fault and halts. A serial register dump is
/// added in M7 once the 0x3F8 console logger exists; page-fault handling (demand
/// paging / CoW) is wired in M4.
///
/// # Safety
/// `frame` must point at a valid `InterruptFrame` built by `isr_common`.
#[cfg(feature = "bare")]
#[no_mangle]
pub extern "C" fn isr_dispatch(frame: *const InterruptFrame) {
    // SAFETY: the stub always passes a valid frame pointer.
    let vector = unsafe { (*frame).vector };
    let _ = vector;
    // TODO(M7): print vector, error code, and the register dump over serial.
    // TODO(M4): if vector == 14 (page fault), attempt demand paging / CoW.
    loop {
        unsafe { core::arch::asm!("cli; hlt", options(nomem, nostack)) };
    }
}

// The common tail shared by every ISR stub: save GP registers, call the Rust
// dispatcher with a pointer to the frame, restore, and `iretq`.
#[cfg(feature = "bare")]
core::arch::global_asm!(
    ".global isr_common",
    "isr_common:",
    "push rax", "push rbx", "push rcx", "push rdx",
    "push rsi", "push rdi", "push rbp",
    "push r8",  "push r9",  "push r10", "push r11",
    "push r12", "push r13", "push r14", "push r15",
    "mov rdi, rsp",            // frame pointer → first arg
    "call isr_dispatch",
    "pop r15", "pop r14", "pop r13", "pop r12",
    "pop r11", "pop r10", "pop r9",  "pop r8",
    "pop rbp", "pop rdi", "pop rsi",
    "pop rdx", "pop rcx", "pop rbx", "pop rax",
    "add rsp, 16",            // discard vector + error code
    "iretq",
);

/// Generate a naked ISR stub. Vectors that push a hardware error code use the
/// `err` form; the rest push a dummy 0 so every frame has the same shape.
#[cfg(feature = "bare")]
macro_rules! isr {
    ($name:ident, $vec:expr, noerr) => {
        #[unsafe(naked)]
        pub extern "C" fn $name() {
            core::arch::naked_asm!(
                "push 0",                 // dummy error code
                concat!("push ", $vec),   // vector
                "jmp isr_common",
            );
        }
    };
    ($name:ident, $vec:expr, err) => {
        #[unsafe(naked)]
        pub extern "C" fn $name() {
            core::arch::naked_asm!(
                concat!("push ", $vec),   // vector (CPU already pushed error code)
                "jmp isr_common",
            );
        }
    };
}

// The 32 CPU exception vectors. Vectors 8, 10–14, 17, 21, 29, 30 push a real
// error code; the rest do not (AMD64 Vol 2 §8.2).
#[cfg(feature = "bare")]
pub mod stubs {
    isr!(isr0, "0", noerr);   isr!(isr1, "1", noerr);
    isr!(isr2, "2", noerr);   isr!(isr3, "3", noerr);
    isr!(isr4, "4", noerr);   isr!(isr5, "5", noerr);
    isr!(isr6, "6", noerr);   isr!(isr7, "7", noerr);
    isr!(isr8, "8", err);     isr!(isr9, "9", noerr);
    isr!(isr10, "10", err);   isr!(isr11, "11", err);
    isr!(isr12, "12", err);   isr!(isr13, "13", err);
    isr!(isr14, "14", err);   isr!(isr15, "15", noerr);
    isr!(isr16, "16", noerr); isr!(isr17, "17", err);
    isr!(isr18, "18", noerr); isr!(isr19, "19", noerr);
    isr!(isr20, "20", noerr); isr!(isr21, "21", err);
    isr!(isr22, "22", noerr); isr!(isr23, "23", noerr);
    isr!(isr24, "24", noerr); isr!(isr25, "25", noerr);
    isr!(isr26, "26", noerr); isr!(isr27, "27", noerr);
    isr!(isr28, "28", noerr); isr!(isr29, "29", err);
    isr!(isr30, "30", err);   isr!(isr31, "31", noerr);
}

#[cfg(feature = "bare")]
impl Idt {
    /// Populate the 32 CPU-exception vectors with our stubs.
    pub fn install_exceptions(&mut self) {
        use stubs::*;
        let handlers: [extern "C" fn(); 32] = [
            isr0, isr1, isr2, isr3, isr4, isr5, isr6, isr7,
            isr8, isr9, isr10, isr11, isr12, isr13, isr14, isr15,
            isr16, isr17, isr18, isr19, isr20, isr21, isr22, isr23,
            isr24, isr25, isr26, isr27, isr28, isr29, isr30, isr31,
        ];
        for (vec, handler) in handlers.iter().enumerate() {
            self.entries[vec] = IdtEntry::new(
                *handler as usize as u64,
                super::gdt::KERNEL_CODE,
                0,
                GATE_INTERRUPT,
                0,
            );
        }
    }

    /// Load this IDT (`lidt`).
    ///
    /// # Safety
    /// The IDT must outlive its use (store it in a `static`; the CPU keeps only
    /// the base pointer). Ring 0 only.
    pub unsafe fn load(&self) {
        let ptr = self.pointer();
        unsafe {
            core::arch::asm!("lidt [{}]", in(reg) &ptr, options(readonly, nostack, preserves_flags));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entry_splits_offset_across_fields() {
        let addr = 0xDEAD_BEEF_1234_5678u64;
        let e = IdtEntry::new(addr, 0x08, 0, GATE_INTERRUPT, 0);
        assert_eq!(e.offset(), addr);
        // {offset_low} = e.offset_low; accessed via reassembly above to avoid
        // taking references to packed fields.
    }

    #[test]
    fn type_attr_encodes_present_dpl_and_gate() {
        let e = IdtEntry::new(0, 0x08, 0, GATE_INTERRUPT, 0);
        assert_eq!(e.type_attr, 0x8E); // present | DPL0 | interrupt gate
        let t = IdtEntry::new(0, 0x08, 0, GATE_TRAP, 3);
        assert_eq!(t.type_attr, 0xEF); // present | DPL3 | trap gate
    }

    #[test]
    fn ist_index_is_masked_to_three_bits() {
        let e = IdtEntry::new(0, 0x08, 0xFF, GATE_INTERRUPT, 0);
        assert_eq!(e.ist, 0x7);
    }

    #[test]
    fn missing_entry_is_not_present() {
        let e = IdtEntry::missing();
        assert_eq!(e.type_attr & 0x80, 0);
    }

    #[test]
    fn idt_has_256_entries() {
        assert_eq!(core::mem::size_of::<[IdtEntry; IDT_LEN]>(), 256 * 16);
    }
}
