//! x86_64 context switching.
//!
//! zeroxos uses **cooperative kernel context switches**: a task's saved state is
//! just its kernel stack pointer (`rsp`) and its address space root (`cr3`). The
//! callee-saved registers live *on the task's own kernel stack* between
//! switches, pushed and popped by [`switch_to`]. This is the classic, minimal,
//! correct design — the roadmap's illustrative stub tried to `mov rip` directly
//! (impossible) and stored registers in a struct; we store them on the stack and
//! let `ret` reload the instruction pointer.
//!
//! A brand-new task has never run, so it has no saved frame. [`init_kernel_task`]
//! forges one: it lays down six zeroed callee-saved slots and a return address
//! pointing at the task's entry function, so the *first* `switch_to` into it
//! naturally `ret`s into the entry point on a fresh stack.
//!
//! FPU/SSE state is intentionally not saved here: the kernel target is
//! soft-float / no-SSE, so kernel code touches no vector registers. Lazy FPU
//! save/restore (via `cr0.TS`) for *user* threads is a later optimization.

/// The minimal saved state of a task between context switches.
///
/// Field order is load-bearing: [`switch_to`]'s assembly reads `rsp` at offset 0
/// and `cr3` at offset 8. The `offset_of` tests below guard that contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct TaskContext {
    /// Saved kernel stack pointer (points at the saved callee-saved frame).
    pub rsp: u64,
    /// Address space root (CR3 / PML4 physical address).
    pub cr3: u64,
}

impl TaskContext {
    /// An empty context (used for the initial/boot task, whose state is captured
    /// on its first outbound `switch_to`).
    pub const fn empty() -> Self {
        TaskContext { rsp: 0, cr3: 0 }
    }
}

/// Number of callee-saved registers `switch_to` pushes (rbp, rbx, r12–r15).
pub const SAVED_REGS: usize = 6;

/// Forge the initial stack frame for a fresh kernel task so the first
/// [`switch_to`] into it lands at `entry` on a clean stack.
///
/// `stack_top` is the (16-byte aligned) highest address of the task's kernel
/// stack. `entry` never returns (a returning kernel task is a bug). Returns the
/// [`TaskContext`] to hand to `switch_to`.
///
/// # Safety
/// `[stack_top - 8*(SAVED_REGS+1), stack_top)` must be valid, writable, and
/// exclusively owned by this task.
pub unsafe fn init_kernel_task(
    stack_top: u64,
    entry: extern "C" fn() -> !,
    cr3: u64,
) -> TaskContext {
    // Layout, low → high address (matches switch_to's pop order then `ret`):
    //   [rsp] r15=0, r14=0, r13=0, r12=0, rbx=0, rbp=0, [return addr]=entry
    let ret_slot = stack_top - 8;
    unsafe {
        core::ptr::write(ret_slot as *mut u64, entry as usize as u64);
        let regs = stack_top - 8 * (SAVED_REGS as u64 + 1);
        for i in 0..SAVED_REGS as u64 {
            core::ptr::write((regs + i * 8) as *mut u64, 0);
        }
        TaskContext { rsp: regs, cr3 }
    }
}

/// Switch execution from `prev` to `next`.
///
/// Saves the current callee-saved registers on the current stack, records the
/// stack pointer into `*prev`, switches `cr3` if the address space differs,
/// loads `next`'s stack, restores its callee-saved registers, and `ret`s into
/// wherever `next` last yielded (or its entry point, for a fresh task).
///
/// # Safety
/// Must run in ring 0 with interrupts disabled. `prev` and `next` must be valid,
/// and `next` must describe a task whose stack was set up by [`init_kernel_task`]
/// or a prior `switch_to`.
#[cfg(feature = "bare")]
#[unsafe(naked)]
pub unsafe extern "C" fn switch_to(prev: *mut TaskContext, next: *const TaskContext) {
    // System V: prev = rdi, next = rsi. TaskContext.rsp @ 0, .cr3 @ 8.
    core::arch::naked_asm!(
        // Save callee-saved registers onto the current (prev) stack.
        "push rbp",
        "push rbx",
        "push r12",
        "push r13",
        "push r14",
        "push r15",
        // Save current stack pointer into prev.rsp.
        "mov [rdi + 0], rsp",
        // Switch address space if next.cr3 differs from the current one.
        "mov rax, [rsi + 8]",
        "mov rcx, cr3",
        "cmp rax, rcx",
        "je 2f",
        "mov cr3, rax",
        "2:",
        // Load next's stack pointer.
        "mov rsp, [rsi + 0]",
        // Restore next's callee-saved registers (reverse push order).
        "pop r15",
        "pop r14",
        "pop r13",
        "pop r12",
        "pop rbx",
        "pop rbp",
        // Return into next's saved RIP (return address on its stack).
        "ret",
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::mem::offset_of;

    #[test]
    fn field_offsets_match_switch_to_asm() {
        // switch_to hard-codes these offsets; keep them in sync.
        assert_eq!(offset_of!(TaskContext, rsp), 0);
        assert_eq!(offset_of!(TaskContext, cr3), 8);
    }

    extern "C" fn dummy_entry() -> ! {
        loop {}
    }

    #[test]
    fn init_kernel_task_forges_a_correct_frame() {
        // A 32-slot stack; treat its address range as the task's kernel stack.
        let mut stack = [0u64; 32];
        let base = stack.as_mut_ptr() as u64;
        let stack_top = base + (stack.len() as u64) * 8;

        let ctx = unsafe { init_kernel_task(stack_top, dummy_entry, 0xCAFE_F000) };

        // rsp points 7 slots below the top (6 saved regs + return address).
        assert_eq!(ctx.rsp, stack_top - 8 * (SAVED_REGS as u64 + 1));
        assert_eq!(ctx.cr3, 0xCAFE_F000);

        // The return-address slot holds the entry function.
        let ret_slot = (stack_top - 8) as *const u64;
        assert_eq!(unsafe { *ret_slot }, dummy_entry as usize as u64);

        // The six callee-saved slots are zeroed.
        for i in 0..SAVED_REGS as u64 {
            let slot = (ctx.rsp + i * 8) as *const u64;
            assert_eq!(unsafe { *slot }, 0);
        }
    }

    #[test]
    fn empty_context_is_zero() {
        let c = TaskContext::empty();
        assert_eq!(c.rsp, 0);
        assert_eq!(c.cr3, 0);
    }
}
