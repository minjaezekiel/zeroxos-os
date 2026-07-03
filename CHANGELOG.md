# Changelog

All notable changes to zeroxos are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

This file is the single source of truth for **what changed, what is functional,
and what is non-functional** at any point in time. Every milestone in
`ROADMAP.md` closes with an entry here so any contributor (or future session)
can see exactly where to resume. See `docs/MANUAL.md` for the live
functional/non-functional matrix.

## [Unreleased]

Work in progress: **Phase 1 — Boot on real hardware (x86_64 → QEMU)**.
See `ROADMAP.md` §2 (milestones M1–M7) and the Phase 1 plan.

### Added
- **Engineering infrastructure (Milestone 0).**
  - `rust-toolchain.toml` pinning nightly `2026-07-03` with `rust-src`,
    `llvm-tools-preview`, `rustfmt`, `clippy`, and the `x86_64-unknown-none`
    target — required for `build-std`, custom bare-metal targets, and
    `#[naked]` functions in later milestones.
  - This `CHANGELOG.md` and `docs/MANUAL.md`, seeded with the current state.
  - `Makefile` with `sim`, `game`, `ipc`, `fs`, `test`, `fmt`, `clippy`,
    `build-x86_64`, and `qemu-x86_64` targets.
  - `KernelError` / `KernelResult` (`crates/zerox-kernel/src/error.rs`) — a
    `core`-only error enum (no `std`, no `thiserror`). `Kernel::boot()` now
    returns `KernelResult<()>` and fails with `KernelError::MemoryInit` if no
    usable physical memory is registered (a real, tested failure path).
- **`#![no_std]` kernel + kernel heap allocator (Milestone 1).**
  - `zerox-kernel` and `zerox-fs` are now `#![no_std]` (via
    `cfg_attr(not(test), no_std)` so host unit tests keep `std`), using
    `alloc::{vec, string, format}` instead of the `std` prelude.
  - New `crates/zerox-kernel/src/heap.rs`: a first-fit free-list kernel heap
    (`Heap` / `LockedHeap`) with block splitting and alignment support, plus 5
    host unit tests (within-arena, non-overlap, alignment, reuse, OOM).
  - New `crates/zerox-kernel/src/panic.rs`: a `#[panic_handler]` compiled only
    under the `bare` feature.
  - `bare` cargo feature on `zerox-kernel` gates the `#[global_allocator]` and
    `#[panic_handler]`; the default (host/sim) build provides both via `std`.

- **Custom bare-metal target, linker script & boot entry (Milestone 2).**
  - `targets/x86_64-unknown-zeroxos.json` — freestanding higher-half x86_64
    target (soft-float, no red zone, `code-model: kernel`, static reloc, panic
    = abort), validated against the current nightly target-spec schema.
  - `linker/x86_64.ld` — links the kernel at `0xffffffff80200000` with separate
    R+X / R / R+W program headers and a `.bss` for the early heap.
  - `crates/zeroxos-boot` — the freestanding (`#![no_std] #![no_main]`) binary.
    `_start` initializes a 1 MiB static early heap, calls `hal::init()`, and
    boots the kernel. Excluded from `default-members` so host builds ignore it.
  - `.cargo/config.toml` — per-target `rustflags` (linker script + gc-sections)
    and a QEMU `runner`, plus a `cargo kbuild` alias. Deliberately sets **no**
    global default target / `build-std`, so the host sim is unaffected.
  - `hal` is now `#![no_std]` when the `host` feature is off (the `host` backend
    still uses `std`).

### Changed
- `zerox-sim` boot call sites handle the new `Result` from `Kernel::boot()`.
- HAL backend selection is now feature-driven: `hal` defaults to **no** backend
  (set in the workspace manifest, since Cargo ignores per-member
  `default-features` overrides on inherited deps); host crates opt in with
  `features = ["host"]`, and the bare target selects `arch/x86_64.rs`.
- `make test` / `make clippy` operate on `default-members` (host crates), so the
  bare-only crate is never compiled for the host.

### Functional (new this cycle)
- **First bootable artifact.** `make build-x86_64` (or `cargo kbuild`) produces
  `target/x86_64-unknown-zeroxos/release/zeroxos-boot` — a statically-linked
  64-bit ELF with `.text` at `0xffffffff80200000` and the kernel + early heap
  linked in. (Not yet run in QEMU — that needs the multiboot2 header, stack
  setup, and serial logger from M7.)

### Functional (new this cycle)
- Kernel and filesystem compile as genuine `#![no_std]` crates (verified: the
  non-test `cargo build` compiles them with `no_std` active) while the host
  simulator (`boot`/`game`/`ipc`/`fs`) continues to run unchanged.
- `cargo test --workspace` green, including the new heap and error tests.

- **GDT, IDT & TSS (Milestone 3).**
  - New `crates/zerox-kernel/src/arch/` module tree (`arch/x86_64/{gdt,idt,tss,
    mod}.rs`), isolating all CPU-specific kernel code behind
    `#[cfg(target_arch)]`.
  - **GDT** ([gdt.rs](crates/zerox-kernel/src/arch/x86_64/gdt.rs)): null + kernel
    code/data + user code/data + a 16-byte TSS descriptor, with `const`
    descriptor encoders. Loads via `lgdt`, reloads segment registers (CS via a
    `retfq` far return), and loads the TSS via `ltr`.
  - **TSS** ([tss.rs](crates/zerox-kernel/src/arch/x86_64/tss.rs)): architectural
    104-byte `#[repr(C, packed)]` layout with `rsp0` and 7 IST slots.
  - **IDT** ([idt.rs](crates/zerox-kernel/src/arch/x86_64/idt.rs)): 256-entry
    table with `const` gate encoders, 32 naked ISR stubs (correct error-code vs
    dummy handling per vector), a shared `isr_common` trampoline that builds a
    full `InterruptFrame` and calls a Rust `isr_dispatch`.
  - `arch::x86_64::init()` installs all three; wired into `zeroxos-boot::_start`.
  - **15 new host unit tests** for the descriptor/gate bit encoders (canonical
    long-mode segment values, TSS base/limit split, IDT offset split, DPL/gate
    type, IST masking).
  - Fixed a latent bug in `hal::arch::x86_64`: `shutdown`/`reboot` used AT&T
    `outw`/`outb` mnemonics invalid in Rust's Intel-syntax asm → now `out dx, ..`.

- **4-level page tables (Milestone 4).**
  - New `crates/hal/src/arch/x86_64/paging.rs` — a PML4→PDPT→PD→PT walker:
    `map_page_in` / `unmap_page_in` / `translate_in`, PTE flag encoding
    (`pte_flags_from`, incl. NX-by-default), `virt_to_indices`, and 2 MiB/1 GiB
    huge-page translation. The walk is abstracted over `PhysMapper` +
    `FrameAllocator` traits so it is **fully unit-tested on the host** (7 tests:
    map→translate roundtrip, double-map rejection, unmap, independent pages,
    OOB) against a fixed table pool.
  - Compiled on any x86_64 target (independent of the `host` feature) so the
    tests run under `cargo test`.
  - The bare `hal::arch::x86_64` `map_page`/`unmap_page` now bind that logic to
    the live CPU: `CR3` for the root PML4, a physical direct-map
    (`set_direct_map_base`), a kernel-registered frame allocator
    (`set_frame_allocator`), and `invlpg` TLB flushes — replacing the former
    `unimplemented!()` panics.

- **Context switch (Milestone 5).**
  - New `crates/zerox-kernel/src/arch/x86_64/context.rs` — a minimal
    `TaskContext` (`rsp` + `cr3`) and a naked `switch_to` implementing the
    classic cooperative switch (callee-saved regs live on each task's own kernel
    stack; `ret` reloads RIP). `init_kernel_task` forges a fresh task's initial
    stack frame. **3 host tests** (field offsets vs the asm contract via
    `offset_of!`, forged-frame layout, empty context). Corrects the roadmap's
    stub, which tried to `mov rip` directly. FPU/SSE save is intentionally
    omitted (kernel target is soft-float).
- **Fast system calls (Milestone 6).**
  - New `crates/zerox-kernel/src/arch/x86_64/syscall.rs` — a host-tested
    `dispatch` table (`exit`/`write`/`read` → `-ENOSYS` for unknown), the MSR
    programming (`init_syscalls`: EFER.SCE + NXE, STAR, LSTAR, FMASK), and a
    naked `syscall_entry`/`sysret` trampoline that builds a `SyscallFrame` and
    calls a Rust `syscall_handler`. **4 host tests** for dispatch routing.
  - Reordered the GDT so **user-data (0x18) precedes user-code (0x20)** — required
    for `sysret` to derive the correct user `CS`/`SS` from `STAR[63:48]`.
  - `init_syscalls()` is wired into `arch::x86_64::init()` after the GDT loads.

### Non-functional (deliberately deferred)
- The `zeroxos-boot` image links but is **not runnable in QEMU yet**: it has no
  multiboot2 header, sets up no stack, and installs no serial logger, so it
  produces no output. That is milestone **M7** (also: QEMU must be installed —
  `brew install qemu`).
- The bare page-table path compiles and is host-tested as logic, but its live
  behaviour (direct-map base, `set_frame_allocator` → buddy) is only exercised
  once we boot in QEMU (M7).
- The `syscall_entry` stub does not yet switch to a per-CPU kernel stack and
  assumes a single CPU — sufficient for Phase 1 (nothing issues a `syscall`
  until userspace exists in Phase 2). `switch_to` is not yet driven by the
  scheduler (the scheduler still models threads without real register state).
- `isr_dispatch` halts on any exception; serial dump + page-fault handling
  arrive in **M7**. ISR-stub / syscall-stub stack alignment is deferred
  (harmless while soft-float/no-SSE).
- `hal::arch::x86_64::allocate_dma` and the bare IRQ paths are still
  `unimplemented!()`.
- Bare-metal builds require nightly `-Z` flags (`json-target-spec`, `build-std`);
  baked into `make build-x86_64` and the `cargo kbuild` alias.

### Notes
- Phase 1 status: **M0–M6 complete**; only **M7 (first QEMU boot)** remains.
  M7 adds a multiboot2 header + assembly stub (stack setup), a 0x3F8 serial
  logger, and boots the image in QEMU. It requires QEMU to be installed.

## [0.1.0] — foundational skeleton

The initial skeleton: a well-organized design with working **host-side
simulations** of every intended subsystem, runnable entirely on a developer's
machine, plus a functional toy compiler for the `agex` language.

### Functional (verified running under `zerox-sim`)
- **`zerox-sim`** CLI (`boot | game | ipc | fs | compile | run`) — boots the
  kernel struct in-process via the `host` HAL backend and runs demos.
- **`zerox-kernel`** — `Kernel::boot()` initializes memory, scheduler, security,
  IPC, and driver subsystems in dependency order with structured logging.
  - Buddy allocator (10 orders, 4 KB → 2 MB) with splitting and coalescing.
  - MLFQ + CFS + RT scheduler with gaming-mode priority boosts and
    frame-deadline awareness (single global run queue).
  - IPC: fast messages (~84–152 ns/msg, target < 500 ns), shared-memory
    channels, capability objects.
  - Capability-based security manager (no root).
- **`zerox-fs`** — superblock (CRC32), write-ahead journal (begin/commit/replay),
  CoW B-tree with snapshots, checksums. All **in-memory** (no on-disk format).
- **`agex`** compiler — lexer → recursive-descent parser → AST/HIR → Rust
  codegen, plus the `agc` CLI. `hello.agex` and `functions.agex` compile to
  valid Rust and run.
- **`hal` host backend** — runs the whole stack as a normal userspace process.

### Non-functional / stubbed
- **Bare-metal HAL** (`hal::arch::{x86_64,aarch64}`) — `map_page`, `unmap_page`,
  `allocate_dma`, `register_irq`, `hibernate` are `unimplemented!()`.
- **Kernel is `std`-based**, not `#![no_std]` — cannot target bare metal.
- No bootloader, custom target spec, linker script, page tables, context switch,
  syscalls, ELF loader, VFS, or real drivers.
- `zerox-runtime` (window/audio/network/power/package/supervisor) — data-structure
  skeletons; no real supervision.
- `apm`, `agdb`, `agprof` — hardcoded `println!` stubs.
- `agex` — type inference is a stub (emits `_`); `game.agex` compiles but the
  generated Rust does not.
- `zerox-fs` compression — format only, no real LZ4/Zstd codec.
