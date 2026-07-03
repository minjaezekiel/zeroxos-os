//! agex HIR (High-level IR) — a thin lowering pass over the AST.
//!
//! The HIR is structurally identical to the AST today, but exists as a separate
//! stage so that future passes (type inference, borrow-checking, monomorphization,
//! lifetime elision, capability resolution) can run here without polluting either
//! the parser or the code generator.

use crate::ast::*;

#[derive(Debug, Clone)]
pub struct HirProgram {
    pub decls: Vec<HirDecl>,
}

#[derive(Debug, Clone)]
pub enum HirDecl {
    Fn(FnDecl),
    Class(ClassDecl),
    Interface(InterfaceDecl),
    Object(ObjectDecl),
    Driver(DriverDecl),
    Import(ImportDecl),
    Extern(ExternDecl),
    /// A bare top-level statement — collected into a synthetic `fn main()` by codegen.
    Stmt(Stmt),
}

pub fn lower(prog: &Program) -> HirProgram {
    // Pass-through today; reserved for future analysis passes.
    HirProgram {
        decls: prog.decls.iter().map(|d| match d {
            Decl::Fn(f) => HirDecl::Fn(f.clone()),
            Decl::Class(c) => HirDecl::Class(c.clone()),
            Decl::Interface(i) => HirDecl::Interface(i.clone()),
            Decl::Object(o) => HirDecl::Object(o.clone()),
            Decl::Driver(v) => HirDecl::Driver(v.clone()),
            Decl::Import(i) => HirDecl::Import(i.clone()),
            Decl::Extern(e) => HirDecl::Extern(e.clone()),
            Decl::Stmt(s) => HirDecl::Stmt(s.clone()),
        }).collect(),
    }
}
