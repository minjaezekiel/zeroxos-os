//! # zerox-runtime — userspace services
//!
//! All of these run as userspace daemons. A crash here never brings down the
//! kernel — the supervisor simply restarts the affected service.

pub mod window;
pub mod audio;
pub mod network;
pub mod power;
pub mod package;
pub mod supervisor;

pub use window::WindowManager;
pub use audio::AudioServer;
pub use network::NetworkManager;
pub use power::PowerManager;
pub use package::PackageService;
pub use supervisor::Supervisor;
