//! agex code generator — emits idiomatic Rust from the HIR.

use crate::ast::*;
use crate::hir::{HirDecl, HirProgram};

pub struct GenResult {
    pub rust: String,
    pub warnings: Vec<String>,
}

struct Generator {
    out: String,
    indent: usize,
    warnings: Vec<String>,
    /// When true, bare identifiers that match class field names should be
    /// prefixed with `self.`. Set when generating a method body.
    in_method_fields: Vec<String>,
    /// Map of sealed class name -> list of (variant_name, field_names).
    /// Used to translate `Damage.Physical(50.0)` into `Damage::Physical { amount: 50.0 }`.
    sealed_classes: Vec<(String, Vec<(String, Vec<String>)>)>,
}

const AGEX_TYPE_MAP: &[(&str, &str)] = &[
    ("int", "i32"), ("i8", "i8"), ("i16", "i16"), ("i32", "i32"), ("i64", "i64"),
    ("u8", "u8"), ("u16", "u16"), ("u32", "u32"), ("u64", "u64"),
    ("usize", "usize"), ("isize", "isize"),
    ("f32", "f32"), ("f64", "f64"),
    ("String", "String"), ("string", "String"), ("str", "&str"),
    ("bool", "bool"), ("char", "char"),
    ("Any", "impl std::any::Any"),
    ("Result", "Result"), ("Option", "Option"), ("Vec", "Vec"),
];

impl Generator {
    fn new() -> Self { Self { out: String::new(), indent: 0, warnings: Vec::new(), in_method_fields: Vec::new(), sealed_classes: Vec::new() } }

    fn emit(&mut self, line: &str) {
        if line.is_empty() {
            self.out.push('\n');
        } else {
            for _ in 0..self.indent { self.out.push_str("    "); }
            self.out.push_str(line);
            self.out.push('\n');
        }
    }

    fn generate(&mut self, prog: &HirProgram) -> GenResult {
        // Separate top-level statements from real declarations.
        let mut top_stmts: Vec<Stmt> = Vec::new();
        let mut decls: Vec<&HirDecl> = Vec::new();
        for d in &prog.decls {
            if let HirDecl::Stmt(s) = d { top_stmts.push(s.clone()); } else { decls.push(d); }
        }

        self.emit("// Generated from agex source by the agex compiler (agc)");
        self.emit("#![allow(dead_code, unused_variables, unused_imports, unused_mut)]");
        self.emit("");
        self.emit("use std::prelude::v1::*;");
        self.emit("");
        for d in decls {
            self.gen_decl(d);
            self.emit("");
        }
        if !top_stmts.is_empty() {
            self.emit("fn main() {");
            self.indent += 1;
            for s in &top_stmts { self.gen_stmt(s); }
            self.indent -= 1;
            self.emit("}");
        }
        GenResult { rust: self.out.clone(), warnings: std::mem::take(&mut self.warnings) }
    }

    fn gen_decl(&mut self, d: &HirDecl) {
        match d {
            HirDecl::Import(i) => {
                self.emit(&format!("// import {}", i.module));
                if matches!(i.module.as_str(), "graphics" | "network" | "filesystem") {
                    self.emit(&format!("mod {};", i.module));
                }
            }
            HirDecl::Extern(e) => self.gen_extern(e),
            HirDecl::Fn(f) => self.gen_fn(f),
            HirDecl::Class(c) => self.gen_class(c),
            HirDecl::Interface(i) => self.gen_interface(i),
            HirDecl::Object(o) => self.gen_object(o),
            HirDecl::Driver(v) => self.gen_driver(v),
            HirDecl::Stmt(_) => {} // handled separately
        }
    }

    fn gen_extern(&mut self, e: &ExternDecl) {
        self.emit(&format!("extern \"{}\" {{", e.abi));
        self.indent += 1;
        for f in &e.fns {
            let params = f.params.iter().map(|p| format!("{}: {}", p.name, self.gen_type(&p.ty))).collect::<Vec<_>>().join(", ");
            let ret = if matches!(f.return_type, TypeNode::Infer { .. }) { String::new() } else { format!(" -> {}", self.gen_type(&f.return_type)) };
            self.emit(&format!("fn {}({}){};", f.name, params, ret));
        }
        self.indent -= 1;
        self.emit("}");
    }

    fn gen_fn(&mut self, f: &FnDecl) {
        self.gen_fn_with_self(f, false)
    }

    fn gen_fn_with_self(&mut self, f: &FnDecl, is_method: bool) {
        self.gen_fn_inner(f, is_method, &[]);
    }

    /// Generate a function. When `is_method` is true, `fields` lists the class's
    /// field names so that bare identifier references can be rewritten to
    /// `self.<field>`.
    fn gen_fn_inner(&mut self, f: &FnDecl, is_method: bool, fields: &[String]) {
        let ret = if matches!(f.return_type, TypeNode::Infer { .. }) { String::new() } else { format!(" -> {}", self.gen_type(&f.return_type)) };
        let params_str = self.gen_params(&f.params, false);
        if let Some(recv) = &f.receiver {
            // Extension function: `fn String.shout() -> String { ... }`
            self.emit(&format!("impl {} {{", recv));
            self.indent += 1;
            let self_param = if f.params.is_empty() { "self: Self".to_string() } else { format!("self: &Self, {}", params_str) };
            self.emit(&format!("pub fn {}({}){} {{", f.name, self_param, ret));
        } else if is_method {
            let needs_mut = Self::method_needs_mut(f);
            let self_param = if needs_mut { "&mut self" } else { "&self" };
            let qualifier = if f.is_unsafe { "unsafe " } else { "" };
            let all_params = if f.params.is_empty() { self_param.to_string() } else { format!("{}, {}", self_param, params_str) };
            self.emit(&format!("{}pub fn {}({}){} {{", qualifier, f.name, all_params, ret));
            // Push field scope
            self.in_method_fields = fields.to_vec();
        } else {
            let qualifier = if f.is_unsafe { "unsafe " } else { "" };
            self.emit(&format!("{}pub fn {}({}){} {{", qualifier, f.name, params_str, ret));
        }
        self.indent += 1;
        if let Some(expr) = &f.expr_body { self.emit(&self.gen_expr(expr)); }
        if let Some(body) = &f.body { for s in body { self.gen_stmt(s); } }
        self.indent -= 1;
        self.emit("}");
        if f.receiver.is_some() { self.indent -= 1; self.emit("}"); }
        if is_method {
            // Pop field scope
            self.in_method_fields.clear();
        }
    }

    /// Heuristic: scan the method body for assignments to fields (which require &mut self).
    fn method_needs_mut(f: &FnDecl) -> bool {
        if let Some(body) = &f.body {
            for s in body {
                if Self::stmt_mutates_self(s) { return true; }
            }
        }
        false
    }

    fn stmt_mutates_self(s: &Stmt) -> bool {
        match s {
            Stmt::Assign { target, .. } => {
                matches!(target, Expr::Field { .. } | Expr::Ident { .. })
            }
            Stmt::If { then, els, .. } => {
                then.iter().any(|s| Self::stmt_mutates_self(s))
                    || els.as_ref().map_or(false, |els| els.iter().any(|s| Self::stmt_mutates_self(s)))
            }
            Stmt::Match { cases, .. } => {
                cases.iter().any(|c| c.body.iter().any(|s| Self::stmt_mutates_self(s)))
            }
            Stmt::Unsafe { body } => body.iter().any(|s| Self::stmt_mutates_self(s)),
            _ => false,
        }
    }

    fn gen_params(&self, params: &[Param], is_extension: bool) -> String {
        let mut out = Vec::new();
        if is_extension { out.push("&self".into()); }
        for p in params {
            let prefix = if p.mutable { "mut " } else { "" };
            out.push(format!("{}{}: {}", prefix, p.name, self.gen_type(&p.ty)));
        }
        out.join(", ")
    }

    fn gen_class(&mut self, c: &ClassDecl) {
        if c.is_sealed && !c.subclasses.is_empty() {
            // Register the sealed class so we can translate `Damage.Physical(x)` → `Damage::Physical { amount: x }`
            let variants: Vec<(String, Vec<String>)> = c.subclasses.iter()
                .map(|sub| (sub.name.clone(), sub.params.iter().map(|p| p.name.clone()).collect()))
                .collect();
            self.sealed_classes.push((c.name.clone(), variants));

            self.emit(&format!("pub enum {} {{", c.name));
            self.indent += 1;
            for sub in &c.subclasses {
                let fields = sub.params.iter().map(|p| format!("{}: {}", p.name, self.gen_type(&p.ty))).collect::<Vec<_>>().join(", ");
                self.emit(&format!("{} {{ {} }},", sub.name, fields));
            }
            self.indent -= 1;
            self.emit("}");
            self.emit(&format!("impl std::fmt::Display for {} {{", c.name));
            self.indent += 1;
            self.emit("fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {");
            self.indent += 1;
            self.emit("match self {");
            self.indent += 1;
            for sub in &c.subclasses {
                let names = sub.params.iter().map(|p| p.name.as_str()).collect::<Vec<_>>().join(", ");
                let parts = sub.params.iter().map(|p| format!("${{{}}}", p.name)).collect::<Vec<_>>().join(", ");
                self.emit(&format!("{}::{} {{ {} }} => write!(f, \"{}(\" + {}.to_string() + \")\"),", c.name, sub.name, names, sub.name, parts));
            }
            self.indent -= 1;
            self.emit("}");
            self.indent -= 1;
            self.emit("}");
            self.indent -= 1;
            self.emit("}");
            return;
        }
        if c.is_data {
            self.emit("#[derive(Debug, Clone, PartialEq)]");
            self.emit(&format!("pub struct {} {{", c.name));
            self.indent += 1;
            for p in &c.params { self.emit(&format!("pub {}: {},", p.name, self.gen_type(&p.ty))); }
            self.indent -= 1;
            self.emit("}");
            self.emit(&format!("impl {} {{", c.name));
            self.indent += 1;
            let ctor_params = c.params.iter().map(|p| format!("{}: {}", p.name, self.gen_type(&p.ty))).collect::<Vec<_>>().join(", ");
            self.emit(&format!("pub fn new({}) -> Self {{", ctor_params));
            self.indent += 1;
            let inits = c.params.iter().map(|p| p.name.as_str()).collect::<Vec<_>>().join(", ");
            self.emit(&format!("Self {{ {} }}", inits));
            self.indent -= 1;
            self.emit("}");
            for (i, p) in c.params.iter().enumerate() {
                self.emit(&format!("pub fn component_{}(&self) -> &{} {{ &self.{} }}", i + 1, self.gen_type(&p.ty), p.name));
            }
            let fields: Vec<String> = c.params.iter().map(|p| p.name.clone()).collect();
            for m in &c.members { self.gen_fn_inner(m, true, &fields); }
            self.indent -= 1;
            self.emit("}");
            self.emit(&format!("impl std::fmt::Display for {} {{", c.name));
            self.indent += 1;
            self.emit("fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {");
            self.indent += 1;
            let parts = c.params.iter().map(|p| format!("${{self.{}}}", p.name)).collect::<Vec<_>>().join(", ");
            self.emit(&format!("write!(f, \"{}(\" + \"{}\".to_string() + \")\")", c.name, parts));
            self.indent -= 1;
            self.emit("}");
            self.indent -= 1;
            self.emit("}");
            return;
        }
        // Regular class
        self.emit(&format!("pub struct {} {{", c.name));
        self.indent += 1;
        for p in &c.params { self.emit(&format!("pub {}: {},", p.name, self.gen_type(&p.ty))); }
        self.indent -= 1;
        self.emit("}");
        self.emit(&format!("impl {} {{", c.name));
        self.indent += 1;
        let ctor_params = c.params.iter().map(|p| format!("{}: {}", p.name, self.gen_type(&p.ty))).collect::<Vec<_>>().join(", ");
        self.emit(&format!("pub fn new({}) -> Self {{", ctor_params));
        self.indent += 1;
        let inits = c.params.iter().map(|p| p.name.as_str()).collect::<Vec<_>>().join(", ");
        self.emit(&format!("Self {{ {} }}", inits));
        self.indent -= 1;
        self.emit("}");
        let fields: Vec<String> = c.params.iter().map(|p| p.name.clone()).collect();
        for m in &c.members { self.gen_fn_inner(m, true, &fields); }
        self.indent -= 1;
        self.emit("}");
    }

    fn gen_interface(&mut self, i: &InterfaceDecl) {
        self.emit(&format!("pub trait {} {{", i.name));
        self.indent += 1;
        for m in &i.methods {
            let ret = if matches!(m.return_type, TypeNode::Infer { .. }) { String::new() } else { format!(" -> {}", self.gen_type(&m.return_type)) };
            let params = m.params.iter().map(|p| format!("{}: {}", p.name, self.gen_type(&p.ty))).collect::<Vec<_>>().join(", ");
            if m.body.is_some() || m.expr_body.is_some() {
                self.emit(&format!("fn {}({}){} {{", m.name, params, ret));
                self.indent += 1;
                if let Some(expr) = &m.expr_body { self.emit(&self.gen_expr(expr)); }
                if let Some(body) = &m.body { for s in body { self.gen_stmt(s); } }
                self.indent -= 1;
                self.emit("}");
            } else {
                self.emit(&format!("fn {}({}){};", m.name, params, ret));
            }
        }
        self.indent -= 1;
        self.emit("}");
    }

    fn gen_object(&mut self, o: &ObjectDecl) {
        self.emit(&format!("pub struct {};", o.name));
        self.emit(&format!("impl {} {{", o.name));
        self.indent += 1;
        self.emit(&format!("pub fn instance() -> &'static {} {{", o.name));
        self.indent += 1;
        self.emit("use std::sync::OnceLock;");
        self.emit(&format!("static INSTANCE: OnceLock<{}> = OnceLock::new();", o.name));
        self.emit(&format!("INSTANCE.get_or_init(|| {})", o.name));
        self.indent -= 1;
        self.emit("}");
        for m in &o.members {
            let ret = if matches!(m.return_type, TypeNode::Infer { .. }) { String::new() } else { format!(" -> {}", self.gen_type(&m.return_type)) };
            let params = m.params.iter().map(|p| format!("{}: {}", p.name, self.gen_type(&p.ty))).collect::<Vec<_>>().join(", ");
            self.emit(&format!("pub fn {}({}){} {{", m.name, params, ret));
            self.indent += 1;
            if let Some(expr) = &m.expr_body { self.emit(&self.gen_expr(expr)); }
            if let Some(body) = &m.body { for s in body { self.gen_stmt(s); } }
            self.indent -= 1;
            self.emit("}");
        }
        self.indent -= 1;
        self.emit("}");
    }

    fn gen_driver(&mut self, v: &DriverDecl) {
        self.emit(&format!("pub struct {};", v.name));
        self.emit(&format!("impl {} for {} {{", v.implements, v.name));
        self.indent += 1;
        for m in &v.members {
            let ret = if matches!(m.return_type, TypeNode::Infer { .. }) { String::new() } else { format!(" -> {}", self.gen_type(&m.return_type)) };
            let params = m.params.iter().map(|p| format!("{}: {}", p.name, self.gen_type(&p.ty))).collect::<Vec<_>>().join(", ");
            self.emit(&format!("fn {}({}){} {{", m.name, params, ret));
            self.indent += 1;
            if let Some(expr) = &m.expr_body { self.emit(&self.gen_expr(expr)); }
            if let Some(body) = &m.body { for s in body { self.gen_stmt(s); } }
            else { self.emit("// hardware setup"); }
            self.indent -= 1;
            self.emit("}");
        }
        self.indent -= 1;
        self.emit("}");
    }

    fn gen_type(&self, t: &TypeNode) -> String {
        match t {
            TypeNode::Infer { nullable } => if *nullable { "Option<impl std::any::Any>".into() } else { "_".into() },
            TypeNode::Named { name, nullable } => {
                let base = AGEX_TYPE_MAP.iter().find(|(k, _)| *k == name.as_str()).map(|(_, v)| *v).unwrap_or(name);
                if *nullable { format!("Option<{}>", base) } else { base.into() }
            }
            TypeNode::Generic { name, args, nullable } => {
                let base = AGEX_TYPE_MAP.iter().find(|(k, _)| *k == name.as_str()).map(|(_, v)| *v).unwrap_or(name);
                let args = args.iter().map(|a| self.gen_type(a)).collect::<Vec<_>>().join(", ");
                let rust = format!("{}<{}>", base, args);
                if *nullable { format!("Option<{}>", rust) } else { rust }
            }
            TypeNode::Pointer { inner, nullable } => {
                let rust = format!("*mut {}", self.gen_type(inner));
                if *nullable { format!("Option<{}>", rust) } else { rust }
            }
        }
    }

    fn gen_stmt(&mut self, s: &Stmt) {
        match s {
            Stmt::Let { name, ty, init } => {
                if let Some(init) = init {
                    if matches!(ty, TypeNode::Infer { .. }) { self.emit(&format!("let {} = {};", name, self.gen_expr(init))); }
                    else { self.emit(&format!("let {}: {} = {};", name, self.gen_type(ty), self.gen_expr(init))); }
                } else { self.emit(&format!("let {}: {};", name, self.gen_type(ty))); }
            }
            Stmt::Const { name, ty, init } => {
                let init = init.as_ref().map(|e| self.gen_expr(e)).unwrap_or_default();
                if matches!(ty, TypeNode::Infer { .. }) {
                    // For inferred const, we need an explicit type. Use &str for string literals,
                    // otherwise let rustc infer via a `let` instead.
                    self.emit(&format!("let {}: _ = {};", name, init));
                }
                else { self.emit(&format!("const {}: {} = {};", name, self.gen_type(ty), init)); }
            }
            Stmt::Var { name, ty, init } => {
                if let Some(init) = init {
                    if matches!(ty, TypeNode::Infer { .. }) { self.emit(&format!("let mut {} = {};", name, self.gen_expr(init))); }
                    else { self.emit(&format!("let mut {}: {} = {};", name, self.gen_type(ty), self.gen_expr(init))); }
                } else { self.emit(&format!("let mut {}: {};", name, self.gen_type(ty))); }
            }
            Stmt::Assign { target, op, value } => {
                let op_str = match op {
                    AssignOp::Assign => "=", AssignOp::PlusEq => "+=",
                    AssignOp::MinusEq => "-=", AssignOp::StarEq => "*=", AssignOp::SlashEq => "/=",
                };
                self.emit(&format!("{} {} {};", self.gen_expr(target), op_str, self.gen_expr(value)));
            }
            Stmt::Return { value } => {
                if let Some(v) = value { self.emit(&format!("return {};", self.gen_expr(v))); }
                else { self.emit("return;"); }
            }
            Stmt::If { cond, then, els } => self.gen_if(cond, then, els),
            Stmt::For { var, iter, body } => self.gen_for(var, iter, body),
            Stmt::Match { expr, cases } => self.gen_match(expr, cases),
            Stmt::Expr { expr } => self.emit(&format!("{};", self.gen_expr(expr))),
            Stmt::Print { args } => {
                if args.is_empty() {
                    self.emit("println!();");
                } else {
                    // Build a format string: each arg becomes either a literal string
                    // (concatenated directly) or a {} placeholder.
                    let mut fmt = String::new();
                    let mut rest: Vec<String> = Vec::new();
                    let mut first = true;
                    for a in args {
                        if !first { fmt.push(' '); }
                        first = false;
                        if let Expr::String { value } = a {
                            fmt.push_str(value);
                        } else {
                            fmt.push_str("{}");
                            rest.push(self.gen_expr(a));
                        }
                    }
                    if rest.is_empty() {
                        self.emit(&format!("println!(\"{}\");", fmt));
                    } else {
                        self.emit(&format!("println!(\"{}\", {});", fmt, rest.join(", ")));
                    }
                }
            }
            Stmt::Unsafe { body } => {
                self.emit("unsafe {");
                self.indent += 1;
                for s in body { self.gen_stmt(s); }
                self.indent -= 1;
                self.emit("}");
            }
        }
    }

    fn args_to_format(&self, args: &[Expr]) -> String {
        let mut parts = Vec::new();
        let mut rest = Vec::new();
        if let Expr::String { value } = &args[0] {
            parts.push(format!("\"{}\"", value));
        } else {
            parts.push("\"{}\"".into());
            rest.push(self.gen_expr(&args[0]));
        }
        for arg in &args[1..] { rest.push(self.gen_expr(arg)); }
        let mut out = parts[0].clone();
        if !rest.is_empty() { out.push_str(", "); out.push_str(&rest.join(", ")); }
        out
    }

    fn gen_if(&mut self, cond: &Expr, then: &[Stmt], els: &Option<Vec<Stmt>>) {
        self.emit(&format!("if {} {{", self.gen_expr(cond)));
        self.indent += 1;
        for s in then { self.gen_stmt(s); }
        self.indent -= 1;
        if let Some(els) = els {
            self.emit("} else {");
            self.indent += 1;
            for s in els { self.gen_stmt(s); }
            self.indent -= 1;
        }
        self.emit("}");
    }

    fn gen_for(&mut self, var: &str, iter: &Expr, body: &[Stmt]) {
        if let Expr::Call { callee, args } = iter {
            if let Expr::Ident { name } = callee.as_ref() {
                if name == "range" {
                    let n = args.first().map(|a| self.gen_expr(a)).unwrap_or_else(|| "0".into());
                    self.emit(&format!("for {} in 0..{} {{", var, n));
                    self.indent += 1;
                    for s in body { self.gen_stmt(s); }
                    self.indent -= 1;
                    self.emit("}");
                    return;
                }
            }
        }
        self.emit(&format!("for {} in {} {{", var, self.gen_expr(iter)));
        self.indent += 1;
        for s in body { self.gen_stmt(s); }
        self.indent -= 1;
        self.emit("}");
    }

    fn gen_match(&mut self, expr: &Expr, cases: &[MatchCase]) {
        self.emit(&format!("match {} {{", self.gen_expr(expr)));
        self.indent += 1;
        for c in cases {
            self.emit(&format!("{} => {{", self.format_case(c)));
            self.indent += 1;
            for s in &c.body { self.gen_stmt(s); }
            self.indent -= 1;
            self.emit("}");
        }
        if !cases.iter().any(|c| c.pattern == "_") { self.emit("_ => {}"); }
        self.indent -= 1;
        self.emit("}");
    }

    fn format_case(&self, c: &MatchCase) -> String {
        if c.pattern == "_" { return "_".into(); }
        if c.pattern == "Ok" || c.pattern == "Err" {
            if c.bindings.len() == 1 { return format!("{}({})", c.pattern, c.bindings[0]); }
            return format!("{}(..)", c.pattern);
        }
        if c.pattern.contains('.') {
            let mut parts = c.pattern.splitn(2, '.');
            let enum_name = parts.next().unwrap();
            let variant = parts.next().unwrap();
            if !c.bindings.is_empty() {
                return format!("{}::{} {{ {} }}", enum_name, variant, c.bindings.join(", "));
            }
            return format!("{}::{} {{ .. }}", enum_name, variant);
        }
        c.pattern.clone()
    }

    fn gen_expr(&self, e: &Expr) -> String {
        match e {
            Expr::Int { value } => value.clone(),
            Expr::Float { value } => value.clone(),
            Expr::String { value } => format!("\"{}\"", value),
            Expr::Bool { value } => value.to_string(),
            Expr::Null => "None".into(),
            Expr::This => "self".into(),
            Expr::Ident { name } => {
                // Inside a class method, bare identifiers matching a field name
                // are field accesses on self.
                if self.in_method_fields.iter().any(|f| f == name) {
                    format!("self.{}", name)
                } else {
                    name.clone()
                }
            }
            Expr::Binary { op, left, right } => {
                // String concatenation: when + is applied to a string literal,
                // emit format!("{}{}", ...) so &str + &str works.
                if op == "+" && (matches!(left.as_ref(), Expr::String { .. }) || matches!(right.as_ref(), Expr::String { .. })) {
                    format!("format!(\"{{}}{{}}\", {}, {})", self.gen_expr(left), self.gen_expr(right))
                } else {
                    format!("({} {} {})", self.gen_expr(left), op, self.gen_expr(right))
                }
            }
            Expr::Unary { op, operand } => {
                if op == "&" { format!("&{}", self.gen_expr(operand)) }
                else { format!("({}{})", op, self.gen_expr(operand)) }
            }
            Expr::Call { callee, args } => {
                if let Expr::Ident { name } = callee.as_ref() {
                    if name == "range" {
                        let arg = args.first().map(|a| self.gen_expr(a)).unwrap_or_else(|| "0".into());
                        return format!("(0..{})", arg);
                    }
                    if name == "println" {
                        if args.is_empty() { return "println!()".into(); }
                        let fmt = args.iter().map(|a| if let Expr::String { value } = a { value.clone() } else { "{}".into() }).collect::<Vec<_>>().join(" ");
                        let rest = args.iter().filter(|a| !matches!(a, Expr::String { .. })).map(|a| self.gen_expr(a)).collect::<Vec<_>>();
                        if rest.is_empty() { return format!("println!(\"{}\")", fmt); }
                        return format!("println!(\"{}\", {})", fmt, rest.join(", "));
                    }
                }
                let args = args.iter().map(|a| self.gen_expr(a)).collect::<Vec<_>>().join(", ");
                format!("{}({})", self.gen_expr(callee), args)
            }
            Expr::Method { recv, name, args } => {
                // Check if this is a sealed-class variant constructor: `Damage.Physical(50.0)`
                if let Expr::Ident { name: enum_name } = recv.as_ref() {
                    for (sealed_name, variants) in &self.sealed_classes {
                        if sealed_name == enum_name {
                            for (variant_name, field_names) in variants {
                                if variant_name == name {
                                    // `Damage::Physical { amount: 50.0 }`
                                    let pairs = args.iter().enumerate()
                                        .map(|(i, a)| format!("{}: {}", field_names.get(i).cloned().unwrap_or_else(|| format!("field{}", i)), self.gen_expr(a)))
                                        .collect::<Vec<_>>()
                                        .join(", ");
                                    return format!("{}::{} {{ {} }}", enum_name, name, pairs);
                                }
                            }
                        }
                    }
                }
                let recv = self.gen_expr(recv);
                let args = args.iter().map(|a| self.gen_expr(a)).collect::<Vec<_>>().join(", ");
                if name == "uppercase" { return format!("{}.to_uppercase()", recv); }
                if name == "lowercase" { return format!("{}.to_lowercase()", recv); }
                if name == "length" || name == "len" { return format!("{}.len()", recv); }
                format!("{}.{}({})", recv, name, args)
            }
            Expr::Field { recv, name } => {
                let recv = self.gen_expr(recv);
                if name == "length" || name == "len" { return format!("{}.len()", recv); }
                format!("{}.{}", recv, name)
            }
            Expr::SafeCall { recv, name, args } => {
                let recv = self.gen_expr(recv);
                let args = args.iter().map(|a| self.gen_expr(a)).collect::<Vec<_>>().join(", ");
                format!("{}.as_ref().and_then(|r| Some(r.{}({})))", recv, name, args)
            }
            Expr::Elvis { left, right } => format!("{}.unwrap_or({})", self.gen_expr(left), self.gen_expr(right)),
            Expr::New { type_name, args } => {
                // Wrap string literals in .to_string() so they convert to String params
                let args = args.iter().map(|a| {
                    let s = self.gen_expr(a);
                    if matches!(a, Expr::String { .. }) { format!("{}.to_string()", s) } else { s }
                }).collect::<Vec<_>>().join(", ");
                format!("{}::new({})", type_name, args)
            }
            Expr::Cast { expr, type_name } => format!("{} as {}", self.gen_expr(expr), type_name),
        }
    }
}

pub fn generate(prog: &HirProgram) -> GenResult {
    Generator::new().generate(prog)
}
