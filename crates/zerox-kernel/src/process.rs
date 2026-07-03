//! # Process and thread tables
//!
//! Tracks all processes and threads in the system. Each process has a
//! capability set, an address space, and one or more threads. Threads are
//! the schedulable unit.

use alloc::string::String;
use alloc::vec::Vec;
use crate::security::CapabilitySet;

/// Process identifier.
pub type Pid = u64;
/// Thread identifier.
pub type ThreadId = u64;

/// Process state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessState {
    /// Just created, not yet runnable
    New,
    /// Running or ready to run
    Runnable,
    /// Blocked on IPC, I/O, or a lock
    Blocked,
    /// Exited normally with exit code
    Exited(i32),
    /// Killed by signal or panic
    Killed,
}

/// A process.
#[derive(Debug)]
pub struct Process {
    pub pid: Pid,
    pub parent: Option<Pid>,
    pub name: String,
    pub state: ProcessState,
    pub threads: Vec<ThreadId>,
    pub caps: CapabilitySet,
    pub is_game: bool,
    pub is_foreground: bool,
    pub started_ns: u64,
    pub cpu_time_ns: u64,
}

impl Process {
    pub fn new(pid: Pid, name: impl Into<String>) -> Self {
        Self {
            pid,
            parent: None,
            name: name.into(),
            state: ProcessState::New,
            threads: Vec::new(),
            caps: CapabilitySet::new(),
            is_game: false,
            is_foreground: false,
            started_ns: 0,
            cpu_time_ns: 0,
        }
    }
}

/// Thread state (scheduler view).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThreadState {
    Ready,
    Running,
    Blocked,
    Exited,
}

/// A thread — the schedulable unit.
#[derive(Debug, Clone)]
pub struct Thread {
    pub tid: ThreadId,
    pub pid: Pid,
    pub state: ThreadState,
    pub user_stack_top: u64,
    pub kernel_stack_top: u64,
}

impl Thread {
    pub fn new(tid: ThreadId, pid: Pid) -> Self {
        Self {
            tid, pid,
            state: ThreadState::Ready,
            user_stack_top: 0,
            kernel_stack_top: 0,
        }
    }
}

/// The global process/thread table.
pub struct ProcessTable {
    pub processes: Vec<Process>,
    pub threads: Vec<Thread>,
    next_pid: u64,
    next_tid: u64,
}

impl ProcessTable {
    pub const fn new() -> Self {
        Self {
            processes: Vec::new(),
            threads: Vec::new(),
            next_pid: 1,
            next_tid: 1,
        }
    }

    pub fn spawn(&mut self, name: impl Into<String>) -> Pid {
        let pid = self.next_pid;
        self.next_pid += 1;
        let mut p = Process::new(pid, name);
        p.started_ns = hal::timer::read_time_ns();
        p.state = ProcessState::Runnable;
        self.processes.push(p);
        log::info!("[proc] spawn pid={}", pid);
        pid
    }

    pub fn spawn_thread(&mut self, pid: Pid) -> ThreadId {
        let tid = self.next_tid;
        self.next_tid += 1;
        let t = Thread::new(tid, pid);
        self.threads.push(t);
        if let Some(p) = self.processes.iter_mut().find(|p| p.pid == pid) {
            p.threads.push(tid);
        }
        log::info!("[proc] +thread tid={} pid={}", tid, pid);
        tid
    }

    pub fn kill(&mut self, pid: Pid) {
        if let Some(p) = self.processes.iter_mut().find(|p| p.pid == pid) {
            p.state = ProcessState::Killed;
            log::info!("[proc] kill pid={} ({})", pid, p.name);
        }
        self.threads.retain(|t| t.pid != pid);
    }

    pub fn exit(&mut self, pid: Pid, code: i32) {
        if let Some(p) = self.processes.iter_mut().find(|p| p.pid == pid) {
            p.state = ProcessState::Exited(code);
            log::info!("[proc] exit pid={} ({}) code={}", pid, p.name, code);
        }
        self.threads.retain(|t| t.pid != pid);
    }

    pub fn get(&self, pid: Pid) -> Option<&Process> {
        self.processes.iter().find(|p| p.pid == pid)
    }

    pub fn get_mut(&mut self, pid: Pid) -> Option<&mut Process> {
        self.processes.iter_mut().find(|p| p.pid == pid)
    }

    pub fn process_count(&self) -> usize { self.processes.len() }
    pub fn thread_count(&self) -> usize { self.threads.len() }

    pub fn list(&self) {
        log::info!("[proc] {} processes, {} threads", self.processes.len(), self.threads.len());
        for p in &self.processes {
            log::info!("  pid={} {:?} {} caps={} threads={}",
                p.pid, p.state, p.name, p.caps.capabilities.len(), p.threads.len());
        }
    }
}
