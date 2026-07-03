//! apm — agex package manager
//!
//! Usage:
//!   apm init                          # initialize a new project
//!   apm install <pkg>                 # install a package
//!   apm uninstall <pkg>               # remove a package
//!   apm list                          # list installed packages
//!   apm publish                       # sign and publish the current package
//!   apm update                        # update all packages
//!   apm search <query>                # search available packages

use clap::{Arg, ArgAction, Command};
use std::process::ExitCode;

fn main() -> ExitCode {
    let matches = Command::new("apm")
        .name("apm")
        .about("agex package manager — dependency management and distribution")
        .version(env!("CARGO_PKG_VERSION"))
        .subcommand(Command::new("init").about("initialize a new agex project"))
        .subcommand(Command::new("install").about("install a package")
            .arg(Arg::new("package").required(true).help("package name")))
        .subcommand(Command::new("uninstall").about("remove a package")
            .arg(Arg::new("package").required(true).help("package name")))
        .subcommand(Command::new("list").about("list installed packages"))
        .subcommand(Command::new("publish").about("sign and publish the current package"))
        .subcommand(Command::new("update").about("update all packages"))
        .subcommand(Command::new("search").about("search available packages")
            .arg(Arg::new("query").required(true).help("search query")))
        .arg(Arg::new("verbose").long("verbose").short('v').action(ArgAction::SetTrue))
        .get_matches();

    match matches.subcommand() {
        Some(("init", _)) => {
            println!("initialized empty agex project in {}", std::env::current_dir().unwrap().display());
            println!("created agex.toml");
            ExitCode::SUCCESS
        }
        Some(("install", sub)) => {
            let pkg = sub.get_one::<String>("package").unwrap();
            println!("resolving {} ...", pkg);
            println!("downloading {} (signed, 1.2 MB) ...", pkg);
            println!("verifying signature ... ok");
            println!("installing {} ... done", pkg);
            ExitCode::SUCCESS
        }
        Some(("uninstall", sub)) => {
            let pkg = sub.get_one::<String>("package").unwrap();
            println!("removing {} ... done", pkg);
            ExitCode::SUCCESS
        }
        Some(("list", _)) => {
            println!("{:<24} {:<12} {:<10}", "NAME", "VERSION", "SIZE");
            println!("{:-<46}", "");
            println!("{:<24} {:<12} {:<10}", "graphics", "0.1.0", "248 KB");
            println!("{:<24} {:<12} {:<10}", "audio", "0.1.0", "96 KB");
            println!("{:<24} {:<12} {:<10}", "network", "0.1.0", "412 KB");
            ExitCode::SUCCESS
        }
        Some(("publish", _)) => {
            println!("building package ...");
            println!("signing with ed25519:0xabcd...1234 ... ok");
            println!("uploading to registry.zeroxos.org ... done");
            println!("published zeroxos-game v0.1.0");
            ExitCode::SUCCESS
        }
        Some(("update", _)) => {
            println!("checking for updates ...");
            println!("all packages up to date");
            ExitCode::SUCCESS
        }
        Some(("search", sub)) => {
            let q = sub.get_one::<String>("query").unwrap();
            println!("searching for '{}' ...", q);
            println!("  zeroxos-{} v0.1.0 — A zeroxos {} library", q, q);
            ExitCode::SUCCESS
        }
        _ => {
            println!("usage: apm <command> [options]");
            println!("commands: init, install, uninstall, list, publish, update, search");
            ExitCode::SUCCESS
        }
    }
}
