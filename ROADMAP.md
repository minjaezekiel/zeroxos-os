# zeroxos — Implementation Roadmap

> **Status as of v0.1.0** — 5,430 lines of Rust across 9 crates. Foundational
> skeleton complete. The architecture is sound; what's missing is the million
> small pieces that turn a design into a working system.
>
> This document tracks **what is done**, **what is missing**, and includes
> **stub implementations and recipes** for the next pieces so work can continue
> without re-deriving the design.

---

## Table of contents

1. [Current state — what works today](#1-current-state--what-works-today)
2. [Critical path to booting on real hardware](#2-critical-path-to-booting-on-real-hardware)
3. [Memory management — detailed gaps](#3-memory-management--detailed-gaps)
4. [CPU, SMP, and context switching](#4-cpu-smp-and-context-switching)
5. [Interrupts and system calls](#5-interrupts-and-system-calls)
6. [Process model and ELF loader](#6-process-model-and-elf-loader)
7. [VFS and file descriptors](#7-vfs-and-file-descriptors)
8. [Time subsystem](#8-time-subsystem)
9. [SMP and concurrency primitives](#9-smp-and-concurrency-primitives)
10. [Security hardening](#10-security-hardening)
11. [Bus and device discovery](#11-bus-and-device-discovery)
12. [Device drivers — complete inventory](#12-device-drivers--complete-inventory)
13. [SoC / board support packages](#13-soc--board-support-packages)
14. [Userspace ABI and runtime](#14-userspace-abi-and-runtime)
15. [Shell, coreutils, init](#15-shell-coreutils-init)
16. [agex language completeness](#16-agex-language-completeness)
17. [Development tooling](#17-development-tooling)
18. [Production hardening](#18-production-hardening)
19. [Recommended order of attack](#19-recommended-order-of-attack)
20. [Glossary](#20-glossary)

---

## 1. Current state — what works today

### 1.1 Crates

| Crate | LOC | Status | What it does |
|-------|-----|--------|--------------|
| `agex` | ~1,800 | ✅ compiles programs | Real agex→Rust compiler: lexer, recursive-descent parser, AST, HIR lowering pass, Rust code generator, `agc` CLI |
| `hal` | ~600 | ⚠️ host-only | HAL with three backends: `host` (works), `x86_64` (inline-asm stubs), `aarch64` (inline-asm stubs) |
| `zerox-kernel` | ~1,200 | ⚠️ library only | Hybrid kernel: scheduler, memory manager, IPC, capabilities, process table, driver registry |
| `zerox-fs` | ~350 | ⚠️ in-memory only | zeroxfs: superblock, journal, CRC32, CoW tree, compression format |
| `zerox-runtime` | ~300 | ⚠️ skeleton | Userspace services: window manager, audio, network, power, package, supervisor |
| `zerox-sim` | ~350 | ✅ runs | Host simulator — boots kernel as userspace process, runs demos |
| `apm` | ~80 | ⚠️ CLI stub | Package manager CLI |
| `agdb` | ~50 | ⚠️ CLI stub | Debugger CLI |
| `agprof` | ~80 | ⚠️ CLI stub | Profiler CLI |

### 1.2 Verified working

```bash
$ cargo run --release --bin zerox-sim -- boot
# ✅ Boots in 0 ms, 7 processes, 13 drivers loaded, 256 MB memory

$ cargo run --release --bin zerox-sim -- ipc
# ✅ 84 ns per message (target: <500 ns — 6× under)

$ cargo run --release --bin zerox-sim -- game
# ✅ Gaming mode scheduler picks RT audio thread, frame-deadline aware

$ cargo run --release --bin zerox-sim -- fs
# ✅ Superblock CRC verified, CoW snapshot, journal committed, compression roundtrip

$ cargo run --bin agc -- examples/functions.agex -o funcs.rs && rustc funcs.rs -o funcs && ./funcs
# ✅ add: 30 / multiply: 30 / 5! = 120 / i = 0..4
```

### 1.3 Architectural decisions locked in

These are **stable** and should not be revisited:

- **Hybrid kernel** — scheduler, memory, IPC, security, perf-critical drivers in kernel; filesystem, network, audio, bluetooth, package mgr in userspace
- **Capability-based security** — no root; explicit revocable permissions per process
- **HAL boundary** — single portability surface; adding RISC-V needs only a new HAL
- **Three-policy scheduler** — MLFQ + CFS + RT, gaming-aware with frame-deadline boosting
- **agex language** — Python-readable curly-brace syntax, Rust codegen target, ownership-checked
- **zeroxfs** — CoW, journaled, checksummed, compressed, encrypted
- **Three IPC primitives** — fast messages (<500ns), shared memory, capability objects
- **Three driver models** — kernel-mode, user-mode, paravirtualized

---

## 2. Critical path to booting on real hardware

These are the milestones that block everything else. Each is a prerequisite for
the next.

### Milestone M1 — `#[no_std]` kernel with custom allocator

**Status:** ❌ Not started

The kernel currently uses `std` (via `Vec`, `String`, `spin::Mutex` over `std`).
A real kernel is `#![no_std]` with a custom `#[global_allocator]`.

**Tasks:**
- [ ] Add `#![no_std]` to `zerox-kernel/src/lib.rs`
- [ ] Implement `#[panic_handler]` with panic-on-panic strategy
- [ ] Implement a heap allocator backed by the buddy allocator
- [ ] Replace all `std::sync::Mutex` with `spin::Mutex`
- [ ] Replace `std::vec::Vec` with `alloc::vec::Vec`
- [ ] Replace `std::string::String` with `alloc::string::String`
- [ ] Remove `std::time::Instant` usage in `hal::arch::host`

**Stub implementation — kernel heap allocator:**

```rust
// crates/zerox-kernel/src/alloc.rs
use alloc::alloc::{GlobalAlloc, Layout};
use core::ptr::null_mut;
use spin::Mutex;
use crate::memory::BuddyAllocator;

pub struct KernelAlloc;

unsafe impl GlobalAlloc for KernelAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let order = layout.size.next_power_of_two().trailing_zeros() as u32;
        let mut buddy = BUDDY.lock();
        match buddy.alloc(order) {
            Some(frame) => frame.0 as *mut u8,
            None => null_mut(),
        }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let order = layout.size.next_power_of_two().trailing_zeros() as u32;
        let mut buddy = BUDDY.lock();
        buddy.free(crate::memory::PageFrame(ptr as u64), order);
    }
}

static BUDDY: Mutex<BuddyAllocator> = Mutex::new(BuddyAllocator::new());

#[global_allocator]
static ALLOC: KernelAlloc = KernelAlloc;
```

### Milestone M2 — Custom target spec + linker script

**Status:** ❌ Not started

**Tasks:**
- [ ] Write `targets/x86_64-unknown-zeroxos.json`
- [ ] Write `targets/aarch64-unknown-zeroxos.json`
- [ ] Write `linker/x86_64.ld` and `linker/aarch64.ld`
- [ ] Add `.cargo/config.toml` with default target
- [ ] Create `kernel_entry` crate with `#[no_mangle] pub extern "C" fn _start()`

**Stub — `targets/x86_64-unknown-zeroxos.json`:**

```json
{
  "llvm-target": "x86_64-unknown-none",
  "data-layout": "e-m:e-p270:32:32-p271:32:32-p272:64:64-i64:64-i128:128-f80:128-n8:16:32:64-S128",
  "arch": "x86_64",
  "target-endian": "little",
  "target-pointer-width": "64",
  "target-c-int-width": "32",
  "os": "none",
  "executables": true,
  "linker-flavor": "ld.lld",
  "panic-strategy": "abort",
  "disable-redzone": true,
  "features": "-mmx,-sse,+soft-float",
  "code-model": "kernel",
  "relocation-model": "static",
  "pre-link-args": {
    "ld.lld": ["-Tlinker/x86_64.ld", "-nostdlib"]
  }
}
```

**Stub — `linker/x86_64.ld`:**

```ld
OUTPUT_FORMAT(elf64-x86-64)
OUTPUT_ARCH(i386:x86-64)
ENTRY(_start)

PHDRS
{
    text    PT_LOAD    FLAGS((1 << 0) | (1 << 2)) /* +X */;
    rodata  PT_LOAD    FLAGS(1 << 2)              /*   */;
    data    PT_LOAD    FLAGS((1 << 1) | (1 << 2)) /* +W */;
}

SECTIONS
{
    . = 0xffffffff80200000;
    .text           : { *(.text .text.*) }     :text
    .rodata         : { *(.rodata .rodata.*) } :rodata
    .data           : { *(.data .data.*) }     :data
    .bss            : { *(.bss .bss.*) }       :data
    /DISCARD/       : { *(.eh_frame) *(.note.*) }
}
```

### Milestone M3 — GDT, IDT, TSS (x86_64)

**Status:** ❌ Not started

**Tasks:**
- [ ] Implement GDT with kernel code/data segments + user code/data segments + TSS
- [ ] Implement IDT with 256 entries, each pointing to an ISR stub
- [ ] Generate ISR stubs with a macro that pushes regs and calls a dispatcher
- [ ] Implement TSS with RSP0 for syscall/interrupt kernel stack
- [ ] Load GDT with `lgdt`, IDT with `lidt`, set segment registers

**Stub — IDT entry + ISR stub macro:**

```rust
// crates/zerox-kernel/src/arch/x86_64/idt.rs
use core::mem::size_of;

#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
pub struct IdtEntry {
    pub offset_low: u16,
    pub selector: u16,
    pub ist: u8,
    pub attr: u8,
    pub offset_mid: u16,
    pub offset_high: u32,
    pub zero: u32,
}

#[repr(C, packed)]
pub struct IdtPointer {
    pub limit: u16,
    pub base: *const IdtEntry,
}

static mut IDT: [IdtEntry; 256] = [IdtEntry::NULL; 256];

impl IdtEntry {
    pub const fn NULL() -> Self {
        Self { offset_low: 0, selector: 0, ist: 0, attr: 0, offset_mid: 0, offset_high: 0, zero: 0 }
    }
    pub fn set_handler(&mut self, handler: unsafe extern "C" fn() -> (), selector: u16, ist: u8) {
        let addr = handler as usize;
        self.offset_low = (addr & 0xFFFF) as u16;
        self.offset_mid = ((addr >> 16) & 0xFFFF) as u16;
        self.offset_high = ((addr >> 32) & 0xFFFFFFFF) as u32;
        self.selector = selector;
        self.ist = ist;
        self.attr = 0x8E; // present | DPL=0 | interrupt gate
        self.zero = 0;
    }
}

/// Macro to generate an ISR stub that pushes a fake error code if the CPU didn't,
/// saves all registers, and calls the Rust dispatcher.
#[macro_export]
macro_rules! isr_stub {
    ($n:expr) => {
        #[naked]
        unsafe extern "C" fn isr_stub_$n() {
            unsafe {
                core::arch::asm!(
                    "push 0",                         // fake error code
                    concat!("push rax"),               // save regs
                    "push rbx", "push rcx", "push rdx", "push rsi", "push rdi",
                    "push r8",  "push r9",  "push r10", "push r11",
                    "push r12", "push r13", "push r14", "push r15",
                    "mov rdi, rsp",                    // pass regs ptr to handler
                    concat!("call isr_handler_", $n),
                    "pop r15", "pop r14", "pop r13", "pop r12",
                    "pop r11", "pop r10", "pop r9",  "pop r8",
                    "pop rdi", "pop rsi", "pop rdx", "pop rcx", "pop rbx", "pop rax",
                    "add rsp, 8",                      // drop error code
                    "iretq",
                    options(noreturn)
                );
            }
        }
    };
}
```

### Milestone M4 — Real page tables

**Status:** ❌ Not started — `hal::arch::x86_64::map_page` calls `unimplemented!()`

**Tasks:**
- [ ] Implement 4-level page table walk (PML4 → PDPT → PD → PT)
- [ ] Implement `map_page(virt, phys, flags)` that allocates intermediate tables
- [ ] Implement `unmap_page(virt)` that clears PTE and frees empty tables
- [ ] Implement TLB shootdown via IPI
- [ ] Implement huge page mapping (2 MB at PD level, 1 GB at PDPT level)
- [ ] Implement page fault handler with demand paging + CoW

**Stub — page table walk:**

```rust
// crates/hal/src/arch/x86_64/pte.rs
use crate::memory::{PageFlags, PhysAddr, VirtAddr, PAGE_SIZE};

const PTE_PRESENT: u64 = 1 << 0;
const PTE_WRITABLE: u64 = 1 << 1;
const PTE_USER: u64 = 1 << 2;
const PTE_LARGE: u64 = 1 << 7;
const PTE_ADDR_MASK: u64 = 0x000FFFFFFFFFF000;

#[repr(transparent)]
pub struct PageTable([Pte; 512]);

#[repr(transparent)]
pub struct Pte(pub u64);

impl Pte {
    pub fn flags(&self) -> PageFlags {
        let mut f = PageFlags::default();
        if self.0 & PTE_WRITABLE != 0 { f = f.or(PageFlags::WRITE); }
        if self.0 & PTE_USER != 0     { f = f.or(PageFlags::USER); }
        // ... etc
        f
    }
    pub fn addr(&self) -> PhysAddr { PhysAddr(self.0 & PTE_ADDR_MASK); }
    pub fn is_present(&self) -> bool { self.0 & PTE_PRESENT != 0 }
    pub fn is_large(&self) -> bool { self.0 & PTE_LARGE != 0 }
}

/// virt = [PML4 idx:9][PDPT idx:9][PD idx:9][PT idx:9][offset:12]
pub fn virt_to_indices(virt: VirtAddr) -> (usize, usize, usize, usize) {
    let v = virt.0 as usize;
    (
        (v >> 39) & 0x1FF,  // PML4
        (v >> 30) & 0x1FF,  // PDPT
        (v >> 21) & 0x1FF,  // PD
        (v >> 12) & 0x1FF,  // PT
    )
}

/// Walk the page table, allocating intermediate levels as needed.
///
/// # Safety
/// Caller must ensure `cr3` points to a valid PML4.
pub unsafe fn map_page(pml4: &mut PageTable, virt: VirtAddr, phys: PhysAddr, flags: PageFlags) {
    let (pml4_i, pdpt_i, pd_i, pt_i) = virt_to_indices(virt);
    let pdpt = pml4.get_or_alloc_child(pml4_i);
    let pd   = pdpt.get_or_alloc_child(pdpt_i);
    let pt   = pd.get_or_alloc_child(pd_i);
    let pte = &mut pt.0[pt_i];
    pte.0 = phys.0 | pte_flags_from(flags) | PTE_PRESENT;
    unsafe { core::arch::asm!("invlpg [{}]", in(reg) virt.0 as usize, options(nostack, preserves_flags)); }
}
```

### Milestone M5 — Context switch

**Status:** ❌ Not started

**Tasks:**
- [ ] Implement `TaskContext` struct (saved callee-saved regs + RIP + RSP + CR3)
- [ ] Implement `switch_to(next: &TaskContext)` in naked asm
- [ ] Save/restore FPU state (SSE/AVX) lazily via `cr0.TS`
- [ ] Per-CPU current-task pointer
- [ ] Switch `cr3` (TTBR0_EL1 on ARM) on address space change

**Stub — context switch on x86_64:**

```rust
// crates/zerox-kernel/src/arch/x86_64/context.rs
use core::ptr::NonNull;

#[repr(C)]
pub struct TaskContext {
    pub r15: u64, pub r14: u64, pub r13: u64, pub r12: u64,
    pub rbx: u64, pub rbp: u64,
    pub rip: u64,
    pub rsp: u64,
    pub cr3: u64,           // page table root
    pub fxsave_area: [u8; 512], // FPU state
}

impl TaskContext {
    pub const fn empty() -> Self {
        Self {
            r15: 0, r14: 0, r13: 0, r12: 0, rbx: 0, rbp: 0,
            rip: 0, rsp: 0, cr3: 0, fxsave_area: [0; 512],
        }
    }
}

/// Switch from `prev` to `next`. Saves prev's state, loads next's, swaps cr3.
///
/// # Safety
/// Must be called from a kernel context with interrupts disabled.
#[naked]
pub unsafe extern "C" fn switch_to(prev: *mut TaskContext, next: *const TaskContext) {
    unsafe {
        core::arch::asm!(
            // Save callee-saved regs to prev
            "mov [rdi + 0x00], r15",
            "mov [rdi + 0x08], r14",
            "mov [rdi + 0x10], r13",
            "mov [rdi + 0x18], r12",
            "mov [rdi + 0x20], rbx",
            "mov [rdi + 0x28], rbp",
            "mov [rdi + 0x30], rip",     // populated from return addr
            // Load callee-saved regs from next
            "mov r15, [rsi + 0x00]",
            "mov r14, [rsi + 0x08]",
            "mov r13, [rsi + 0x10]",
            "mov r12, [rsi + 0x18]",
            "mov rbx, [rsi + 0x20]",
            "mov rbp, [rsi + 0x28]",
            // Swap cr3 if different
            "mov rax, [rsi + 0x38]",     // next.cr3
            "mov rcx, cr3",
            "cmp rax, rcx",
            "je 1f",
            "mov cr3, rax",
            "1:",
            // Push next.rip and ret to it
            "push qword ptr [rsi + 0x30]",
            "ret",
            options(noreturn)
        );
    }
}
```

### Milestone M6 — Syscall entry/exit

**Status:** ❌ Not started

**Tasks:**
- [ ] Enable `syscall`/`sysret` via EFER.SCE
- [ ] Set STAR MSR with kernel CS/SS and user CS/SS
- [ ] Set LSTAR MSR to point to `syscall_entry`
- [ ] Set FMASK MSR to clear IF on syscall
- [ ] Implement `syscall_entry` that swaps gs, switches to kernel stack, saves user state
- [ ] Implement syscall table dispatch
- [ ] Implement `sysret` to return

**Stub — syscall entry:**

```rust
// crates/zerox-kernel/src/arch/x86_64/syscall.rs
use crate::syscall::Syscall;

/// MSR addresses
const MSR_EFER: u32 = 0xC0000080;
const MSR_STAR: u32 = 0xC0000081;
const MSR_LSTAR: u32 = 0xC0000082;
const MSR_FMASK: u32 = 0xC0000084;

pub unsafe fn init_syscalls() {
    // Enable syscall/sysret
    let mut efer: u64;
    core::arch::asm!("rdmsr", in("ecx") MSR_EFER, out("eax") _, out("edx") _);
    // ... actually read/modify/write EFER.SCE bit
    // Set STAR: kernel CS=0x08, SS=0x10, user CS=0x1B, SS=0x23
    wrmsr(MSR_STAR, (0x1Bu64 << 48) | (0x08u64 << 32));
    wrmsr(MSR_LSTAR, syscall_entry as usize as u64);
    wrmsr(MSR_FMASK, 0x200);  // clear IF on entry
}

/// Syscall entry. RAX = syscall number, RDI-R9 = args.
#[naked]
unsafe extern "sysv64" fn syscall_entry() {
    core::arch::asm!(
        // Swap GS to kernel GS base
        "swapgs",
        // Save user stack pointer and switch to kernel stack
        "mov gs:[0], rsp",
        "mov rsp, gs:[8]",
        // Save user registers
        "push rcx",      // user RIP
        "push r11",      // user RFLAGS
        "push rdi", "push rsi", "push rdx", "push r10", "push r8", "push r9",
        // Call the dispatcher (rax = syscall number, rdi-r9 = args)
        "call syscall_dispatch",
        // Restore user registers
        "pop r9", "pop r8", "pop r10", "pop rdx", "pop rsi", "pop rdi",
        "pop r11",       // user RFLAGS
        "pop rcx",       // user RIP
        // Restore user stack
        "mov rsp, gs:[0]",
        "swapgs",
        "sysretq",
        options(noreturn)
    );
}

#[no_mangle]
extern "sysv64" fn syscall_dispatch(nr: u64, a: u64, b: u64, c: u64, d: u64, e: u64, f: u64) -> u64 {
    match nr {
        0 => sys_exit(a as i32),
        1 => sys_write(a, b as *const u8, c),
        2 => sys_read(a, b as *mut u8, c),
        // ...
        _ => -1i64 as u64,
    }
}
```

### Milestone M7 — First QEMU boot

**Status:** ❌ Not started

**Tasks:**
- [ ] Write `boot/x86_64-boot.S` — multiboot2 header + early init
- [ ] Wire `zerox-sim` demos to run under QEMU via `-kernel`
- [ ] Create `initramfs` with hello-world init
- [ ] Add `make qemu-x86_64` and `make qemu-aarch64` targets
- [ ] Verify console output via serial port (`0x3F8`)

**Expected QEMU command:**

```bash
qemu-system-x86_64 \
    -kernel target/x86_64-unknown-zeroxos/release/zeroxos.bin \
    -initrd initramfs.cpio \
    -m 256M \
    -serial stdio \
    -no-reboot
```

---

## 3. Memory management — detailed gaps

### 3.1 What works today

- ✅ Buddy allocator with coalescing (10 orders, 4 KB → 2 MB)
- ✅ Slab allocator struct (constant-time alloc/free when cache is hot)
- ✅ Huge page counter
- ✅ CoW fault counter (but not wired to a real fault handler)
- ✅ Memory stats (total/free/allocated/cow/huge)

### 3.2 What's missing

| Component | Status | Notes |
|-----------|--------|-------|
| Real page tables (PML4 walk on x86, 3-level on ARM) | ❌ | `map_page` is `unimplemented!()` |
| Per-process address space (`struct AddressSpace`) | ❌ | No VMA, no mmap |
| Page fault handler | ❌ | Demand paging, CoW, swap-in all missing |
| TLB shootdown IPIs | ❌ | Multi-core updates will corrupt |
| Kernel heap allocator (`#[global_allocator]`) | ❌ | Currently uses `std::alloc` |
| User/kernel address space split | ❌ | No 47/13 or 48/16 split |
| IOMMU/SMMU driver | ❌ | DMA from untrusted devices can write kernel |
| NUMA per-node allocators | ❌ | Field exists, no implementation |
| Page migration | ❌ | |
| Swap / backing store | ❌ | |
| KASLR | ❌ | |
| Memory hotplug | ❌ | |
| Memory cgroups | ❌ | |
| `mmap` / `munmap` / `mprotect` / `madvise` | ❌ | |
| `mremap` | ❌ | |
| `msync` | ❌ | |
| Userfaultfd | ❌ | |
| Huge page collapse / split | ❌ | |
| Memory compaction | ❌ | |
| OOM killer | ❌ | |

### 3.3 Stub — per-process address space

```rust
// crates/zerox-kernel/src/memory/vas.rs
use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use spin::Mutex;

#[derive(Debug, Clone, Copy)]
pub struct Vma {
    pub start: u64,
    pub end: u64,
    pub flags: VmaFlags,
    pub backing: VmaBacking,
}

#[derive(Debug, Clone, Copy)]
pub struct VmaFlags(pub u32);
impl VmaFlags {
    pub const READ: Self = Self(1 << 0);
    pub const WRITE: Self = Self(1 << 1);
    pub const EXEC: Self = Self(1 << 2);
    pub const USER: Self = Self(1 << 3);
    pub const STACK: Self = Self(1 << 4);
    pub const HEAP: Self = Self(1 << 5);
}

#[derive(Debug, Clone, Copy)]
pub enum VmaBacking {
    Anonymous,
    File { inode: u64, offset: u64 },
    Shared { shmid: u64 },
}

pub struct AddressSpace {
    pub pml4_phys: u64,           // cr3 / TTBR0_EL1
    pub vmas: Mutex<BTreeMap<u64, Vma>>,  // keyed by start addr
    pub brk: u64,                 // end of heap
    pub stack_top: u64,
    pub stack_bottom: u64,
}

impl AddressSpace {
    pub fn new() -> Self {
        // Allocate a new PML4, identity-map the kernel region
        // ...
        unimplemented!()
    }

    pub fn mmap(&self, hint: u64, len: u64, flags: VmaFlags, backing: VmaBacking) -> Result<u64, MmapError> {
        // Find a free gap, create a VMA, optionally map pages immediately
        unimplemented!()
    }

    pub fn munmap(&self, addr: u64, len: u64) -> Result<(), MunmapError> {
        unimplemented!()
    }

    pub fn handle_page_fault(&self, addr: u64, write: bool, user: bool) -> Result<(), FaultError> {
        // 1. Find the VMA containing addr
        // 2. If VMA is anonymous → allocate a new page, map it
        // 3. If VMA is file-backed → read the page from the file
        // 4. If VMA is CoW and write → copy the page, remap
        // 5. If no VMA → SIGSEGV
        unimplemented!()
    }
}

#[derive(Debug)]
pub enum MmapError { NoSpace, InvalidFlags }
#[derive(Debug)]
pub enum MunmapError { NotFound }
#[derive(Debug)]
pub enum FaultError { NoMapping, ProtectionViolation }
```

---

## 4. CPU, SMP, and context switching

### 4.1 What works today

- ✅ Scheduler data structures (SchedThread, Policy, Priority)
- ✅ pick_next() that picks RT > MLFQ > CFS
- ✅ Gaming mode priority boosting
- ✅ Frame deadline awareness

### 4.2 What's missing

| Component | Status |
|-----------|--------|
| GDT/IDT setup (x86), VBAR_EL1 (ARM) | ❌ |
| Real context switch routine | ❌ |
| Per-CPU data (`__get_cpu_var`) | ❌ |
| Per-CPU runqueues | ❌ — current scheduler has one global queue |
| Idle task per CPU | ❌ |
| SMP boot (APIC INIT-SIPI-SIPI on x86, PSCI CPU_ON on ARM) | ❌ |
| FPU/SIMD lazy save/restore | ❌ |
| Signal delivery | ❌ |
| CPU hotplug | ❌ |
| CPU idle states (C-states, mwait) | ❌ |
| CPU frequency scaling | ❌ — `set_cpu_frequency` is a stub on host |
| Topology discovery (NUMA, cache hierarchy) | ❌ |

### 4.3 Stub — per-CPU data

```rust
// crates/zerox-kernel/src/percpu.rs
use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicUsize, Ordering};

pub struct PerCpu<T> {
    data: UnsafeCell<Vec<T>>,
    current_cpu: AtomicUsize,
}

unsafe impl<T: Send> Sync for PerCpu<T> {}

impl<T: Default + Clone> PerCpu<T> {
    pub fn new(num_cpus: usize) -> Self {
        Self {
            data: UnsafeCell::new(vec![T::default(); num_cpus]),
            current_cpu: AtomicUsize::new(0),
        }
    }

    /// # Safety
    /// Must be called with preemption disabled.
    pub unsafe fn current(&self) -> &T {
        let cpu = crate::arch::current_cpu_id() as usize;
        &(*self.data.get())[cpu]
    }

    pub unsafe fn current_mut(&self) -> &mut T {
        let cpu = crate::arch::current_cpu_id() as usize;
        &mut (*self.data.get())[cpu]
    }
}

// Usage:
// static CURRENT_TASK: PerCpu<Option<TaskRef>> = PerCpu::new(8);
```

### 4.4 Stub — per-CPU runqueue scheduler

The current scheduler has a single global `Vec<SchedThread>` which becomes a
bottleneck on multi-core. The fix is per-CPU runqueues with work stealing.

```rust
// crates/zerox-kernel/src/scheduler/percpu.rs
use crate::percpu::PerCpu;
use crate::scheduler::{SchedThread, ThreadId};
use alloc::collections::VecDeque;
use spin::Mutex;

pub struct PerCpuRunqueue {
    pub queue: Mutex<VecDeque<ThreadId>>,
    pub cpu: u32,
}

pub struct SchedulerV2 {
    pub runqueues: PerCpu<PerCpuRunqueue>,
    pub threads: Mutex<alloc::collections::BTreeMap<ThreadId, SchedThread>>,
}

impl SchedulerV2 {
    pub fn pick_next(&self) -> Option<ThreadId> {
        let rq = unsafe { self.runqueues.current() };
        let mut q = rq.queue.lock();
        q.pop_front()
    }

    pub fn enqueue(&self, tid: ThreadId) {
        // Try to enqueue on the current CPU's queue
        let rq = unsafe { self.runqueues.current() };
        rq.queue.lock().push_back(tid);
    }

    /// Steal work from another CPU's runqueue when ours is empty.
    pub fn steal(&self) -> Option<ThreadId> {
        let my_cpu = unsafe { crate::arch::current_cpu_id() };
        for cpu in 0..8 {
            if cpu == my_cpu { continue; }
            // ... lock their runqueue, pop from front
        }
        None
    }
}
```

---

## 5. Interrupts and system calls

### 5.1 What works today

- ✅ IRQ registration data structure
- ✅ Fast IPC channels that could carry IRQ notifications

### 5.2 What's missing

| Component | Status |
|-----------|--------|
| Local APIC init (x86) | ❌ |
| IO-APIC routing (x86) | ❌ |
| GICv3 distributor + redistributor init (ARM) | ❌ |
| MSI/MSI-X programming | ❌ |
| IRQ affinity management | ❌ |
| IRQ storm detection | ❌ |
| Softirqs / tasklets / workqueues | ❌ |
| Threaded IRQs | ❌ |
| `request_irq` / `free_irq` API | ❌ — current `register_irq` is a no-op |
| Syscall entry/exit | ❌ |
| Syscall table | ❌ |
| `swapgs` (x86) | ❌ |
| Per-thread kernel stack | ❌ |

### 5.3 Stub — top-half / bottom-half IRQ framework

```rust
// crates/zerox-kernel/src/irq.rs
use alloc::boxed::Box;
use alloc::sync::Arc;
use spin::Mutex;

pub type IrqHandler = Box<dyn Fn(&IrqContext) + Send + Sync>;

pub struct IrqContext {
    pub irq: u32,
    pub regs: *const crate::arch::Registers,
}

pub struct IrqDescriptor {
    pub handler: Option<IrqHandler>,
    pub threaded: Option<Arc<Mutex<ThreadedIrq>>>,
    pub flags: IrqFlags,
}

pub struct ThreadedIrq {
    pub handler: Box<dyn Fn() + Send + Sync>,
    pub woken: bool,
}

pub struct IrqFlags(pub u32);
impl IrqFlags {
    pub const SHARED: Self = Self(1 << 0);
    pub const THREADED: Self = Self(1 << 1);
    pub const NOAUTOEN: Self = Self(1 << 2);
}

pub static IRQ_TABLE: Mutex<[Option<IrqDescriptor>; 256]> = Mutex::new([const { None }; 256]);

pub fn request_irq(irq: u32, handler: IrqHandler, flags: IrqFlags) -> Result<(), IrqError> {
    let mut table = IRQ_TABLE.lock();
    let slot = &mut table[irq as usize];
    if slot.is_some() && !flags.0 & IrqFlags::SHARED.0 != 0 {
        return Err(IrqError::AlreadyInUse);
    }
    *slot = Some(IrqDescriptor { handler: Some(handler), threaded: None, flags });
    // ... enable at interrupt controller
    Ok(())
}

pub fn handle_irq(irq: u32, ctx: &IrqContext) {
    let table = IRQ_TABLE.lock();
    if let Some(desc) = &table[irq as usize] {
        if let Some(h) = &desc.handler {
            h(ctx);
        }
        if let Some(threaded) = &desc.threaded {
            threaded.lock().woken = true;
            // wake up the IRQ thread
        }
    }
    // ack at interrupt controller
}

#[derive(Debug)]
pub enum IrqError { AlreadyInUse, Invalid }
```

---

## 6. Process model and ELF loader

### 6.1 What works today

- ✅ Process struct (pid, name, state, threads, caps)
- ✅ Thread struct
- ✅ Spawn / kill / exit

### 6.2 What's missing

| Component | Status |
|-----------|--------|
| `exec()` — load and run a new binary | ❌ |
| `fork()` with CoW | ❌ |
| `clone()` with `CLONE_VM`, `CLONE_FILES`, etc. | ❌ |
| `wait()` / `waitpid()` with zombie reaping | ❌ |
| Process groups, sessions, job control | ❌ |
| `ptrace()` | ❌ |
| Core dump generation | ❌ |
| ELF loader (PT_LOAD, dynamic symbols) | ❌ |
| TLS setup | ❌ |
| Futex | ❌ — no real userspace mutexes possible |
| `prctl` / `arch_prctl` | ❌ |
| `setuid` / `setgid` / `setgroups` (with capabilities) | ❌ |
| Resource limits (rlimits) | ❌ |
| `umask` | ❌ |

### 6.3 Stub — ELF loader

```rust
// crates/zerox-kernel/src/process/elf.rs
use crate::memory::AddressSpace;
use crate::vfs::File;

#[derive(Debug)]
pub enum ElfError {
    BadMagic,
    UnsupportedClass,
    UnsupportedEndian,
    NoInterp,
    IoError,
}

pub struct ElfImage {
    pub entry: u64,
    pub interp: Option<alloc::string::String>,  // dynamic linker path
    pub phdrs: alloc::vec::Vec<Phdr>,
}

pub fn load_elf(file: &File, aspace: &AddressSpace) -> Result<ElfImage, ElfError> {
    // 1. Read ELF header (first 64 bytes)
    let mut hdr_buf = [0u8; 64];
    file.read_at(0, &mut hdr_buf)?;

    // 2. Verify magic: 0x7F 'E' 'L' 'F'
    if &hdr_buf[0..4] != b"\x7fELF" { return Err(ElfError::BadMagic); }

    // 3. Parse e_entry, e_phoff, e_phnum, e_phentsize
    let entry = u64::from_le_bytes(hdr_buf[24..32].try_into().unwrap());
    let phoff = u64::from_le_bytes(hdr_buf[32..40].try_into().unwrap());
    let phentsize = u16::from_le_bytes(hdr_buf[54..56].try_into().unwrap());
    let phnum = u16::from_le_bytes(hdr_buf[56..58].try_into().unwrap());

    // 4. Read program headers
    let mut phdrs = alloc::vec::Vec::new();
    let mut interp = None;
    for i in 0..phnum {
        let off = phoff + (i as u64) * (phentsize as u64);
        let mut phdr_buf = [0u8; 56];
        file.read_at(off, &mut phdr_buf)?;
        let p_type = u32::from_le_bytes(phdr_buf[0..4].try_into().unwrap());
        let p_flags = u32::from_le_bytes(phdr_buf[4..8].try_into().unwrap());
        let p_offset = u64::from_le_bytes(phdr_buf[8..16].try_into().unwrap());
        let p_vaddr = u64::from_le_bytes(phdr_buf[16..24].try_into().unwrap());
        let p_filesz = u64::from_le_bytes(phdr_buf[32..40].try_into().unwrap());
        let p_memsz = u64::from_le_bytes(phdr_buf[40..48].try_into().unwrap());

        match p_type {
            1 /* PT_LOAD */ => {
                // Map [p_vaddr, p_vaddr + p_memsz) into the address space
                let mut flags = crate::memory::VmaFlags::USER;
                if p_flags & 4 != 0 { flags = flags.or(crate::memory::VmaFlags::READ); }
                if p_flags & 2 != 0 { flags = flags.or(crate::memory::VmaFlags::WRITE); }
                if p_flags & 1 != 0 { flags = flags.or(crate::memory::VmaFlags::EXEC); }
                aspace.mmap(p_vaddr, p_memsz, flags, crate::memory::VmaBacking::File {
                    inode: file.inode(),
                    offset: p_offset,
                })?;
                // Read file contents into memory
                let mut buf = alloc::vec![0u8; p_filesz as usize];
                file.read_at(p_offset, &mut buf)?;
                // ... copy buf into the mapped pages
            }
            3 /* PT_INTERP */ => {
                let mut s = alloc::vec![0u8; p_filesz as usize];
                file.read_at(p_offset, &mut s)?;
                interp = Some(alloc::string::String::from_utf8_lossy(&s).into_owned());
            }
            _ => {}
        }
        phdrs.push(Phdr { ty: p_type, flags: p_flags, offset: p_offset, vaddr: p_vaddr, filesz: p_filesz, memsz: p_memsz });
    }

    Ok(ElfImage { entry, interp, phdrs })
}

#[derive(Debug, Clone, Copy)]
pub struct Phdr { pub ty: u32, pub flags: u32, pub offset: u64, pub vaddr: u64, pub filesz: u64, pub memsz: u64 }
```

### 6.4 Stub — futex

Without futex, userspace mutexes/condvars cannot work.

```rust
// crates/zerox-kernel/src/sync/futex.rs
use alloc::collections::BTreeMap;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::Mutex;

/// Per-address wait queue.
static FUTEX_WAITERS: Mutex<BTreeMap<u64, alloc::collections::VecDeque<u64>>> = Mutex::new(BTreeMap::new());

/// Futex wait — block if *uaddr == expected.
pub fn wait(uaddr: u64, expected: u32) -> Result<(), FutexError> {
    let val = unsafe { (uaddr as *const AtomicU32).as_ref().unwrap() }.load(Ordering::Acquire);
    if val != expected { return Err(FutexError::ValueChanged); }
    let tid = crate::process::current_tid();
    let mut map = FUTEX_WAITERS.lock();
    map.entry(uaddr).or_default().push_back(tid);
    drop(map);
    // block the current thread
    crate::scheduler::block_current();
    Ok(())
}

/// Futex wake — wake at most `n` waiters on `uaddr`.
pub fn wake(uaddr: u64, n: usize) -> Result<usize, FutexError> {
    let mut map = FUTEX_WAITERS.lock();
    if let Some(queue) = map.get_mut(&uaddr) {
        let mut woken = 0;
        for _ in 0..n {
            if let Some(tid) = queue.pop_front() {
                crate::scheduler::unblock(tid);
                woken += 1;
            } else { break; }
        }
        Ok(woken)
    } else {
        Ok(0)
    }
}

#[derive(Debug)]
pub enum FutexError { ValueChanged, InvalidAddr }
```

---

## 7. VFS and file descriptors

### 7.1 What works today

- ✅ zeroxfs CoW tree, journal, checksums (in-memory only)
- ✅ Package service data structure

### 7.2 What's missing

| Component | Status |
|-----------|--------|
| VFS layer (inode_operations, file_operations) | ❌ |
| Dentry cache | ❌ |
| Inode cache | ❌ |
| Page cache | ❌ |
| Buffer cache | ❌ |
| Per-process fd table | ❌ |
| `open` / `read` / `write` / `close` / `lseek` / `stat` / `unlink` / `mkdir` / `readdir` | ❌ |
| `mount()` | ❌ |
| Mount tree | ❌ |
| Bind mounts | ❌ |
| `/dev` (devtmpfs) | ❌ |
| `/proc` (procfs) | ❌ |
| `/sys` (sysfs) | ❌ |
| `/tmp` (tmpfs) | ❌ |
| `pipe()` | ❌ |
| `socket()` | ❌ |
| `eventfd()` | ❌ |
| `timerfd()` | ❌ |
| `signalfd()` | ❌ |
| `epoll` / `kqueue` | ❌ |
| `mmap()` for files | ❌ |
| `ioctl()` | ❌ |
| `inotify` | ❌ |
| Block device layer | ❌ |
| I/O scheduler | ❌ |
| Partition table parser (GPT, MBR) | ❌ |
| Real on-disk zeroxfs format | ❌ — current structs are in-memory |

### 7.3 Stub — VFS

```rust
// crates/zerox-kernel/src/vfs/mod.rs
use alloc::sync::Arc;
use alloc::vec::Vec;
use spin::Mutex;

pub struct Inode {
    pub ino: u64,
    pub mode: u32,
    pub size: u64,
    pub ops: Arc<dyn InodeOps>,
}

pub trait InodeOps: Send + Sync {
    fn lookup(&self, name: &str) -> Result<Arc<Inode>, VfsError>;
    fn create(&self, name: &str, mode: u32) -> Result<Arc<Inode>, VfsError>;
    fn mkdir(&self, name: &str, mode: u32) -> Result<Arc<Inode>, VfsError>;
    fn unlink(&self, name: &str) -> Result<(), VfsError>;
    fn readdir(&self, offset: u64) -> Result<Vec<DirEntry>, VfsError>;
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<usize, VfsError>;
    fn write_at(&self, offset: u64, buf: &[u8]) -> Result<usize, VfsError>;
    fn stat(&self) -> Result<Stat, VfsError>;
}

pub struct File {
    pub inode: Arc<Inode>,
    pub pos: Mutex<u64>,
    pub flags: u32,
}

impl File {
    pub fn read(&self, buf: &mut [u8]) -> Result<usize, VfsError> {
        let mut pos = self.pos.lock();
        let n = self.inode.ops.read_at(*pos, buf)?;
        *pos += n as u64;
        Ok(n)
    }
    pub fn write(&self, buf: &[u8]) -> Result<usize, VfsError> {
        let mut pos = self.pos.lock();
        let n = self.inode.ops.write_at(*pos, buf)?;
        *pos += n as u64;
        Ok(n)
    }
}

pub struct DirEntry {
    pub ino: u64,
    pub name: alloc::string::String,
    pub ty: FileType,
}

#[derive(Debug, Clone, Copy)]
pub enum FileType { File, Dir, Symlink, Block, Char, Fifo, Socket }

#[derive(Debug, Clone, Copy)]
pub struct Stat {
    pub ino: u64, pub mode: u32, pub nlink: u32, pub size: u64,
    pub blksize: u32, pub blocks: u64,
}

#[derive(Debug)]
pub enum VfsError { NotFound, NotADirectory, NotAFile, Exists, ReadOnly, IoError }

/// Per-process fd table.
pub struct FdTable {
    pub fds: Mutex<alloc::collections::BTreeMap<u32, Arc<File>>>,
    pub next_fd: Mutex<u32>,
}

impl FdTable {
    pub fn open(&self, file: Arc<File>) -> u32 {
        let mut next = self.next_fd.lock();
        let fd = *next;
        *next += 1;
        self.fds.lock().insert(fd, file);
        fd
    }
    pub fn close(&self, fd: u32) -> Result<(), VfsError> {
        self.fds.lock().remove(&fd).ok_or(VfsError::NotFound).map(|_| ())
    }
    pub fn get(&self, fd: u32) -> Result<Arc<File>, VfsError> {
        self.fds.lock().get(&fd).cloned().ok_or(VfsError::NotFound)
    }
}
```

---

## 8. Time subsystem

### 8.1 What works today

- ✅ `read_time_ns()` on host
- ✅ `read_cycle_counter()` on x86 (RDTSC) and ARM (CNTVCT_EL0)
- ✅ Scheduler tick counter

### 8.2 What's missing

| Component | Status |
|-----------|--------|
| Clocksource framework (picks best source at runtime) | ❌ |
| Clockevents (per-CPU `set_next_event`) | ❌ |
| hrtimer (high-resolution timer) subsystem | ❌ |
| POSIX timers (`timer_create`, `timer_settime`) | ❌ |
| `timerfd` | ❌ |
| `clock_nanosleep` | ❌ |
| NTP discipline (`adjtimex`) | ❌ |
| RTC sync at boot | ❌ |
| Watchdog driver | ❌ |
| PTP (Precision Time Protocol) | ❌ |

### 8.3 Stub — clocksource framework

```rust
// crates/zerox-kernel/src/time/clocksource.rs
use alloc::string::String;
use alloc::sync::Arc;
use spin::RwLock;

pub trait Clocksource: Send + Sync {
    fn name(&self) -> &str;
    fn read(&self) -> u64;            // current counter value
    fn freq_hz(&self) -> u64;          // ticks per second
    fn rating(&self) -> u32;           // higher = better
}

pub static CURRENT_CLOCKSOURCE: RwLock<Option<Arc<dyn Clocksource>>> = RwLock::new(None);

pub fn register_clocksource(cs: Arc<dyn Clocksource>) {
    let mut cur = CURRENT_CLOCKSOURCE.write();
    let should_replace = cur.as_ref().map_or(true, |old| cs.rating() > old.rating());
    if should_replace {
        *cur = Some(cs);
    }
}

pub fn read_time_ns() -> u64 {
    if let Some(cs) = CURRENT_CLOCKSOURCE.read().as_ref() {
        let ticks = cs.read();
        let freq = cs.freq_hz();
        ticks.saturating_mul(1_000_000_000) / freq.max(1)
    } else {
        0
    }
}

// TSC clocksource on x86
pub struct TscClocksource;
impl Clocksource for TscClocksource {
    fn name(&self) -> &str { "tsc" }
    fn read(&self) -> u64 { hal::cpu::read_cycle_counter() }
    fn freq_hz(&self) -> u64 { 3_000_000_000 } // calibrated at boot
    fn rating(&self) -> u32 { 300 }
}
```

---

## 9. SMP and concurrency primitives

### 9.1 What works today

- ✅ `spin::Mutex` used throughout
- ✅ Atomic counters for stats

### 9.2 What's missing

| Component | Status |
|-----------|--------|
| Per-CPU runqueues | ❌ |
| RCU (Read-Copy-Update) | ❌ |
| `stop_machine()` | ❌ |
| MCS lock / ticket spinlock | ❌ |
| Lockdep | ❌ |
| Workqueues / tasklets / softirqs | ❌ |
| Seqlock | ❌ |
| Read-copy-update | ❌ |
| Per-CPU reference counting | ❌ |
| Memory barriers with proper documentation | ⚠️ inline asm uses `preserves_flags` but no formal memory model |

### 9.3 Stub — ticket spinlock

```rust
// crates/zerox-kernel/src/sync/ticket.rs
use core::sync::atomic::{AtomicU16, Ordering};
use core::cell::UnsafeCell;
use core::ops::{Deref, DerefMut};

pub struct TicketLock<T> {
    next_ticket: AtomicU16,
    now_serving: AtomicU16,
    data: UnsafeCell<T>,
}

unsafe impl<T: Send> Sync for TicketLock<T> {}
unsafe impl<T: Send> Send for TicketLock<T> {}

pub struct TicketGuard<'a, T> {
    lock: &'a TicketLock<T>,
    ticket: u16,
}

impl<T> TicketLock<T> {
    pub const fn new(t: T) -> Self {
        Self {
            next_ticket: AtomicU16::new(0),
            now_serving: AtomicU16::new(0),
            data: UnsafeCell::new(t),
        }
    }
    pub fn lock(&self) -> TicketGuard<'_, T> {
        let ticket = self.next_ticket.fetch_add(1, Ordering::Relaxed);
        while self.now_serving.load(Ordering::Acquire) != ticket {
            core::hint::spin_loop();
        }
        TicketGuard { lock: self, ticket }
    }
}

impl<'a, T> Deref for TicketGuard<'a, T> {
    type Target = T;
    fn deref(&self) -> &T { unsafe { &*self.lock.data.get() } }
}

impl<'a, T> DerefMut for TicketGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut T { unsafe { &mut *self.lock.data.get() } }
}

impl<'a, T> Drop for TicketGuard<'a, T> {
    fn drop(&mut self) {
        self.lock.now_serving.fetch_add(1, Ordering::Release);
    }
}
```

---

## 10. Security hardening

### 10.1 What works today

- ✅ Capability data structure (Camera, Filesystem, Network, Bluetooth, GPU, Audio, etc.)
- ✅ Grant / revoke / verify

### 10.2 What's missing

| Component | Status |
|-----------|--------|
| Secure boot implementation (signature verification) | ❌ |
| TPM driver + measured boot | ❌ |
| Keyring / key management | ❌ |
| Full disk encryption (dm-crypt equivalent) | ❌ |
| MAC framework (SELinux/AppArmor equivalent) | ❌ |
| seccomp | ❌ |
| Audit framework | ❌ |
| Namespaces / cgroups (containers) | ❌ |
| KASLR implementation | ❌ |
| Stack protector canary in kernel | ❌ |
| KCFI (control-flow integrity) | ❌ |
| Pointer authentication (ARM PAC) wiring | ❌ |
| NX / W^X enforcement | ❌ |
| ASLR for userspace | ❌ |
| Sandboxing (seccomp-bpf, namespaces) | ❌ |
| Policy enforcement (audit2allow) | ❌ |

### 10.3 Stub — secure boot

```rust
// crates/zerox-kernel/src/security/secureboot.rs
use alloc::vec::Vec;

pub struct Signature {
    pub alg: SigAlg,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, Copy)]
pub enum SigAlg { RsaPkcs1Sha256, EcdsaP256Sha256, Ed25519 }

pub trait KeyProvider: Send + Sync {
    fn verify(&self, msg: &[u8], sig: &Signature) -> bool;
}

pub struct SecureBoot {
    pub enabled: bool,
    pub key_provider: Option<alloc::sync::Arc<dyn KeyProvider>>,
    pub measured: bool,  // TPM measured boot
}

impl SecureBoot {
    pub fn verify_image(&self, image: &[u8], sig: &Signature) -> Result<(), VerifyError> {
        if !self.enabled { return Ok(()); }
        let kp = self.key_provider.as_ref().ok_or(VerifyError::NoKey)?;
        if kp.verify(image, sig) { Ok(()) } else { Err(VerifyError::BadSignature) }
    }
}

#[derive(Debug)]
pub enum VerifyError { NoKey, BadSignature, UnsupportedAlg }
```

---

## 11. Bus and device discovery

### 11.1 What's missing — everything

| Bus / discovery mechanism | Status |
|---------------------------|--------|
| PCI/PCIe bus enumeration | ❌ |
| Device tree parser (ARM, RISC-V) | ❌ |
| ACPI AML interpreter (x86) | ❌ |
| SMBIOS/DMI parser | ❌ |
| USB device enumeration | ❌ |
| I2C bus framework | ❌ |
| SPI bus framework | ❌ |
| GPIO framework | ❌ |
| Clock framework (PLLs, clock tree) | ❌ |
| Reset controller framework | ❌ |
| Pinctrl/mux framework | ❌ |
| Regulator framework | ❌ |
| PHY framework (USB, PCIe, MIPI) | ❌ |
| DMA engine framework | ❌ |
| NVMEM framework | ❌ |

### 11.2 Stub — PCI enumeration

```rust
// crates/zerox-kernel/src/bus/pci.rs
use alloc::vec::Vec;

const PCI_CONFIG_ADDRESS: u16 = 0xCF8;
const PCI_CONFIG_DATA: u16 = 0xCFC;

#[derive(Debug, Clone, Copy)]
pub struct PciDevice {
    pub bus: u8,
    pub dev: u8,
    pub func: u8,
    pub vendor_id: u16,
    pub device_id: u16,
    pub class_code: u8,
    pub subclass: u8,
    pub prog_if: u8,
    pub bars: [u32; 6],
    pub interrupt_line: u8,
}

pub unsafe fn config_read32(bus: u8, dev: u8, func: u8, offset: u8) -> u32 {
    let addr = 0x80000000
        | ((bus as u32) << 16)
        | ((dev as u32) << 11)
        | ((func as u32) << 8)
        | ((offset as u32) & 0xFC);
    core::arch::asm!(
        "out dx, eax", "in eax, dx",
        inout("eax") addr => _,
        in("dx") PCI_CONFIG_ADDRESS,
        lateout("eax") ret: u32,
    );
    // Actually: write addr to 0xCF8, read from 0xCFC
    ret
}

pub fn enumerate_bus() -> Vec<PciDevice> {
    let mut devices = Vec::new();
    for bus in 0..256 {
        for dev in 0..32 {
            let vendor = unsafe { config_read16(bus, dev, 0, 0) };
            if vendor == 0xFFFF { continue; }
            // Check multifunction devices
            let header_type = unsafe { config_read8(bus, dev, 0, 14) };
            let max_func = if header_type & 0x80 != 0 { 8 } else { 1 };
            for func in 0..max_func {
                let v = unsafe { config_read16(bus, dev, func, 0) };
                if v == 0xFFFF { continue; }
                devices.push(read_config(bus, dev, func));
            }
        }
    }
    devices
}

pub fn read_config(bus: u8, dev: u8, func: u8) -> PciDevice {
    let vendor_id = unsafe { config_read16(bus, dev, func, 0) };
    let device_id = unsafe { config_read16(bus, dev, func, 2) };
    let class_full = unsafe { config_read32(bus, dev, func, 8) };
    let class_code = ((class_full >> 24) & 0xFF) as u8;
    let subclass = ((class_full >> 16) & 0xFF) as u8;
    let prog_if = ((class_full >> 8) & 0xFF) as u8;
    let mut bars = [0u32; 6];
    for i in 0..6 {
        bars[i] = unsafe { config_read32(bus, dev, func, 16 + (i as u8) * 4) };
    }
    let interrupt_line = unsafe { config_read8(bus, dev, func, 60) };
    PciDevice { bus, dev, func, vendor_id, device_id, class_code, subclass, prog_if, bars, interrupt_line }
}
```

---

## 12. Device drivers — complete inventory

The driver registry currently just stores names. Every real driver is missing.

### 12.1 Storage drivers

| Driver | Class | Status |
|--------|-------|--------|
| NVMe (submission/completion queues, PRP/SGL, multi-queue block) | Block | ❌ |
| AHCI / SATA | Block | ❌ |
| SCSI layer (USB Mass Storage, etc.) | Block | ❌ |
| UFS (mobile) | Block | ❌ |
| eMMC (mobile) | Block | ❌ |
| SD / SDIO | Block | ❌ |
| Virtio-blk (VMs) | Block | ❌ |
| NVMe over Fabrics (NVMe-oF) | Block | ❌ |
| dm-crypt (encryption) | Block | ❌ |
| md (RAID) | Block | ❌ |
| LVM | Block | ❌ |

### 12.2 Network drivers

| Driver | Class | Status |
|--------|-------|--------|
| Virtio-net | Net | ❌ |
| Intel e1000/e1000e/ice/i40e | Net | ❌ |
| Realtek r8169 | Net | ❌ |
| Broadcom tg3 | Net | ❌ |
| Qualcomm ath/mhi (WiFi) | Net | ❌ |
| Mediatek mt7921 (WiFi) | Net | ❌ |
| Intel iwlwifi/ax210 (WiFi) | Net | ❌ |
| Broadcom brcmfmac (WiFi) | Net | ❌ |
| PHY framework (Marvell, Realtek, Microchip) | Net | ❌ |
| MDIO bus | Net | ❌ |
| TCP/IP stack (ARP, IPv4, IPv6, ICMP, IGMP, TCP, UDP, SCTP) | Net | ❌ |
| Routing table / fib / neighbour cache | Net | ❌ |
| Netfilter / iptables equivalent | Net | ❌ |
| Socket API for userspace | Net | ❌ |
| DNS resolver | Net | ❌ |
| DHCP client | Net | ❌ |
| WPA supplicant (WPA2/3, 802.1X) | Net | ❌ |

### 12.3 GPU / Display drivers

| Driver | Class | Status |
|--------|-------|--------|
| DRM (Direct Rendering Manager) framework | GPU | ❌ |
| KMS (Kernel Mode Setting) — connector/encoder/CRTC/plane | GPU | ❌ |
| GEM/TTM buffer management | GPU | ❌ |
| AMD amdgpu | GPU | ❌ |
| NVIDIA driver (open/nouveau) | GPU | ❌ |
| Intel i915 / Xe | GPU | ❌ |
| ARM Mali (Panfrost) | GPU | ❌ |
| Adreno (Freedreno) | GPU | ❌ |
| Display: HDMI / DisplayPort / DSI / eDP — PHY, link training | Display | ❌ |
| EDID parser | Display | ❌ |
| Backlight control | Display | ❌ |
| Cursor planes, page-flip, vblank | Display | ❌ |
| Sync/fence primitives (sync_file, dma_fence) | GPU | ❌ |
| Vulkan ICD loader, Mesa-style userspace | GPU | ❌ |

### 12.4 Audio drivers

| Driver | Class | Status |
|--------|-------|--------|
| ALSA-like framework | Audio | ❌ |
| Intel HDA controller + codec | Audio | ❌ |
| ASoC (ALSA on SoC) for mobile | Audio | ❌ |
| PCM ring buffer, mmap to userspace | Audio | ❌ |
| Mixer, codec routing | Audio | ❌ |
| Bluetooth A2DP / HFP | Audio | ❌ |

### 12.5 Input drivers

| Driver | Class | Status |
|--------|-------|--------|
| evdev framework | Input | ❌ |
| HID parser | Input | ❌ |
| AT keyboard (i8042) | Input | ❌ |
| PS/2 mouse | Input | ❌ |
| USB HID (keyboard, mouse, gamepad) | Input | ❌ |
| Touchscreen (I2C Goodix, FocalTech, ELAN) | Input | ❌ |
| Touchpad (synaptics, elantech, I2C-HID) | Input | ❌ |
| Joystick / gamepad (XInput, DirectInput) | Input | ❌ |

### 12.6 USB drivers

| Driver | Class | Status |
|--------|-------|--------|
| xHCI host controller (TRB rings, streams) | USB | ❌ |
| xHCI debug capability | USB | ❌ |
| USB hub driver | USB | ❌ |
| USB mass storage class | USB | ❌ |
| USB HID class | USB | ❌ |
| USB audio class | USB | ❌ |
| USB CDC (Ethernet, serial) | USB | ❌ |
| USB video class (UVC webcams) | USB | ❌ |
| USB PD (power delivery) | USB | ❌ |
| OHCI / EHCI (legacy) | USB | ❌ |

### 12.7 Bluetooth drivers

| Driver | Class | Status |
|--------|-------|--------|
| HCI driver (UART, USB, SDIO) | BT | ❌ |
| L2CAP | BT | ❌ |
| ATT/GATT | BT | ❌ |
| RFCOMM | BT | ❌ |
| BLE advertising, scanning, connection | BT | ❌ |
| Profiles: A2DP, HFP, HID-over-GATT, BLE mesh | BT | ❌ |

### 12.8 Thermal / Power drivers

| Driver | Class | Status |
|--------|-------|--------|
| ACPI thermal zones | Thermal | ❌ |
| CPU thermal cooling (freq capping) | Thermal | ❌ |
| GPU thermal | Thermal | ❌ |
| Battery fuel gauge (I2C, ACPI) | Power | ❌ |
| Charger IC | Power | ❌ |
| USB PD controller | Power | ❌ |
| Fan controller | Thermal | ❌ |
| CPU idle states (C-states, mwait) | Power | ❌ |
| CPU frequency scaling (governors) | Power | ❌ |
| ACPI S-states (suspend, hibernate) | Power | ❌ |
| Runtime PM framework | Power | ❌ |
| CPU hotplug | Power | ❌ |

### 12.9 Misc platform drivers

| Driver | Class | Status |
|--------|-------|--------|
| I2C master (DesignWare, Synopsys, NXP, Broadcom) | I2C | ❌ |
| SPI master | SPI | ❌ |
| UART (8250, 16550, PL011) | Serial | ❌ |
| GPIO controllers (Intel pinctrl, Synopsys, Qualcomm) | GPIO | ❌ |
| DMA engines (Intel, Synopsys DMA, PL330) | DMA | ❌ |
| EEPROM / NVMEM | Misc | ❌ |
| Sensors (accel, gyro, mag, pressure, light, proximity) | Sensor | ❌ |

---

## 13. SoC / board support packages

For a real device you need board files / device tree blobs for every supported SoC.

| SoC | Architecture | Use case | Status |
|-----|--------------|----------|--------|
| Intel Tiger Lake / Alder Lake / Meteor Lake | x86_64 | Laptops | ❌ |
| AMD Ryzen 6000/7000/9000 | x86_64 | Laptops, handhelds | ❌ |
| Apple Silicon M1-M4 | aarch64 | MacBooks | ❌ — needs Apple's SEP/SMC/ANS |
| Qualcomm SM8550/SM8650 | aarch64 | Phones | ❌ |
| MediaTek Dimensity | aarch64 | Phones | ❌ |
| Samsung Exynos | aarch64 | Phones | ❌ |
| Google Tensor | aarch64 | Pixel | ❌ |
| Raspberry Pi 5 (BCM2712) | aarch64 | SBC | ❌ |
| Rockchip RK3588 | aarch64 | SBC, handhelds | ❌ |
| Steam Deck APU | x86_64 | Gaming handheld | ❌ |
| ASUS ROG Ally (AMD Z1 Extreme) | x86_64 | Gaming handheld | ❌ |
| Nintendo Switch (Tegra X1) | aarch64 | Gaming handheld | ❌ |
| RISC-V (SiFive, VisionFive) | riscv64 | SBC | ❌ — HAL backend doesn't exist |

---

## 14. Userspace ABI and runtime

### 14.1 What's missing

| Component | Status |
|-----------|--------|
| libc (printf, malloc, fopen, pthread, errno) | ❌ |
| Dynamic linker (ld.so) — .so loading, PLT/GOT, lazy binding | ❌ |
| POSIX IPC (message queues, semaphores, shared memory) | ❌ |
| Terminal layer — TTY, line discipline, PTY | ❌ |
| curses / terminfo | ❌ |
| pthreads / threads API | ❌ |
| getaddrinfo / DNS resolver | ❌ |
| SSL/TLS library | ❌ |

### 14.2 Stub — libc structure

The cleanest path is to port [rust-libc](https://github.com/rust-lang/libc) bindings and implement the underlying syscalls:

```rust
// crates/libc/src/lib.rs
#![no_std]

extern "C" {
    pub fn open(path: *const u8, flags: u32, mode: u32) -> i32;
    pub fn close(fd: i32) -> i32;
    pub fn read(fd: i32, buf: *mut u8, len: usize) -> isize;
    pub fn write(fd: i32, buf: *const u8, len: usize) -> isize;
    pub fn lseek(fd: i32, offset: i64, whence: i32) -> i64;
    pub fn stat(path: *const u8, stat: *mut Stat) -> i32;
    pub fn fstat(fd: i32, stat: *mut Stat) -> i32;
    pub fn mmap(addr: *mut u8, len: usize, prot: i32, flags: i32, fd: i32, off: i64) -> *mut u8;
    pub fn munmap(addr: *mut u8, len: usize) -> i32;
    pub fn fork() -> i32;
    pub fn execve(path: *const u8, argv: *const *const u8, envp: *const *const u8) -> i32;
    pub fn exit(code: i32) -> !;
    pub fn getpid() -> i32;
    pub fn waitpid(pid: i32, status: *mut i32, options: i32) -> i32;
    pub fn socket(domain: i32, ty: i32, protocol: i32) -> i32;
    pub fn bind(fd: i32, addr: *const SockAddr, len: u32) -> i32;
    pub fn listen(fd: i32, backlog: i32) -> i32;
    pub fn accept(fd: i32, addr: *mut SockAddr, len: *mut u32) -> i32;
    pub fn connect(fd: i32, addr: *const SockAddr, len: u32) -> i32;
    pub fn send(fd: i32, buf: *const u8, len: usize, flags: i32) -> isize;
    pub fn recv(fd: i32, buf: *mut u8, len: usize, flags: i32) -> isize;
    pub fn ioctl(fd: i32, cmd: u32, arg: usize) -> i32;
    pub fn futex(uaddr: *mut u32, op: u32, val: u32, timeout: *const Timespec, uaddr2: *mut u32, val3: u32) -> i32;
    pub fn epoll_create(size: i32) -> i32;
    pub fn epoll_ctl(epfd: i32, op: i32, fd: i32, event: *mut EpollEvent) -> i32;
    pub fn epoll_wait(epfd: i32, events: *mut EpollEvent, maxevents: i32, timeout: i32) -> i32;
    // ... hundreds more
}

#[repr(C)]
pub struct Stat {
    pub st_dev: u64, pub st_ino: u64, pub st_mode: u32, pub st_nlink: u32,
    pub st_uid: u32, pub st_gid: u32, pub st_rdev: u64, pub st_size: i64,
    pub st_blksize: i64, pub st_blocks: i64,
    pub st_atime: i64, pub st_mtime: i64, pub st_ctime: i64,
}
```

Each `extern "C"` function calls into the kernel via the syscall instruction.

---

## 15. Shell, coreutils, init

### 15.1 What's missing

| Component | Status |
|-----------|--------|
| Shell (bash, zsh, dash) | ❌ |
| Coreutils (ls, cd, cp, mv, rm, cat, echo, grep, sed, awk, find, chmod, chown, ps, kill, top, df, mount) | ❌ |
| Init system (systemd, OpenRC, s6, or simple init) | ❌ |
| Service manager — current `Supervisor` is just a data structure | ❌ |
| Display server (Wayland compositor, X server) | ❌ |
| Toolkit (Qt, GTK, Flutter, Android Runtime) | ❌ |

### 15.2 Stub — minimal init

```rust
// crates/init/src/main.rs
#![no_std]
#![no_main]

#[no_mangle]
pub extern "C" fn _start() -> ! {
    // 1. Mount /proc, /sys, /dev, /tmp
    // 2. Bring up loopback interface
    // 3. Read /etc/init.conf
    // 4. Spawn services listed in init.conf:
    //    - window-manager
    //    - audio-server
    //    - network-manager
    //    - bluetooth
    //    - power-manager
    // 5. Open a login shell on the console
    // 6. Reap zombies forever
    loop {
        // waitpid(-1, &status, 0);
    }
}
```

---

## 16. agex language completeness

### 16.1 What works today

- ✅ Lexer (full keyword set, comments, strings, numbers, all operators)
- ✅ Parser (recursive descent, all statement and expression forms)
- ✅ AST serialization (serde JSON)
- ✅ HIR lowering pass (pass-through today)
- ✅ Rust code generator (functions, classes, data classes, sealed classes, interfaces, objects, drivers, pattern matching, async, unsafe, FFI)
- ✅ String concatenation via `+` (emits `format!`)
- ✅ Method calls with `&self` / `&mut self` inference
- ✅ Field access rewriting (`hp` → `self.hp` in methods)
- ✅ Sealed variant constructor rewriting (`Damage.Physical(50.0)` → `Damage::Physical { amount: 50.0 }`)
- ✅ `agc` CLI with `--ast`, `--tokens`, `--check`, `-o`, stdin support
- ✅ Two example programs compile to valid Rust and run (`hello.agex`, `functions.agex`)

### 16.2 What's missing

| Feature | Status | Notes |
|---------|--------|-------|
| Type inference | ⚠️ uses Rust's `_` placeholder | Fails for `const`, complex generics |
| Borrow checker | ❌ | Rust's runs on generated code, but error messages map poorly back to agex source |
| Lifetime elision | ❌ | No actual lifetime analysis in agex itself |
| Trait resolution | ❌ | |
| Monomorphization | ❌ | |
| Default parameter values | ⚠️ parsed, not emitted | Rust has no default args; need builder pattern |
| Pattern matching exhaustiveness | ❌ | Compiler doesn't verify all branches covered |
| Standard library | ❌ | No `std::collections`, `std::io`, `std::net`, `std::sync` for agex |
| Async runtime | ❌ | No executor, no reactor, no `tokio`-equivalent |
| Panic handler and unwinder | ❌ | |
| Allocator integration | ❌ | agex programs can't call `alloc::alloc` because the kernel allocator isn't exposed via syscalls yet |
| Macro system | ❌ | |
| Compile-time function evaluation | ❌ | |
| Trait objects / dynamic dispatch | ❌ | |
| Closures | ❌ | |
| Iterators | ❌ | |
| Error propagation (`?` operator on `Result`) | ❌ | |
| Custom derive macros | ❌ | |
| Inline assembly in agex | ❌ | |
| Module system beyond `import` | ❌ | |
| Cross-compilation | ⚠️ | Works because Rust cross-compiles, but agex itself has no target spec |
| Incremental compilation | ❌ | |
| Debugger info (DWARF) emission | ❌ | |
| Source maps back to agex | ❌ | |

### 16.3 Stub — type checker

```rust
// crates/agex/src/types.rs
use crate::ast::*;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    Int(i_kind),       // i8, i16, i32, i64, u8, ..., usize
    Float(f_kind),     // f32, f64
    Bool,
    Char,
    Str,               // &str
    String,
    Array(Box<Type>, usize),
    Slice(Box<Type>),
    Tuple(Vec<Type>),
    Func { params: Vec<Type>, ret: Box<Type> },
    Struct(String),
    Enum(String),
    Trait(String),
    Generic(String),
    Pointer(Box<Type>, bool),  // *mut T or *const T
    Option(Box<Type>),
    Result(Box<Type>, Box<Type>),
    Unknown,
}

pub struct TypeChecker {
    /// Variable name -> type
    scopes: Vec<HashMap<String, Type>>,
    /// Class name -> field types
    classes: HashMap<String, Vec<(String, Type)>>,
    /// Sealed class name -> variant types
    sealed: HashMap<String, Vec<(String, Vec<(String, Type)>)>>,
    /// Function name -> signature
    functions: HashMap<String, (Vec<Type>, Type)>,
}

impl TypeChecker {
    pub fn check(&mut self, prog: &Program) -> Result<(), TypeError> {
        for decl in &prog.decls {
            match decl {
                Decl::Fn(f) => self.check_fn(f)?,
                Decl::Class(c) => self.check_class(c)?,
                Decl::Interface(i) => self.check_interface(i)?,
                Decl::Object(o) => self.check_object(o)?,
                Decl::Driver(d) => self.check_driver(d)?,
                Decl::Extern(e) => self.check_extern(e)?,
                Decl::Import(_) => {},
                Decl::Stmt(s) => { self.scopes.push(HashMap::new()); self.check_stmt(s)?; self.scopes.pop(); }
            }
        }
        Ok(())
    }

    fn check_expr(&mut self, e: &Expr) -> Result<Type, TypeError> {
        match e {
            Expr::Int { value } => {
                let v: i64 = value.parse().unwrap_or(0);
                if v >= i32::MIN as i64 && v <= i32::MAX as i64 { Ok(Type::Int("i32".into())) }
                else { Ok(Type::Int("i64".into())) }
            }
            Expr::Float { .. } => Ok(Type::Float("f64".into())),
            Expr::String { .. } => Ok(Type::Str),
            Expr::Bool { .. } => Ok(Type::Bool),
            Expr::Null => Ok(Type::Option(Box::new(Type::Unknown))),
            Expr::Ident { name } => self.lookup(name).ok_or(TypeError::UnknownIdent(name.clone())),
            Expr::Binary { op, left, right } => {
                let lt = self.check_expr(left)?;
                let rt = self.check_expr(right)?;
                match (op.as_str(), &lt, &rt) {
                    ("+", Type::Str, _) | (_, _, Type::Str) => Ok(Type::String),
                    _ if lt == rt => Ok(lt),
                    _ => Err(TypeError::TypeMismatch(lt, rt)),
                }
            }
            // ...
            _ => Ok(Type::Unknown),
        }
    }
}

#[derive(Debug)]
pub enum TypeError {
    UnknownIdent(String),
    TypeMismatch(Type, Type),
    NotCallable(Type),
    WrongArgCount { expected: usize, got: usize },
    MissingField { struct_name: String, field: String },
    UnknownType(String),
    BorrowConflict,
    LifetimeError,
}
```

---

## 17. Development tooling

### 17.1 What works today

- ✅ `agc` CLI fully functional (compile, --ast, --tokens, --check)
- ✅ `apm` CLI (stub — prints fake install messages)
- ✅ `agdb` CLI (stub — prints fake breakpoint info)
- ✅ `agprof` CLI (stub — prints fake frame-time report)

### 17.2 What's missing

| Component | Status |
|-----------|--------|
| `agdb` actual debugger | ❌ — no ptrace, no debug info, no DWARF emitter in `agc` |
| `agprof` actual profiler | ❌ — no perf event subsystem, no PMU, no stack walker |
| `agbuild` build system | ❌ |
| IDE integration (LSP for agex) | ❌ |
| Documentation generator (`cargo doc` equivalent) | ❌ |
| Package formatter (`rustfmt` equivalent) | ❌ |
| Linter (`clippy` equivalent) | ❌ |
| Dependency resolver in `apm` | ❌ |
| Signed package format | ❌ |
| Package registry server | ❌ |
| Build farm for reproducible builds | ❌ |

### 17.3 Stub — `agdb` real debugger

```rust
// crates/agdb/src/debugger.rs
use std::io::{BufRead, Write};

pub struct Debugger {
    pub target_pid: u32,
    pub breakpoints: std::collections::HashMap<u64, u8>,  // addr -> original byte
    pub watchpoints: Vec<WatchExpr>,
    pub source_map: SourceMap,  // agex line <-> Rust line <-> machine addr
}

impl Debugger {
    pub fn attach(pid: u32) -> Result<Self, DebugError> {
        // ptrace(PTRACE_ATTACH, pid, ...)
        // waitpid(pid, ...)
        unimplemented!()
    }

    pub fn set_breakpoint(&mut self, addr: u64) -> Result<(), DebugError> {
        // Read original byte: ptrace(PTRACE_PEEKTEXT, pid, addr)
        // Write 0xCC (int3): ptrace(PTRACE_POKETEXT, pid, addr, (orig & ~0xFF) | 0xCC)
        unimplemented!()
    }

    pub fn cont(&mut self) -> Result<StopReason, DebugError> {
        // ptrace(PTRACE_CONT, pid, ...)
        // waitpid with WIFSTOPPED
        // If SIGTRAP, check if rip-1 is a breakpoint
        unimplemented!()
    }

    pub fn step(&mut self) -> Result<StopReason, DebugError> {
        // ptrace(PTRACE_SINGLESTEP, pid, ...)
        unimplemented!()
    }

    pub fn read_regs(&self) -> Result<Regs, DebugError> {
        // ptrace(PTRACE_GETREGS, pid, ...)
        unimplemented!()
    }

    pub fn read_mem(&self, addr: u64, len: usize) -> Result<Vec<u8>, DebugError> {
        // ptrace(PTRACE_PEEKTEXT, pid, addr) in chunks
        unimplemented!()
    }

    pub fn repl(&mut self) -> ! {
        let stdin = std::io::stdin();
        let stdout = std::io::stdout();
        loop {
            write!(stdout.lock(), "(agdb) ").unwrap();
            let mut line = String::new();
            if stdin.lock().read_line(&mut line).unwrap() == 0 { std::process::exit(0); }
            let line = line.trim();
            match line {
                "run" => { self.cont().unwrap(); }
                "step" | "s" => { self.step().unwrap(); }
                "regs" => { println!("{:?}", self.read_regs().unwrap()); }
                "bt" | "backtrace" => { self.backtrace().unwrap(); }
                cmd if cmd.starts_with("b ") || cmd.starts_with("break ") => {
                    let loc = cmd.split_whitespace().nth(1).unwrap();
                    // resolve loc to addr via source_map
                }
                "quit" | "q" => std::process::exit(0),
                _ => println!("unknown command: {}", line),
            }
        }
    }
}

pub struct SourceMap {
    /// (agex_file, agex_line) -> Vec<(rust_file, rust_line)>
    pub agex_to_rust: std::collections::HashMap<(String, u32), Vec<(String, u32)>>,
    /// (rust_file, rust_line) -> Vec<addr>
    pub rust_to_addr: std::collections::HashMap<(String, u32), Vec<u64>>,
}

#[derive(Debug)]
pub enum StopReason { Breakpoint(u64), Signal(u32), Exited(i32), Signaled(u32) }
#[derive(Debug)]
pub enum DebugError { PtraceFailed, NoTarget, BadAddr }
```

---

## 18. Production hardening

Things you need before shipping to real users.

| Component | Status |
|-----------|--------|
| Fuzzing infrastructure (syzbot-equivalent) | ❌ |
| KASAN / KMSAN / UBSAN | ❌ |
| Lockdep | ❌ |
| Crash dump (kdump equivalent) | ❌ |
| Live patching (kpatch/livepatch equivalent) | ❌ |
| Module loading (`.ko`) | ❌ |
| Backward compatibility guarantees for syscall ABI | ❌ |
| Multi-architecture CI (QEMU runs on every commit) | ❌ |
| Real hardware test lab | ❌ |
| Update mechanism (atomic A/B updates, rollback, delta) | ❌ |
| Recovery mode (fallback boot, rescue shell) | ❌ |
| Disk encryption at install time | ❌ |
| Secure enrollment (provisioning, attestation) | ❌ |
| Telemetry with privacy (opt-in, on-device) | ❌ |
| Internationalization (UTF-8 throughout, IME, bidi text, font rendering) | ❌ |
| Font rendering (HarfBuzz, FreeType) | ❌ |
| Multi-monitor support | ❌ |
| HiDPI / fractional scaling | ❌ |
| Color management (ICC profiles) | ❌ |

---

## 19. Recommended order of attack

This is the most pragmatic order to make progress. Each phase is roughly
independent and produces a verifiable milestone.

### Phase 1 — Boot on QEMU (3-6 months)

1. **M1: `#[no_std]` the kernel** + custom allocator
2. **M2: `target.json` + linker script** for x86_64
3. **M3: GDT / IDT / TSS** on x86_64
4. **M4: Real page tables** (PML4 walk)
5. **M5: Context switch**
6. **M6: Syscall entry/exit**
7. **M7: Boot on QEMU** with `-kernel` and serial console
8. Verify: `cargo run` produces a binary that prints "zeroxos v0.1 booted" on QEMU serial

### Phase 2 — Run a userspace program (2-3 months)

1. Implement ELF loader
2. Implement VFS layer (just enough for `/dev/console`)
3. Implement `open` / `read` / `write` / `close` syscalls
4. Implement `fork` / `exec` / `exit` / `wait`
5. Write a tiny initramfs containing a hello-world program
6. Verify: kernel boots, execs hello-world, prints its output to serial

### Phase 3 — Filesystem on disk (2-3 months)

1. Implement PCI enumeration
2. Implement virtio-blk driver (simplest disk in QEMU)
3. Implement block device layer
4. Wire zeroxfs to the block device (write real on-disk format)
5. Implement `mount` / `umount`
6. Verify: boot from disk, mount root fs, run program from disk

### Phase 4 — Useful shell (3-4 months)

1. Implement PTY / TTY layer
2. Write a minimal shell (`/bin/sh`) — ~500 lines
3. Implement coreutils: `ls`, `cd`, `cat`, `echo`, `pwd`, `cp`, `mv`, `rm`, `mkdir`, `rmdir`
4. Implement `pipe` / `dup` / `dup2` / `fork` for pipes
5. Implement `exec` with `PATH` lookup
6. Verify: boot to a shell prompt, run `ls /`, `cat /etc/init.conf`, etc.

### Phase 5 — SMP and performance (3-4 months)

1. Implement SMP boot (AP bring-up)
2. Per-CPU runqueues with work stealing
3. Per-CPU data and per-CPU IDT
4. TLB shootdown IPIs
5. RCU for VFS
6. Verify: `nproc` shows N CPUs, `stress -c N` scales linearly

### Phase 6 — Networking (3-4 months)

1. Implement virtio-net driver
2. Implement packet sockets
3. Implement ARP, IPv4, ICMP, TCP, UDP
4. Implement socket API (`socket`, `bind`, `listen`, `accept`, `connect`, `send`, `recv`)
5. Implement `epoll`
6. Verify: `curl http://example.com/` works

### Phase 7 — Graphics (4-6 months)

1. Implement DRM/KMS framework
2. Implement bochs-drm or virtio-gpu driver (QEMU)
3. Implement `/dev/fb0` framebuffer
4. Implement simple display compositor
5. Verify: GUI windows render in QEMU

### Phase 8 — Real hardware (12+ months)

1. Port to a real x86_64 laptop (ThinkPad is the usual first target)
2. ACPI interpreter (huge — 6+ months alone)
3. Real GPU driver (Intel i915 is the most mature starting point)
4. Real WiFi driver
5. Real audio driver
6. Real input drivers

### Phase 9 — agex language (parallel, ongoing)

1. Type checker (Phase 1-2)
2. Borrow checker (Phase 3-4)
3. Standard library (Phase 5-6)
4. Async runtime (Phase 7)
5. Debug info emission (Phase 8)

### Phase 10 — Production (indefinite)

This is where Linux is now — 30 years of work.

---

## 20. Glossary

- **AP** — Application Processor (secondary CPU core)
- **AML** — ACPI Machine Language (interpreted bytecode in ACPI tables)
- **BSP** — Bootstrap Processor (the first CPU core)
- **CoW** — Copy-on-Write
- **CRTC** — CRT Controller (display scanout unit)
- **DRM** — Direct Rendering Manager (GPU subsystem)
- **EDID** — Extended Display Identification Data
- **GIC** — Generic Interrupt Controller (ARM)
- **GOT** — Global Offset Table (dynamic linking)
- **HPET** — High Precision Event Timer
- **IOMMU** — Input/Output Memory Management Unit
- **IO-APIC** — I/O Advanced Programmable Interrupt Controller (x86)
- **KASLR** — Kernel Address Space Layout Randomization
- **KMS** — Kernel Mode Setting
- **MCS lock** — Mellor-Crummey-Scott lock (fair spinlock)
- **MSI** — Message Signaled Interrupts
- **NUMA** — Non-Uniform Memory Access
- **PAC** — Pointer Authentication Code (ARM)
- **PCIe** — PCI Express
- **PLT** — Procedure Linkage Table (dynamic linking)
- **PML4** — Page Map Level 4 (top-level x86_64 page table)
- **PSCI** — Power State Coordination Interface (ARM)
- **RCU** — Read-Copy-Update
- **RTC** — Real-Time Clock
- **SEP** — Apple Secure Enclave Processor
- **SMMU** — System Memory Management Unit (ARM IOMMU)
- **SoC** — System on Chip
- **TSS** — Task State Segment (x86)
- **TSC** — Time Stamp Counter (x86)
- **TTBR** — Translation Table Base Register (ARM)
- **VFS** — Virtual File System
- **VMA** — Virtual Memory Area

---

## Appendix A — Current file inventory

```
zeroxos-os/
├── Cargo.toml                          # Workspace
├── README.md                           # Project overview
├── ROADMAP.md                          # This file
├── .gitignore
├── examples/
│   ├── hello.agex                      # ✅ Compiles + runs
│   ├── functions.agex                  # ✅ Compiles + runs
│   ├── game.agex                       # ⚠️ Compiles but Rust errors
│   └── driver.agex                     # ✅ Compiles
└── crates/
    ├── agex/                           # ✅ Compiler (lexer, parser, AST, HIR, codegen, agc CLI)
    │   └── src/
    │       ├── lib.rs
    │       ├── lexer.rs (265 LOC)
    │       ├── ast.rs (149 LOC)
    │       ├── parser.rs (614 LOC)
    │       ├── hir.rs (~40 LOC)
    │       ├── codegen.rs (616 LOC)
    │       └── bin/agc.rs (117 LOC)
    ├── hal/                            # ⚠️ Host works, x86_64/aarch64 are stubs
    │   └── src/
    │       ├── lib.rs
    │       ├── cpu.rs
    │       ├── memory.rs (99 LOC)
    │       ├── interrupt.rs
    │       ├── timer.rs
    │       ├── power.rs
    │       └── arch/
    │           ├── mod.rs
    │           ├── host.rs (104 LOC)    # ✅ Works
    │           ├── x86_64.rs (120 LOC)  # ⚠️ inline asm stubs
    │           ├── aarch64.rs (144 LOC) # ⚠️ inline asm stubs
    │           └── stub.rs              # fallback
    ├── zerox-kernel/                   # ⚠️ Library only, no real arch integration
    │   └── src/
    │       ├── lib.rs (98 LOC)
    │       ├── scheduler.rs (319 LOC)  # ✅ MLFQ+CFS+RT, gaming mode
    │       ├── memory.rs (282 LOC)     # ✅ Buddy+slab, ⚠️ no real page tables
    │       ├── ipc.rs (193 LOC)        # ✅ Fast msgs + shm + capabilities
    │       ├── security.rs (137 LOC)   # ✅ Capability data structures
    │       ├── process.rs (166 LOC)    # ✅ Process/thread table
    │       └── driver.rs (143 LOC)     # ⚠️ Registry only, no real drivers
    ├── zerox-fs/                       # ⚠️ In-memory only
    │   └── src/
    │       ├── lib.rs
    │       ├── superblock.rs           # ✅ CRC32 verified
    │       ├── journal.rs (88 LOC)     # ✅ Begin/commit/replay
    │       ├── checksum.rs             # ✅ CRC32 (with tests)
    │       ├── cow.rs (88 LOC)         # ✅ CoW tree with snapshots
    │       └── compression.rs          # ⚠️ Format only, no real codec
    ├── zerox-runtime/                  # ⚠️ Skeleton
    │   └── src/
    │       ├── lib.rs
    │       ├── window.rs               # ✅ Window mgr data
    │       ├── audio.rs                # ✅ Audio server data
    │       ├── network.rs              # ✅ Net mgr data
    │       ├── power.rs                # ✅ Power mgr data
    │       ├── package.rs              # ✅ Package service data
    │       └── supervisor.rs           # ⚠️ No real supervision
    ├── zerox-sim/                      # ✅ Working
    │   └── src/
    │       ├── main.rs                 # CLI dispatch
    │       └── demo.rs (498 LOC)       # boot/game/ipc/fs/run demos
    ├── apm/                            # ⚠️ CLI stub
    │   └── src/main.rs
    ├── agdb/                           # ⚠️ CLI stub
    │   └── src/main.rs
    └── agprof/                         # ⚠️ CLI stub
        └── src/main.rs
```

## Appendix B — How to add a new driver

Pattern for adding any new device driver:

1. **Identify the bus** (PCI, USB, I2C, SPI, platform)
2. **Add a `Driver` entry** in `crates/zerox-kernel/src/driver.rs`:
   ```rust
   self.register(Driver::new("intel-i915", DriverClass::Gpu, DriverLocation::Kernel, "GpuInterface"));
   ```
3. **Create the driver crate or module:**
   ```
   crates/drivers-intel-i915/
   ├── Cargo.toml
   └── src/
       ├── lib.rs                # Driver entry point
       ├── pci.rs                # PCI probe/discovery
       ├── ring.rs               # GPU command ring buffer
       ├── context.rs            # GPU context management
       └── regs.rs               # Register definitions
   ```
4. **Implement the bus probe:**
   ```rust
   pub fn probe(pci: &PciDevice) -> Option<Self> {
       if pci.vendor_id == 0x8086 && pci.class_code == 0x03 {
           Some(IntelI915::new(pci)?)
       } else { None }
   }
   ```
5. **Implement the HAL interface** (`GpuInterface` trait in this case)
6. **Register in the driver framework** at boot
7. **Test under QEMU** with the matching emulated device (`-device virtio-gpu`)

## Appendix C — How to add a new syscall

1. **Add to syscall table** in `crates/zerox-kernel/src/syscall/mod.rs`:
   ```rust
   pub const SYS_OPEN: u64 = 10;
   pub const SYS_READ: u64 = 11;
   // ...
   ```
2. **Implement the handler:**
   ```rust
   fn sys_open(path: UserStr, flags: u32, mode: u32) -> Result<u32, SysError> {
       let path = path.copy_from_user()?;
       let inode = vfs::resolve(&path)?;
       let file = inode.open(flags)?;
       Ok(current_task().fd_table.open(file))
   }
   ```
3. **Wire in the dispatch table:**
   ```rust
   match nr {
       10 => sys_open(...),
       11 => sys_read(...),
       // ...
   }
   ```
4. **Add to libc bindings** in `crates/libc/src/lib.rs`
5. **Document the ABI** — once shipped, this is set in stone

---

**End of roadmap.** Pick a milestone, implement it, send a PR.
