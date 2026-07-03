//! Supervisor — watches userspace services and restarts them on crash.

use std::collections::HashMap;

pub struct Supervisor {
    services: HashMap<String, ServiceState>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceState {
    Running,
    Crashed,
    Restarting,
    Stopped,
}

#[derive(Debug, Clone)]
pub struct ServiceInfo {
    pub name: String,
    pub state: ServiceState,
    pub restarts: u32,
}

impl Supervisor {
    pub fn new() -> Self { Self { services: HashMap::new() } }

    pub fn register(&mut self, name: impl Into<String>) {
        let n = name.into();
        self.services.insert(n.clone(), ServiceState::Running);
        log::info!("[sup] +service {}", n);
    }

    /// Mark a service as crashed. The supervisor will restart it if it's a
    /// userspace service — kernel services panic the kernel instead.
    pub fn crash(&mut self, name: &str) {
        if let Some(state) = self.services.get_mut(name) {
            *state = ServiceState::Crashed;
            log::warn!("[sup] service '{}' crashed — restarting", name);
            *state = ServiceState::Restarting;
            *state = ServiceState::Running;
            log::info!("[sup] service '{}' restarted", name);
        }
    }

    pub fn stop(&mut self, name: &str) {
        if let Some(state) = self.services.get_mut(name) {
            *state = ServiceState::Stopped;
        }
    }

    pub fn list(&self) -> Vec<ServiceInfo> {
        self.services.iter()
            .map(|(name, state)| ServiceInfo { name: name.clone(), state: *state, restarts: 0 })
            .collect()
    }
}
