//! # zerox-sim — host-runnable simulation of zeroxos
//!
//! Boots the zeroxos kernel as a userspace process. The HAL `host` feature
//! maps hardware operations to host OS calls (threads, allocation, time).
//!
//! zerox-sim can:
//! - Boot the kernel and initialize all subsystems
//! - Spawn userspace processes and threads
//! - Run the scheduler with MLFQ + CFS + RT in gaming mode
//! - Compile and execute agex programs (via agex → Rust source)
//! - Exercise the IPC, security, and driver subsystems
//! - Mount zeroxfs and run filesystem operations
//!
//! ## Usage
//!
//! ```sh
//! # Boot the OS and run the boot demo
//! zerox-sim boot
//!
//! # Compile an agex file to Rust
//! zerox-sim compile path/to/main.agex
//!
//! # Boot + compile + run an agex program under the simulator
//! zerox-sim run path/to/main.agex
//!
//! # Run the gaming workload demo
//! zerox-sim game
//! ```

use clap::{Arg, Command};
use std::process::ExitCode;

mod demo;

fn main() -> ExitCode {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp_millis()
        .init();

    let matches = Command::new("zerox-sim")
        .name("zerox-sim")
        .about("zeroxos host simulator — boots the kernel as a userspace process")
        .version(env!("CARGO_PKG_VERSION"))
        .subcommand(Command::new("boot").about("boot the kernel and run the boot demo"))
        .subcommand(Command::new("game").about("boot + run the gaming workload demo"))
        .subcommand(Command::new("compile")
            .about("compile an agex file to Rust source")
            .arg(Arg::new("input").required(true).help(".agex source file")))
        .subcommand(Command::new("run")
            .about("boot + compile + run an agex program under the simulator")
            .arg(Arg::new("input").required(true).help(".agex source file")))
        .subcommand(Command::new("fs")
            .about("boot + mount zeroxfs and run filesystem demo"))
        .subcommand(Command::new("ipc")
            .about("boot + run IPC fast-message benchmark"))
        .get_matches();

    match matches.subcommand() {
        Some(("boot", _)) => demo::boot_demo(),
        Some(("game", _)) => demo::gaming_demo(),
        Some(("compile", sub)) => {
            let input = sub.get_one::<String>("input").unwrap();
            demo::compile_file(input)
        }
        Some(("run", sub)) => {
            let input = sub.get_one::<String>("input").unwrap();
            demo::run_agex_file(input)
        }
        Some(("fs", _)) => demo::filesystem_demo(),
        Some(("ipc", _)) => demo::ipc_demo(),
        _ => {
            println!("usage: zerox-sim <command>");
            println!("commands: boot, game, compile <file.agex>, run <file.agex>, fs, ipc");
            ExitCode::SUCCESS
        }
    }
}
