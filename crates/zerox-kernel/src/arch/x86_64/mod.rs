//! x86_64 architecture support: descriptor tables and CPU control.
//!
//! [`init`] installs the GDT (with a TSS) and the IDT, giving the CPU valid
//! segments and a full set of exception handlers. It is called once, early on
//! the bare-metal boot path (see `zeroxos-boot::_start`), before interrupts are
//! enabled.

pub mod context;
pub mod gdt;
pub mod idt;
pub mod syscall;
pub mod tss;

/// Reload the segment registers after a new GDT is loaded.
///
/// `CS` cannot be set with a `mov`; it is reloaded via a far return that pops a
/// new `CS:RIP`. The data segment registers are set directly.
///
/// # Safety
/// A valid GDT with the given selectors must already be loaded. Ring 0 only.
#[cfg(feature = "bare")]
pub unsafe fn reload_segments(code_selector: u16, data_selector: u16) {
    unsafe {
        // Data segments (DS/ES/SS/FS/GS) load directly.
        core::arch::asm!(
            "mov ds, {sel:x}",
            "mov es, {sel:x}",
            "mov ss, {sel:x}",
            "mov fs, {sel:x}",
            "mov gs, {sel:x}",
            sel = in(reg) data_selector,
            options(nostack, preserves_flags),
        );
        // CS via far return: push new selector + a return address, then `retfq`.
        core::arch::asm!(
            "lea {tmp}, [rip + 2f]",
            "push {code}",
            "push {tmp}",
            "retfq",
            "2:",
            code = in(reg) code_selector as u64,
            tmp = lateout(reg) _,
            options(preserves_flags),
        );
    }
}

/// The kernel's descriptor tables. They must live for the lifetime of the
/// kernel, so they are `static`. Built and loaded once in [`init`].
#[cfg(feature = "bare")]
mod tables {
    use super::{gdt::Gdt, idt::Idt, tss::TaskStateSegment};
    use core::cell::UnsafeCell;

    /// A minimal `Sync` cell for single-core boot-time table setup. Access is
    /// only sound before other CPUs come up (SMP is Phase 5); until then the
    /// boot CPU is the sole writer/reader.
    pub struct BootCell<T>(UnsafeCell<T>);
    // SAFETY: only touched on the boot CPU before interrupts/SMP are enabled.
    unsafe impl<T> Sync for BootCell<T> {}
    impl<T> BootCell<T> {
        pub const fn new(value: T) -> Self {
            BootCell(UnsafeCell::new(value))
        }
        #[allow(clippy::mut_from_ref)]
        pub fn get(&self) -> *mut T {
            self.0.get()
        }
    }

    pub static TSS: BootCell<TaskStateSegment> = BootCell::new(TaskStateSegment::new());
    pub static IDT: BootCell<Idt> = BootCell::new(Idt::new());
    pub static GDT: BootCell<Option<Gdt>> = BootCell::new(None);
}

/// Install and load the GDT (+TSS) and IDT.
///
/// # Safety
/// Call exactly once, on the boot CPU, before enabling interrupts.
#[cfg(feature = "bare")]
pub unsafe fn init() {
    use tables::{GDT, IDT, TSS};
    unsafe {
        // IDT: fill exception vectors, then load.
        let idt = &mut *IDT.get();
        idt.install_exceptions();
        (*IDT.get()).load();

        // GDT: build around the static TSS, store it, then load (this also
        // reloads the segment registers and loads the TSS via `ltr`).
        let tss: &'static tss::TaskStateSegment = &*TSS.get();
        *GDT.get() = Some(gdt::Gdt::new(tss));
        if let Some(gdt) = &*GDT.get() {
            gdt.load();
        }

        // Enable fast system calls now that the GDT/STAR selectors are valid.
        syscall::init_syscalls();
    }
}
