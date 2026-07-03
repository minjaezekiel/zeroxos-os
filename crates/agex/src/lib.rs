//! # agex compiler
//!
//! Translates agex source into Rust, which is then compiled by `rustc`.
//!
//! Pipeline:
//!   source → lexer → tokens → parser → AST → HIR → codegen → Rust source → rustc
//!
//! ## Example
//!
//! ```no_run
//! use agex::transpile;
//!
//! let src = r#"
//!     fn add(a: int, b: int) -> int = a + b
//!     print(add(2, 3))
//! "#;
//!
//! let result = transpile(src).unwrap();
//! println!("{}", result.rust_source);
//! ```

pub mod ast;
pub mod lexer;
pub mod parser;
pub mod hir;
pub mod codegen;

pub use lexer::{tokenize, LexerError, Token, TokenType};
pub use parser::{parse, ParseError};
pub use ast::Program;
pub use codegen::{generate, GenResult};
pub use hir::lower;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum CompileError {
    #[error("lexer error: {0}")]
    Lexer(#[from] LexerError),
    #[error("parse error: {0}")]
    Parse(#[from] ParseError),
}

#[derive(Debug)]
pub struct TranspileResult {
    pub rust_source: String,
    pub warnings: Vec<String>,
}

pub fn transpile(src: &str) -> Result<TranspileResult, CompileError> {
    let ast = parse(src)?;
    let hir = lower(&ast);
    let GenResult { rust, warnings } = generate(&hir);
    Ok(TranspileResult {
        rust_source: rust,
        warnings,
    })
}
