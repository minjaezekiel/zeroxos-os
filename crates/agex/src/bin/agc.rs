//! agc — the agex compiler CLI.
//!
//! Usage:
//!   agc <input.agex>                  # prints generated Rust to stdout
//!   agc <input.agex> -o output.rs     # writes generated Rust to file
//!   agc <input.agex> --emit rust      # explicit emit kind (only 'rust' supported)
//!   agc --check <input.agex>          # type-check / parse only, no codegen
//!   agc --ast <input.agex>            # prints AST as JSON
//!   agc --version

use agex::{parse, lower, generate, tokenize};
use clap::{Arg, ArgAction, Command};
use std::fs;
use std::io::{self, Read, Write};
use std::process::ExitCode;

fn read_stdin() -> String {
    let mut buf = String::new();
    if io::stdin().read_to_string(&mut buf).is_err() {
        eprintln!("agc: failed to read from stdin");
        std::process::exit(1);
    }
    buf
}

fn main() -> ExitCode {
    let matches = Command::new("agc")
        .name("agc")
        .about("agex compiler — translates agex source to Rust")
        .version(env!("CARGO_PKG_VERSION"))
        .arg(Arg::new("input").required(false).help("input .agex file (use '-' or omit for stdin)"))
        .arg(Arg::new("emit").long("emit").help("emit kind (rust, ast)").default_value("rust"))
        .arg(Arg::new("output").short('o').long("output").help("output file (default: stdout)"))
        .arg(Arg::new("check").long("check").action(ArgAction::SetTrue).help("parse only, no codegen"))
        .arg(Arg::new("ast").long("ast").action(ArgAction::SetTrue).help("print AST as JSON"))
        .arg(Arg::new("tokens").long("tokens").action(ArgAction::SetTrue).help("print tokens"))
        .get_matches();

    // Read source
    let src = match matches.get_one::<String>("input") {
        None => read_stdin(),
        Some(path) if path == "-" => read_stdin(),
        Some(path) => match fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("agc: failed to read {}: {}", path, e);
                return ExitCode::FAILURE;
            }
        },
    };

    // --tokens
    if matches.get_flag("tokens") {
        match tokenize(&src) {
            Ok(tokens) => {
                for t in tokens {
                    println!("{:>4}:{:<3} {:<14} {}", t.line, t.col, format!("{:?}", t.ty), t.value);
                }
                return ExitCode::SUCCESS;
            }
            Err(e) => { eprintln!("agc: {}", e); return ExitCode::FAILURE; }
        }
    }

    // Parse
    let ast = match parse(&src) {
        Ok(a) => a,
        Err(e) => { eprintln!("agc: {}", e); return ExitCode::FAILURE; }
    };

    // --check (parse only)
    if matches.get_flag("check") {
        println!("ok: {} declarations parsed", ast.decls.len());
        return ExitCode::SUCCESS;
    }

    // --ast
    if matches.get_flag("ast") {
        match serde_json::to_string_pretty(&ast) {
            Ok(s) => { println!("{}", s); return ExitCode::SUCCESS; }
            Err(e) => { eprintln!("agc: failed to serialize AST: {}", e); return ExitCode::FAILURE; }
        }
    }

    // Default: lower + generate Rust
    let hir = lower(&ast);
    let result = generate(&hir);

    // Warnings
    for w in &result.warnings { eprintln!("agc: warning: {}", w); }

    let emit = matches.get_one::<String>("emit").map(|s| s.as_str()).unwrap_or("rust");
    let output = match emit {
        "rust" => result.rust,
        "ast" => serde_json::to_string_pretty(&ast).unwrap_or_else(|e| format!("error: {}", e)),
        other => { eprintln!("agc: unknown emit kind '{}'", other); return ExitCode::FAILURE; }
    };

    // Write
    match matches.get_one::<String>("output") {
        Some(path) => {
            if let Err(e) = fs::write(path, output) {
                eprintln!("agc: failed to write {}: {}", path, e);
                return ExitCode::FAILURE;
            }
        }
        None => {
            let stdout = io::stdout();
            let mut lock = stdout.lock();
            if let Err(e) = lock.write_all(output.as_bytes()) {
                eprintln!("agc: failed to write to stdout: {}", e);
                return ExitCode::FAILURE;
            }
        }
    }
    ExitCode::SUCCESS
}
