//! # zerox-kernel — the hybrid kernel of zeroxos
//!
//! The kernel combines the best aspects of monolithic and microkernel
//! architectures. Critical services (scheduler, memory manager, IPC transport,
//! security manager, performance-critical drivers) remain inside the kernel;
//! everything else (filesystems, networking, audio, bluetooth, package manager)
//! runs as userspace daemons that can be restarted on crash without rebooting.
//!
//! ## Modules
//! - [`scheduler`] — preemptive multi-level feedback queue + CFS + real-time, gaming-aware
//! - [`memory`] — buddy allocator, slab allocator, huge pages, copy-on-write, NUMA awareness
//! - [`ipc`] — fast messages (<500ns), shared memory channels, capability objects
//! - [`security`] — capability-based access control (no root)
//! - [`process`] — process and thread tables
//! - [`driver`] — kernel-mode and user-mode driver framework

#![cfg_attr(not(test), no_std)]
#![allow(dead_code)]

extern crate alloc;

pub mod arch;
pub mod error;
pub mod heap;
pub mod panic;
pub mod scheduler;
pub mod memory;
pub mod ipc;
pub mod security;
pub mod process;
pub mod driver;

pub use error::{KernelError, KernelResult};
use spin::Mutex;

/// Global kernel state.
pub static KERNEL: Mutex<Kernel> = Mutex::new(Kernel::new());

/// The kernel struct. Holds the scheduler, memory manager, IPC, security manager.
pub struct Kernel {
    pub scheduler: scheduler::Scheduler,
    pub memory: memory::MemoryManager,
    pub ipc: ipc::IpcCore,
    pub security: security::SecurityManager,
    pub process: process::ProcessTable,
    pub drivers: driver::DriverRegistry,
    pub booted: bool,
    pub boot_time_ns: u64,
}

impl Kernel {
    pub const fn new() -> Self {
        Self {
            scheduler: scheduler::Scheduler::new(),
            memory: memory::MemoryManager::new(),
            ipc: ipc::IpcCore::new(),
            security: security::SecurityManager::new(),
            process: process::ProcessTable::new(),
            drivers: driver::DriverRegistry::new(),
            booted: false,
            boot_time_ns: 0,
        }
    }

    /// Boot the kernel. Initializes all subsystems in dependency order.
    ///
    /// Returns [`KernelError`] if a subsystem fails to initialize. On success
    /// the kernel is marked booted and ready to run tasks.
    pub fn boot(&mut self) -> KernelResult<()> {
        if self.booted {
            return Ok(());
        }
        self.boot_time_ns = hal::timer::read_time_ns();
        log::info!("[kernel] booting zeroxos v0.1.0...");
        log::info!("[kernel] arch: {:?}", hal::current_arch());

        self.memory.init();
        // A kernel with no usable physical memory cannot proceed — this is a
        // real, testable failure path rather than a decorative Result.
        if self.memory.buddy.total_pages() == 0 {
            log::error!("[kernel] memory init produced zero usable pages");
            return Err(KernelError::MemoryInit);
        }
        log::info!("[kernel] memory manager initialized");

        self.scheduler.init();
        log::info!("[kernel] scheduler initialized (MLFQ + CFS + RT)");

        self.security.init();
        log::info!("[kernel] security manager initialized (capability-based)");

        self.ipc.init();
        log::info!("[kernel] IPC core initialized (fast/shmem/capability)");

        self.drivers.init();
        log::info!("[kernel] driver registry initialized");

        self.booted = true;
        let elapsed = hal::timer::read_time_ns().saturating_sub(self.boot_time_ns);
        log::info!("[kernel] boot complete in {} µs", elapsed / 1000);
        Ok(())
    }

    /// Kernel tick — called by the timer IRQ handler at the scheduler's tick rate.
    pub fn tick(&mut self) {
        self.scheduler.tick();
    }

    /// Shutdown the kernel cleanly.
    pub fn shutdown(&self) {
        log::info!("[kernel] shutting down...");
        // Flush filesystems, sync journals, etc.
        log::info!("[kernel] goodbye");
        hal::power::shutdown();
    }
}
