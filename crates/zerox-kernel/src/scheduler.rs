//! # Process scheduler
//!
//! Three scheduling policies compose:
//! - **MLFQ** (Multi-Level Feedback Queue) for interactive tasks
//! - **CFS** (Completely Fair Scheduler) for long-running background work
//! - **RT** (Real-Time) for hard-deadline threads (audio, VR compositor, GPU present)
//!
//! In **Gaming Mode**, foreground game threads get +20 priority and the scheduler
//! synchronizes with GPU frame presentation. Background services get -10 priority
//! and are throttled when a game holds focus.

use alloc::vec::Vec;
use crate::process::ThreadId;
use core::sync::atomic::{AtomicU64, Ordering};

/// Scheduling policy for a thread.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Policy {
    /// Multi-Level Feedback Queue — interactive tasks, decays down on CPU use
    Mlfq,
    /// Completely Fair Scheduler — long-running background work
    Cfs,
    /// Real-Time — hard deadlines, fixed-priority FIFO
    Realtime { priority: u8 },
}

impl Default for Policy {
    fn default() -> Self { Policy::Mlfq }
}

/// Base priority for a thread. Range -20 (lowest) to +20 (highest).
/// Gaming Mode applies a +20 boost to foreground game threads and a -10
/// penalty to background services.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Priority(pub i8);

impl Priority {
    pub const LOWEST: Self = Self(-20);
    pub const NORMAL: Self = Self(0);
    pub const HIGHEST: Self = Self(20);
    pub const GAMING_FOREGROUND: Self = Self(20);
    pub const GAMING_BACKGROUND: Self = Self(-10);

    pub fn boost(self, delta: i8) -> Self {
        Self(self.0.saturating_add(delta).clamp(-20, 20))
    }
}

/// Thread state in the scheduler.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThreadState {
    Ready,
    Running,
    Blocked,
    Sleeping { until_ns: u64 },
    Exited,
}

/// Scheduler-internal per-thread info.
#[derive(Debug, Clone)]
pub struct SchedThread {
    pub id: ThreadId,
    pub policy: Policy,
    pub priority: Priority,
    pub state: ThreadState,
    pub cpu_affinity: u64,        // bitmask of allowed CPUs
    pub pinned_cpu: Option<u32>,  // hard pin (e.g. render thread)
    pub vruntime: u64,            // for CFS
    pub quantum_used: u64,        // for MLFQ
    pub mlfq_level: u8,           // 0 = highest queue, N = lowest
    pub frame_deadline_ns: Option<u64>,  // for RT threads synchronized with GPU
    pub is_game_render: bool,
    pub is_game_physics: bool,
    pub is_game_audio: bool,
    pub last_run_ns: u64,
    pub total_run_ns: u64,
}

impl SchedThread {
    pub fn new(id: ThreadId) -> Self {
        Self {
            id,
            policy: Policy::Mlfq,
            priority: Priority::NORMAL,
            state: ThreadState::Ready,
            cpu_affinity: !0,
            pinned_cpu: None,
            vruntime: 0,
            quantum_used: 0,
            mlfq_level: 0,
            frame_deadline_ns: None,
            is_game_render: false,
            is_game_physics: false,
            is_game_audio: false,
            last_run_ns: 0,
            total_run_ns: 0,
        }
    }
}

/// Number of MLFQ priority levels.
pub const MLFQ_LEVELS: u8 = 4;
/// Time slice (quantum) for MLFQ level 0, in nanoseconds. Doubles per level.
pub const MLFQ_BASE_QUANTUM_NS: u64 = 1_000_000; // 1 ms
/// CFS target latency — all runnable CFS threads get a slice within this window.
pub const CFS_TARGET_LATENCY_NS: u64 = 6_000_000; // 6 ms
/// CFS minimum granularity — no thread gets a slice smaller than this.
pub const CFS_MIN_GRANULARITY_NS: u64 = 750_000; // 0.75 ms

/// The scheduler state.
pub struct Scheduler {
    threads: Vec<SchedThread>,
    current: Option<ThreadId>,
    gaming_mode: bool,
    /// Foreground game PID — its threads get the +20 priority boost.
    foreground_game: Option<u64>,
    tick_count: AtomicU64,
    last_frame_complete_ns: u64,
}

impl Scheduler {
    pub const fn new() -> Self {
        Self {
            threads: Vec::new(),
            current: None,
            gaming_mode: false,
            foreground_game: None,
            tick_count: AtomicU64::new(0),
            last_frame_complete_ns: 0,
        }
    }

    pub fn init(&mut self) {
        log::info!("[sched] MLFQ levels={}, base quantum={}µs", MLFQ_LEVELS, MLFQ_BASE_QUANTUM_NS / 1000);
        log::info!("[sched] CFS target latency={}µs, min granularity={}µs",
            CFS_TARGET_LATENCY_NS / 1000, CFS_MIN_GRANULARITY_NS / 1000);
    }

    pub fn thread_count(&self) -> usize { self.threads.len() }
    pub fn is_gaming_mode(&self) -> bool { self.gaming_mode }
    pub fn current_thread(&self) -> Option<ThreadId> { self.current }

    /// Add a new thread to the scheduler.
    pub fn add_thread(&mut self, thread: SchedThread) {
        log::trace!("[sched] +thread {:?} policy={:?} prio={}", thread.id, thread.policy, thread.priority.0);
        self.threads.push(thread);
    }

    /// Remove a thread (e.g. after it exits).
    pub fn remove_thread(&mut self, id: ThreadId) {
        self.threads.retain(|t| t.id != id);
        if self.current == Some(id) {
            self.current = None;
        }
    }

    /// Enable/disable gaming mode. When enabled, the foreground game's threads
    /// get +20 priority and background services get -10.
    pub fn set_gaming_mode(&mut self, enabled: bool, game_pid: Option<u64>) {
        self.gaming_mode = enabled;
        self.foreground_game = game_pid;
        if enabled {
            log::info!("[sched] gaming mode ON — foreground game pid={:?}", game_pid);
        } else {
            log::info!("[sched] gaming mode OFF");
        }
    }

    /// Called by the timer IRQ at the scheduler's tick rate (default 100 Hz).
    pub fn tick(&mut self) {
        let _n = self.tick_count.fetch_add(1, Ordering::Relaxed);
    }

    /// Pick the next thread to run. This is the core of the scheduler.
    ///
    /// Selection order:
    /// 1. RT threads with imminent frame deadlines
    /// 2. RT threads (fixed priority)
    /// 3. MLFQ threads (highest level first, then by priority)
    /// 4. CFS threads (lowest vruntimes first)
    pub fn pick_next(&mut self) -> Option<ThreadId> {
        let now = hal::timer::read_time_ns();

        // 1. RT threads with imminent frame deadlines
        let mut best_rt_deadline: Option<(ThreadId, u64)> = None;
        for t in &self.threads {
            if t.state != ThreadState::Ready { continue; }
            if let Policy::Realtime { .. } = t.policy {
                if let Some(deadline) = t.frame_deadline_ns {
                    if best_rt_deadline.map_or(true, |(_, d)| deadline < d) {
                        best_rt_deadline = Some((t.id, deadline));
                    }
                }
            }
        }
        if let Some((id, deadline)) = best_rt_deadline {
            if deadline <= now + 1_000_000 {
                self.current = Some(id);
                return Some(id);
            }
        }

        // 2. RT threads (fixed priority)
        let mut best_rt: Option<(ThreadId, u8)> = None;
        for t in &self.threads {
            if t.state != ThreadState::Ready { continue; }
            if let Policy::Realtime { priority } = t.policy {
                if best_rt.map_or(true, |(_, p)| priority > p) {
                    best_rt = Some((t.id, priority));
                }
            }
        }
        if let Some((id, _)) = best_rt {
            self.current = Some(id);
            return Some(id);
        }

        // 3. MLFQ threads (highest level = lowest number, then by priority)
        let mut best_mlfq: Option<(ThreadId, u8, i8)> = None;
        for t in &self.threads {
            if t.state != ThreadState::Ready { continue; }
            if t.policy != Policy::Mlfq { continue; }
            let prio = if self.gaming_mode {
                if t.is_game_render || t.is_game_physics || t.is_game_audio {
                    t.priority.boost(20).0
                } else {
                    t.priority.boost(-10).0
                }
            } else {
                t.priority.0
            };
            if best_mlfq.map_or(true, |(_, l, p)| {
                t.mlfq_level < l || (t.mlfq_level == l && prio > p)
            }) {
                best_mlfq = Some((t.id, t.mlfq_level, prio));
            }
        }
        if let Some((id, _, _)) = best_mlfq {
            self.current = Some(id);
            return Some(id);
        }

        // 4. CFS threads (lowest vruntime first)
        let mut best_cfs: Option<(ThreadId, u64)> = None;
        for t in &self.threads {
            if t.state != ThreadState::Ready { continue; }
            if t.policy != Policy::Cfs { continue; }
            if best_cfs.map_or(true, |(_, v)| t.vruntime < v) {
                best_cfs = Some((t.id, t.vruntime));
            }
        }
        if let Some((id, _)) = best_cfs {
            self.current = Some(id);
            return Some(id);
        }

        self.current = None;
        None
    }

    /// Compute the time slice for the currently-running thread.
    pub fn current_quantum_ns(&self) -> u64 {
        let Some(id) = self.current else { return 0 };
        let Some(t) = self.threads.iter().find(|t| t.id == id) else { return 0 };
        match t.policy {
            Policy::Realtime { .. } => u64::MAX, // RT runs until it blocks or yields
            Policy::Mlfq => {
                let level = t.mlfq_level as u32;
                MLFQ_BASE_QUANTUM_NS << level
            }
            Policy::Cfs => {
                let n_cfs = self.threads.iter().filter(|t| t.policy == Policy::Cfs && t.state == ThreadState::Ready).count().max(1) as u64;
                (CFS_TARGET_LATENCY_NS / n_cfs).max(CFS_MIN_GRANULARITY_NS)
            }
        }
    }

    /// Called when the running thread blocks/yields — update its state.
    pub fn yield_thread(&mut self, id: ThreadId) {
        if let Some(t) = self.threads.iter_mut().find(|t| t.id == id) {
            t.state = ThreadState::Ready;
            // MLFQ: decay to a lower queue if quantum was consumed
            if t.policy == Policy::Mlfq && t.mlfq_level < MLFQ_LEVELS - 1 {
                t.mlfq_level += 1;
            }
            t.quantum_used = 0;
        }
        if self.current == Some(id) {
            self.current = None;
        }
    }

    /// Called when the running thread's quantum expires.
    pub fn preempt(&mut self) {
        if let Some(id) = self.current {
            self.yield_thread(id);
        }
    }

    /// Called by the GPU driver when a frame completes — promotes the render
    /// thread for the next frame to minimize input-to-photon latency.
    pub fn on_frame_complete(&mut self, render_thread: ThreadId) {
        let now = hal::timer::read_time_ns();
        self.last_frame_complete_ns = now;
        if let Some(t) = self.threads.iter_mut().find(|t| t.id == render_thread) {
            t.mlfq_level = 0;
            t.priority = Priority::GAMING_FOREGROUND;
            log::trace!("[sched] frame complete — render thread {:?} promoted", render_thread);
        }
    }

    /// Print a scheduler state summary (for debugging / agprof).
    pub fn dump(&self) {
        log::info!("[sched] threads={} gaming={} current={:?}", self.threads.len(), self.gaming_mode, self.current);
        for t in &self.threads {
            log::info!("  {:?} {:?} prio={} state={:?} vruntime={}µs",
                t.id, t.policy, t.priority.0, t.state, t.vruntime / 1000);
        }
    }
}
