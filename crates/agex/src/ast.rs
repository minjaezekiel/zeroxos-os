//! agex AST (Abstract Syntax Tree) definitions.

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub enum TypeNode {
    Infer { nullable: bool },
    Named { name: String, nullable: bool },
    Generic { name: String, args: Vec<TypeNode>, nullable: bool },
    Pointer { inner: Box<TypeNode>, nullable: bool },
}

impl TypeNode {
    pub fn infer() -> Self { TypeNode::Infer { nullable: false } }
}

#[derive(Debug, Clone, Serialize)]
pub struct Param {
    pub name: String,
    pub ty: TypeNode,
    pub mutable: bool,
    pub default: Option<Expr>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FnDecl {
    pub name: String,
    pub params: Vec<Param>,
    pub return_type: TypeNode,
    pub body: Option<Vec<Stmt>>,
    pub expr_body: Option<Expr>,
    pub is_async: bool,
    pub is_unsafe: bool,
    pub receiver: Option<String>, // for extension fns
}

#[derive(Debug, Clone, Serialize)]
pub struct ClassDecl {
    pub name: String,
    pub is_data: bool,
    pub is_sealed: bool,
    pub params: Vec<Param>,
    pub bases: Vec<String>,
    pub members: Vec<FnDecl>,
    pub subclasses: Vec<ClassDecl>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InterfaceDecl {
    pub name: String,
    pub methods: Vec<FnDecl>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ObjectDecl {
    pub name: String,
    pub members: Vec<FnDecl>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DriverDecl {
    pub name: String,
    pub implements: String,
    pub members: Vec<FnDecl>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImportDecl {
    pub module: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExternDecl {
    pub abi: String,
    pub fns: Vec<ExternFn>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExternFn {
    pub name: String,
    pub params: Vec<Param>,
    pub return_type: TypeNode,
}

#[derive(Debug, Clone, Serialize)]
pub enum Decl {
    Fn(FnDecl),
    Class(ClassDecl),
    Interface(InterfaceDecl),
    Object(ObjectDecl),
    Driver(DriverDecl),
    Import(ImportDecl),
    Extern(ExternDecl),
    Stmt(Stmt),
}

#[derive(Debug, Clone, Serialize)]
pub struct Program {
    pub decls: Vec<Decl>,
}

// Statements
#[derive(Debug, Clone, Serialize)]
pub enum Stmt {
    Let { name: String, ty: TypeNode, init: Option<Expr> },
    Const { name: String, ty: TypeNode, init: Option<Expr> },
    Var { name: String, ty: TypeNode, init: Option<Expr> },
    Assign { target: Expr, op: AssignOp, value: Expr },
    Return { value: Option<Expr> },
    If { cond: Expr, then: Vec<Stmt>, els: Option<Vec<Stmt>> },
    For { var: String, iter: Expr, body: Vec<Stmt> },
    Match { expr: Expr, cases: Vec<MatchCase> },
    Print { args: Vec<Expr> },
    Unsafe { body: Vec<Stmt> },
    Expr { expr: Expr },
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub enum AssignOp {
    Assign, PlusEq, MinusEq, StarEq, SlashEq,
}

#[derive(Debug, Clone, Serialize)]
pub struct MatchCase {
    pub pattern: String,
    pub bindings: Vec<String>,
    pub body: Vec<Stmt>,
}

// Expressions
#[derive(Debug, Clone, Serialize)]
pub enum Expr {
    Int { value: String },
    Float { value: String },
    String { value: String },
    Bool { value: bool },
    Null,
    This,
    Ident { name: String },
    Binary { op: String, left: Box<Expr>, right: Box<Expr> },
    Unary { op: String, operand: Box<Expr> },
    Call { callee: Box<Expr>, args: Vec<Expr> },
    Method { recv: Box<Expr>, name: String, args: Vec<Expr> },
    Field { recv: Box<Expr>, name: String },
    SafeCall { recv: Box<Expr>, name: String, args: Vec<Expr> },
    Elvis { left: Box<Expr>, right: Box<Expr> },
    New { type_name: String, args: Vec<Expr> },
    Cast { expr: Box<Expr>, type_name: String },
}
