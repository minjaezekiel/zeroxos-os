//! # Driver framework
//!
//! Three driver models, one HAL surface:
//!
//! - **Kernel-mode drivers** — for latency-critical devices (GPU, NVMe, NIC)
//! - **User-mode drivers** — for recoverable devices (USB peripherals, printers, sensors)
//! - **Paravirtualized drivers** — for VMs and cloud (virtio)

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

/// Driver class — what kind of device this driver manages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DriverClass {
    Gpu,
    Nvme,
    Usb,
    Display,
    Network,
    Bluetooth,
    Audio,
    Sensor,
    Printer,
    Virtio,
}

/// Driver location — kernel or user space.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriverLocation {
    /// In-kernel for minimum latency
    Kernel,
    /// In userspace for crash recovery
    User,
    /// Paravirtualized — guest/host cooperate via shared memory rings
    Paravirtualized,
}

/// Driver state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriverState {
    NotLoaded,
    Loaded,
    Initialized,
    Running,
    Crashed,
    Unloaded,
}

/// A registered driver.
#[derive(Debug)]
pub struct Driver {
    pub name: String,
    pub class: DriverClass,
    pub location: DriverLocation,
    pub state: DriverState,
    pub implements: String,
}

impl Driver {
    pub fn new(name: impl Into<String>, class: DriverClass, location: DriverLocation, implements: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            class,
            location,
            state: DriverState::NotLoaded,
            implements: implements.into(),
        }
    }
}

/// The driver registry — tracks all loaded drivers.
pub struct DriverRegistry {
    pub drivers: Vec<Driver>,
}

impl DriverRegistry {
    pub const fn new() -> Self { Self { drivers: Vec::new() } }

    pub fn init(&mut self) {
        // Register a baseline set of drivers that exist in every zeroxos system.
        self.register(Driver::new("gpu-nvidia", DriverClass::Gpu, DriverLocation::Kernel, "GpuInterface"));
        self.register(Driver::new("gpu-amd", DriverClass::Gpu, DriverLocation::Kernel, "GpuInterface"));
        self.register(Driver::new("gpu-intel", DriverClass::Gpu, DriverLocation::Kernel, "GpuInterface"));
        self.register(Driver::new("gpu-mali", DriverClass::Gpu, DriverLocation::Kernel, "GpuInterface"));
        self.register(Driver::new("gpu-adreno", DriverClass::Gpu, DriverLocation::Kernel, "GpuInterface"));
        self.register(Driver::new("nvme", DriverClass::Nvme, DriverLocation::Kernel, "BlockDriver"));
        self.register(Driver::new("xhci", DriverClass::Usb, DriverLocation::Kernel, "UsbHost"));
        self.register(Driver::new("display", DriverClass::Display, DriverLocation::Kernel, "DisplayController"));
        self.register(Driver::new("audio-hda", DriverClass::Audio, DriverLocation::User, "AudioInterface"));
        self.register(Driver::new("bluetooth", DriverClass::Bluetooth, DriverLocation::User, "BtInterface"));
        self.register(Driver::new("virtio-gpu", DriverClass::Virtio, DriverLocation::Paravirtualized, "GpuInterface"));
        self.register(Driver::new("virtio-blk", DriverClass::Virtio, DriverLocation::Paravirtualized, "BlockDriver"));
        self.register(Driver::new("virtio-net", DriverClass::Virtio, DriverLocation::Paravirtualized, "NetDriver"));
        log::info!("[drv] {} drivers registered", self.drivers.len());
    }

    pub fn register(&mut self, driver: Driver) {
        log::trace!("[drv] +{} ({:?}/{:?})", driver.name, driver.class, driver.location);
        self.drivers.push(driver);
    }

    pub fn load(&mut self, name: &str) -> Result<(), &'static str> {
        let d = self.drivers.iter_mut().find(|d| d.name == name).ok_or("driver not found")?;
        d.state = DriverState::Loaded;
        log::info!("[drv] loaded {}", name);
        Ok(())
    }

    pub fn init_driver(&mut self, name: &str) -> Result<(), &'static str> {
        let d = self.drivers.iter_mut().find(|d| d.name == name).ok_or("driver not found")?;
        if d.state != DriverState::Loaded { return Err("driver not loaded"); }
        d.state = DriverState::Initialized;
        log::info!("[drv] initialized {}", name);
        Ok(())
    }

    pub fn start(&mut self, name: &str) -> Result<(), &'static str> {
        let d = self.drivers.iter_mut().find(|d| d.name == name).ok_or("driver not found")?;
        if d.state != DriverState::Initialized { return Err("driver not initialized"); }
        d.state = DriverState::Running;
        log::info!("[drv] started {}", name);
        Ok(())
    }

    pub fn crash(&mut self, name: &str) {
        if let Some(d) = self.drivers.iter_mut().find(|d| d.name == name) {
            d.state = DriverState::Crashed;
            log::warn!("[drv] CRASHED {} ({:?}) — supervisor will restart", name, d.class);
            // User-mode drivers can be restarted automatically by the supervisor.
            if d.location == DriverLocation::User {
                log::info!("[drv] restarting user-mode driver {}", name);
                d.state = DriverState::Running;
            }
        }
    }

    pub fn count(&self) -> usize { self.drivers.len() }

    pub fn list(&self) {
        log::info!("[drv] {} drivers", self.drivers.len());
        for d in &self.drivers {
            log::info!("  {:<20} {:<10} {:<15} {:?}",
                d.name, format!("{:?}", d.class), d.implements, d.state);
        }
    }
}
