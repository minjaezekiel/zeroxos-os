# zeroxos Manual

Practical guide to building, running, testing, and hacking on zeroxos. This
document tracks the **live functional/non-functional state** of the system so
you always know what works and where to start. It is updated at the close of
every roadmap milestone alongside `CHANGELOG.md`.

- Vision & architecture: `README.md`
- Full remaining scope & milestone recipes: `ROADMAP.md`
- What changed, release by release: `CHANGELOG.md`

---

## 1. Prerequisites

- **Rust nightly** (pinned by `rust-toolchain.toml`). `rustup` selects it
  automatically inside this repo. Components: `rust-src`, `llvm-tools-preview`.
- **QEMU** (`qemu-system-x86_64`) — only needed from milestone M7 onward for
  bare-metal boot. Not required for the host simulator.
  - macOS: `brew install qemu`

The **host simulator** builds and runs on stable Rust too, but the pinned
nightly is used repo-wide for consistency and is required for the bare-metal
build steps.

---

## 2. Building & running the host simulator

The simulator runs the real kernel as an ordinary userspace process (the `host`
HAL backend). This is the fast iteration loop — no VM, no hardware.

```bash
make sim                      # or: cargo run -p zerox-sim -- boot
cargo run -p zerox-sim -- game    # gaming-mode scheduler demo
cargo run -p zerox-sim -- ipc     # IPC latency benchmark
cargo run -p zerox-sim -- fs      # filesystem demo
cargo run -p zerox-sim -- compile examples/hello.agex
cargo run -p zerox-sim -- run     examples/functions.agex
```

Set `RUST_LOG=debug` (or `trace`) for verbose subsystem logging.

---

## 3. Testing

```bash
make test                     # or: cargo test --workspace
```

Pure-logic modules (allocator order math, page-table index math, descriptor
bit encoders, ELF header parsing, syscall dispatch) are unit-tested on the host
so encoding bugs are caught without QEMU. Integration tests boot the kernel
in-sim.

---

## 4. Building the bare-metal kernel (x86_64) — *from M2 onward*

```bash
make build-x86_64             # builds the custom target with build-std → a bootable ELF
make qemu-x86_64              # boots the kernel image in QEMU (serial console) — from M7
```

`make build-x86_64` works today (M2): it produces
`target/x86_64-unknown-zeroxos/release/zeroxos-boot`, a statically-linked ELF
with `.text` at `0xffffffff80200000`. It requires the pinned nightly and passes
`-Z json-target-spec -Z build-std=core,compiler_builtins,alloc` under the hood.
`make qemu-x86_64` will only produce output once M7 adds the multiboot2 header,
stack setup, and serial logger. See `ROADMAP.md` §2.

---

## 5. Functional / non-functional matrix

Legend: ✅ works · ⚠️ partial/simulated · ❌ not started

| Subsystem | State | Notes |
|---|---|---|
| Host simulator (`zerox-sim`) | ✅ | boot/game/ipc/fs/compile/run |
| Kernel boot sequence | ✅ | in-sim; ordered subsystem init + logging |
| Buddy allocator | ✅ | in-memory; 4 KB–2 MB, coalescing |
| Scheduler (MLFQ+CFS+RT) | ⚠️ | logic works; single global run queue, no real context switch |
| IPC (fast/shmem/capability) | ✅ | in-sim, meets latency target |
| Security (capabilities) | ⚠️ | checks modeled; not enforced at a real syscall boundary yet |
| zeroxfs (superblock/journal/CoW) | ⚠️ | in-memory only; no on-disk format, no block device |
| agex compiler + `agc` | ⚠️ | compiles simple programs; type inference stubbed |
| HAL host backend | ✅ | runs kernel as a process |
| HAL x86_64 bare backend | ❌ | `unimplemented!()` for map/unmap/dma/irq/hibernate |
| HAL aarch64 bare backend | ❌ | same; deferred until after x86_64 boots |
| `#![no_std]` kernel + kernel heap | ✅ | **M1 done** — kernel & fs are `no_std`; free-list heap (`heap.rs`), `bare`-gated global allocator + panic handler |
| `KernelError` / `Result` boot | ✅ | `boot()` returns `KernelResult<()>` |
| Custom target + linker script | ✅ | **M2 done** — `x86_64-unknown-zeroxos` target, higher-half linker script |
| Bare-metal ELF (`zeroxos-boot`) | ✅ | **M2 done** — `make build-x86_64` links a bootable ELF (not run in QEMU until M7) |
| GDT / IDT / TSS | ✅ | **M3 done** — segments + TSS + 256-entry IDT with 32 exception stubs; installed at boot |
| Page tables | ✅ | **M4 done** — host-tested 4-level PML4 walker; bare `map_page`/`unmap_page` bound to CR3 + direct-map + frame hook |
| Context switch | ✅ | **M5 done** — `TaskContext` + naked `switch_to`; `init_kernel_task` forges new-thread frames (not yet scheduler-driven) |
| Syscalls | ✅ | **M6 done** — host-tested dispatch table; `syscall`/`sysret` MSR setup + naked entry; installed at boot |
| QEMU boot | ❌ | **M7 target** (Phase 1 exit) — needs QEMU installed |
| ELF loader / userspace | ❌ | Phase 2 |
| On-disk fs / virtio-blk | ❌ | Phase 3 |
| Shell / coreutils | ❌ | Phase 4 |
| SMP | ❌ | Phase 5 |
| Networking | ❌ | Phase 6 |
| Graphics / desktop UX | ❌ | Phase 7+ (incl. Windows-like file manager) |

---

## 6. Repository layout

```
crates/
  agex/          agex language compiler + agc CLI
  hal/           Hardware Abstraction Layer (arch/{host,x86_64,aarch64})
  zerox-kernel/  hybrid kernel (scheduler, memory, ipc, security, process, driver)
  zerox-fs/      zeroxfs (superblock, journal, checksum, cow, compression)
  zerox-runtime/ userspace service skeletons
  zerox-sim/     host simulator CLI (the runnable "OS" today)
  apm/agdb/agprof/  package manager / debugger / profiler CLI stubs
examples/        *.agex sample programs
docs/            this manual
```

Directories added during Phase 1 (as milestones land): `targets/` (custom target
JSON), `linker/` (linker scripts), `boot/` (multiboot2 entry), and
`crates/zeroxos-boot/` (the bare-metal binary entry point).

---

## 7. Where to start next

Check `CHANGELOG.md` `[Unreleased]` and the matrix above for the first `❌`.
The active milestone is always the lowest-numbered incomplete one in
`ROADMAP.md` §2. Currently: **M7 — first QEMU boot** (the Phase 1 exit): add
`boot/x86_64-boot.S` (multiboot2 header + stack), a 0x3F8 serial logger backing
the `log` crate, wire the panic handler / `isr_dispatch` to dump over serial,
and boot with `make qemu-x86_64`. **Requires QEMU** (`brew install qemu`).
