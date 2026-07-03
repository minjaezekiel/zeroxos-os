//! x86_64 fast system calls (`syscall` / `sysret`).
//!
//! User code enters the kernel with the `syscall` instruction: the CPU loads
//! `CS`/`SS` from the STAR MSR, jumps to the address in LSTAR, saves the user
//! `RIP` in `rcx` and `RFLAGS` in `r11`, and masks the flags in FMASK. Return is
//! via `sysret`, which restores `RIP`/`RFLAGS` from `rcx`/`r11` and the user
//! segments from STAR.
//!
//! The **dispatch table** ([`dispatch`]) is pure routing logic and is
//! unit-tested on the host. The MSR programming ([`init_syscalls`]) and the
//! naked entry trampoline ([`syscall_entry`]) are privileged and compiled only
//! for bare metal.
//!
//! ## Current limitation (finalized in Phase 2)
//! The entry stub does **not** yet switch to a per-CPU kernel stack (it runs the
//! handler on the entry stack) and assumes a single CPU. That is sufficient for
//! Phase 1 — nothing issues a `syscall` until the first userspace process
//! (Phase 2), at which point the TSS `rsp0` / per-CPU stack switch is wired in.

/// Linux-compatible-ish syscall numbers (kept small for now).
pub const SYS_EXIT: u64 = 0;
pub const SYS_WRITE: u64 = 1;
pub const SYS_READ: u64 = 2;

/// Returned for an unknown syscall number (`-ENOSYS`).
pub const ENOSYS: i64 = -38;

/// Route a system call to its handler and return the result (negative =
/// `-errno`). Pure logic: no privileged state, so it is host-testable. The
/// concrete effects (writing to a console/fd, terminating a task) are filled in
/// as the VFS and process model land (Phase 2); for now the routing and return
/// contract are what we validate.
pub fn dispatch(nr: u64, a0: u64, a1: u64, a2: u64, _a3: u64, _a4: u64, _a5: u64) -> i64 {
    match nr {
        SYS_EXIT => {
            // TODO(Phase 2): terminate the current task with exit code `a0`.
            let _code = a0 as i32;
            0
        }
        SYS_WRITE => {
            // TODO(Phase 2): write `a2` bytes from user pointer `a1` to fd `a0`.
            // For now, report the requested length as written.
            let _fd = a0;
            let _buf = a1;
            let len = a2;
            len as i64
        }
        SYS_READ => {
            // TODO(Phase 2): read into user pointer `a1`. No input source yet.
            0
        }
        _ => ENOSYS,
    }
}

// ---------------------------------------------------------------------------
// Bare-metal: MSR setup + entry trampoline.
// ---------------------------------------------------------------------------

#[cfg(feature = "bare")]
mod bare {
    use super::*;

    const MSR_EFER: u32 = 0xC000_0080;
    const MSR_STAR: u32 = 0xC000_0081;
    const MSR_LSTAR: u32 = 0xC000_0082;
    const MSR_FMASK: u32 = 0xC000_0084;

    const EFER_SCE: u64 = 1 << 0; // System Call Extensions (enable syscall/sysret)
    const EFER_NXE: u64 = 1 << 11; // No-Execute Enable (for NX page flags)

    // RFLAGS bits masked on syscall entry: IF (interrupts) and DF (direction).
    const RFLAGS_IF: u64 = 1 << 9;
    const RFLAGS_DF: u64 = 1 << 10;

    unsafe fn rdmsr(msr: u32) -> u64 {
        let (lo, hi): (u32, u32);
        unsafe {
            core::arch::asm!("rdmsr", in("ecx") msr, out("eax") lo, out("edx") hi,
                options(nomem, nostack, preserves_flags));
        }
        ((hi as u64) << 32) | (lo as u64)
    }

    unsafe fn wrmsr(msr: u32, value: u64) {
        let lo = value as u32;
        let hi = (value >> 32) as u32;
        unsafe {
            core::arch::asm!("wrmsr", in("ecx") msr, in("eax") lo, in("edx") hi,
                options(nomem, nostack, preserves_flags));
        }
    }

    /// Enable `syscall`/`sysret` and point LSTAR at [`syscall_entry`].
    ///
    /// # Safety
    /// Ring 0, once, after the GDT is loaded (STAR selectors must be valid).
    pub unsafe fn init_syscalls() {
        unsafe {
            // Enable SCE (syscall) and NXE (so NX page bits are honored).
            wrmsr(MSR_EFER, rdmsr(MSR_EFER) | EFER_SCE | EFER_NXE);
            // STAR: [63:48] = sysret base, [47:32] = syscall base.
            let star = ((super::super::gdt::STAR_SYSRET_BASE as u64) << 48)
                | ((super::super::gdt::STAR_SYSCALL_BASE as u64) << 32);
            wrmsr(MSR_STAR, star);
            // Coerce to a fn pointer before the integer cast (avoids the
            // fn-item-to-integer lint).
            let entry: unsafe extern "C" fn() = syscall_entry;
            wrmsr(MSR_LSTAR, entry as usize as u64);
            wrmsr(MSR_FMASK, RFLAGS_IF | RFLAGS_DF);
        }
    }

    /// The register frame the entry stub builds for the Rust handler. Field
    /// order matches the push order in [`syscall_entry`] (low address first).
    #[repr(C)]
    pub struct SyscallFrame {
        pub r9: u64,   // arg5
        pub r8: u64,   // arg4
        pub r10: u64,  // arg3
        pub rdx: u64,  // arg2
        pub rsi: u64,  // arg1
        pub rdi: u64,  // arg0
        pub nr: u64,   // rax: syscall number
        pub rflags: u64, // r11
        pub rip: u64,  // rcx
    }

    /// Rust side of the syscall: unpack the frame and dispatch. Return value
    /// goes back to user in `rax`.
    #[no_mangle]
    pub extern "C" fn syscall_handler(frame: *const SyscallFrame) -> i64 {
        // SAFETY: the entry stub always passes a valid frame pointer.
        let f = unsafe { &*frame };
        // TODO(Phase 2): capability check against the calling task here.
        dispatch(f.nr, f.rdi, f.rsi, f.rdx, f.r10, f.r8, f.r9)
    }

    /// `syscall` entry point (installed in LSTAR). Saves the user context into a
    /// [`SyscallFrame`], calls [`syscall_handler`], restores, and `sysret`s.
    ///
    /// # Safety
    /// Entered only by the CPU on a `syscall` instruction.
    #[unsafe(naked)]
    pub unsafe extern "C" fn syscall_entry() {
        core::arch::naked_asm!(
            "swapgs",           // switch to the kernel GS base
            // Build a SyscallFrame on the current stack (high → low address).
            "push rcx",         // user RIP
            "push r11",         // user RFLAGS
            "push rax",         // syscall number
            "push rdi",         // arg0
            "push rsi",         // arg1
            "push rdx",         // arg2
            "push r10",         // arg3
            "push r8",          // arg4
            "push r9",          // arg5
            "mov rdi, rsp",     // &SyscallFrame → first C arg
            "call syscall_handler",
            // rax now holds the return value. Restore the saved registers.
            "pop r9",
            "pop r8",
            "pop r10",
            "pop rdx",
            "pop rsi",
            "pop rdi",
            "add rsp, 8",       // discard saved syscall number (rax = result)
            "pop r11",          // user RFLAGS
            "pop rcx",          // user RIP
            "swapgs",           // back to the user GS base
            "sysretq",
        );
    }
}

#[cfg(feature = "bare")]
pub use bare::{init_syscalls, syscall_entry, SyscallFrame};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_syscall_is_enosys() {
        assert_eq!(dispatch(9999, 0, 0, 0, 0, 0, 0), ENOSYS);
    }

    #[test]
    fn write_reports_length() {
        // write(fd=1, buf=0xDEAD, len=42) → 42 bytes "written".
        assert_eq!(dispatch(SYS_WRITE, 1, 0xDEAD, 42, 0, 0, 0), 42);
    }

    #[test]
    fn read_returns_eof_for_now() {
        assert_eq!(dispatch(SYS_READ, 0, 0x1000, 128, 0, 0, 0), 0);
    }

    #[test]
    fn exit_succeeds() {
        assert_eq!(dispatch(SYS_EXIT, 7, 0, 0, 0, 0, 0), 0);
    }
}
