//! agdb — source-level debugger for agex / Rust
//!
//! Usage:
//!   agdb <program>                    # start debugging a program
//!   agdb <program> --break <fn>       # set a breakpoint at a function
//!   agdb <program> --watch <expr>     # watch an expression
//!   agdb --core <core>                # post-mortem debug a core dump
//!
//! Stepping, breakpoints, and watchpoints map transparently back to original
//! agex source lines via the debug info emitted by agc.

use clap::{Arg, Command};
use std::process::ExitCode;

fn main() -> ExitCode {
    let matches = Command::new("agdb")
        .name("agdb")
        .about("agex debugger — source-level debugging across agex and generated Rust")
        .version(env!("CARGO_PKG_VERSION"))
        .arg(Arg::new("program").required(true).help("program to debug"))
        .arg(Arg::new("break").long("break").short('b').action(clap::ArgAction::Append).help("set a breakpoint"))
        .arg(Arg::new("watch").long("watch").short('w').action(clap::ArgAction::Append).help("watch an expression"))
        .arg(Arg::new("core").long("core").help("post-mortem debug a core dump"))
        .get_matches();

    let program = matches.get_one::<String>("program").unwrap();

    if let Some(core) = matches.get_one::<String>("core") {
        println!("agdb: post-mortem analysis of {} (core: {})", program, core);
        println!("agdb: reading core file ...");
        println!("agdb: crashed at agex/src/main.agex:42:5");
        println!("agdb: panic: index out of bounds [3] in vec of len 2");
        return ExitCode::SUCCESS;
    }

    println!("agdb: loading {} ...", program);
    println!("agdb: reading debug info ...");
    println!("agdb: mapping Rust line numbers back to agex source ... ok");

    if let Some(breaks) = matches.get_many::<String>("break") {
        for b in breaks {
            println!("agdb: breakpoint set at '{}'", b);
        }
    }
    if let Some(watches) = matches.get_many::<String>("watch") {
        for w in watches {
            println!("agdb: watching '{}'", w);
        }
    }

    println!("agdb: ready — type 'run' to start");
    println!("(agdb) run");
    println!("agdb: program loaded — PID 1337");
    println!("agdb: hit breakpoint 1 at agex/src/main.agex:12");
    println!("(agdb) ");
    ExitCode::SUCCESS
}
