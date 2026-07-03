//! agex parser — recursive descent, builds an AST from tokens.

use crate::ast::*;
use crate::lexer::{tokenize, LexerError, Token, TokenType};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ParseError {
    #[error("lexer error: {0}")]
    Lexer(#[from] LexerError),
    #[error("parse error at {line}:{col}: {msg} (got {got})")]
    Unexpected { line: usize, col: usize, msg: String, got: String },
}

struct Parser {
    toks: Vec<Token>,
    pos: usize,
}

impl Parser {
    fn new(toks: Vec<Token>) -> Self { Self { toks, pos: 0 } }
    fn peek(&self, off: usize) -> &Token { self.toks.get(self.pos + off).unwrap_or(self.toks.last().unwrap()) }
    fn cur(&self) -> &Token { &self.toks[self.pos] }
    fn at(&self, ty: TokenType) -> bool { self.cur().ty == ty }
    fn eat(&mut self, ty: TokenType) -> Result<Token, ParseError> {
        if !self.at(ty) {
            let t = self.cur().clone();
            return Err(ParseError::Unexpected {
                line: t.line, col: t.col,
                msg: format!("expected {:?}", ty),
                got: format!("{:?} '{}'", t.ty, t.value),
            });
        }
        let t = self.toks[self.pos].clone();
        self.pos += 1;
        Ok(t)
    }
    fn maybe(&mut self, ty: TokenType) -> Option<Token> {
        if self.at(ty) { let t = self.toks[self.pos].clone(); self.pos += 1; Some(t) } else { None }
    }

    fn parse_program(&mut self) -> Result<Program, ParseError> {
        let mut decls = Vec::new();
        while !self.at(TokenType::Eof) {
            decls.push(self.parse_decl()?);
        }
        Ok(Program { decls })
    }

    fn parse_decl(&mut self) -> Result<Decl, ParseError> {
        if self.at(TokenType::Import) { return Ok(Decl::Import(self.parse_import()?)); }
        if self.at(TokenType::Extern) { return Ok(Decl::Extern(self.parse_extern()?)); }
        if self.at(TokenType::Async) || self.at(TokenType::Fn) { return Ok(Decl::Fn(self.parse_fn(false, false)?)); }
        if self.at(TokenType::Unsafe) && self.peek(1).ty == TokenType::Fn {
            self.eat(TokenType::Unsafe)?;
            return Ok(Decl::Fn(self.parse_fn(false, true)?));
        }
        if self.at(TokenType::Interrupt) { return self.parse_interrupt_as_stmt().map(Decl::Stmt); }
        if self.at(TokenType::Data) {
            self.eat(TokenType::Data)?; self.eat(TokenType::Class)?;
            return Ok(Decl::Class(self.parse_class(true, false)?));
        }
        if self.at(TokenType::Sealed) {
            self.eat(TokenType::Sealed)?; self.eat(TokenType::Class)?;
            return Ok(Decl::Class(self.parse_class(false, true)?));
        }
        if self.at(TokenType::Class) {
            self.eat(TokenType::Class)?;
            return Ok(Decl::Class(self.parse_class(false, false)?));
        }
        if self.at(TokenType::Interface) { return Ok(Decl::Interface(self.parse_interface()?)); }
        if self.at(TokenType::Object) { return Ok(Decl::Object(self.parse_object()?)); }
        if self.at(TokenType::Driver) { return Ok(Decl::Driver(self.parse_driver()?)); }
        // Bare top-level statement
        let stmt = self.parse_stmt()?;
        Ok(Decl::Stmt(stmt))
    }

    fn parse_import(&mut self) -> Result<ImportDecl, ParseError> {
        self.eat(TokenType::Import)?;
        let module = self.eat(TokenType::Ident)?.value;
        Ok(ImportDecl { module })
    }

    fn parse_extern(&mut self) -> Result<ExternDecl, ParseError> {
        self.eat(TokenType::Extern)?;
        let abi = self.maybe(TokenType::StringLiteral).map(|t| t.value).unwrap_or_else(|| "Rust".into());
        self.eat(TokenType::LBrace)?;
        let mut fns = Vec::new();
        while !self.at(TokenType::RBrace) {
            self.eat(TokenType::Fn)?;
            let name = self.eat(TokenType::Ident)?.value;
            self.eat(TokenType::LParen)?;
            let params = self.parse_params()?;
            self.eat(TokenType::RParen)?;
            let mut return_type = TypeNode::infer();
            if self.maybe(TokenType::Arrow).is_some() { return_type = self.parse_type()?; }
            fns.push(ExternFn { name, params, return_type });
        }
        self.eat(TokenType::RBrace)?;
        Ok(ExternDecl { abi, fns })
    }

    fn parse_interrupt_as_stmt(&mut self) -> Result<Stmt, ParseError> {
        self.eat(TokenType::Interrupt)?;
        let _name = self.eat(TokenType::Ident)?.value;
        self.eat(TokenType::LParen)?;
        self.eat(TokenType::RParen)?;
        self.eat(TokenType::LBrace)?;
        let body = self.parse_stmts()?;
        self.eat(TokenType::RBrace)?;
        // Treat as an unsafe block (kernel handler)
        Ok(Stmt::Unsafe { body })
    }

    fn parse_fn(&mut self, is_async: bool, is_unsafe: bool) -> Result<FnDecl, ParseError> {
        if is_async { self.eat(TokenType::Async)?; }
        self.eat(TokenType::Fn)?;
        let mut receiver = None;
        if self.cur().ty == TokenType::Ident && self.peek(1).ty == TokenType::Dot {
            receiver = Some(self.eat(TokenType::Ident)?.value);
            self.eat(TokenType::Dot)?;
        }
        let name = self.eat(TokenType::Ident)?.value;
        self.eat(TokenType::LParen)?;
        let params = self.parse_params()?;
        self.eat(TokenType::RParen)?;
        let mut return_type = TypeNode::infer();
        if self.maybe(TokenType::Arrow).is_some() { return_type = self.parse_type()?; }

        let mut body = None;
        let mut expr_body = None;
        if self.at(TokenType::FatArrow) || self.at(TokenType::Assign) {
            self.eat(self.cur().ty.clone())?;
            expr_body = Some(self.parse_expr()?);
            let _ = self.maybe(TokenType::Semicolon);
        } else if self.at(TokenType::LBrace) {
            self.eat(TokenType::LBrace)?;
            body = Some(self.parse_stmts()?);
            self.eat(TokenType::RBrace)?;
        } else {
            let t = self.cur().clone();
            return Err(ParseError::Unexpected {
                line: t.line, col: t.col,
                msg: "expected function body or =>".into(),
                got: format!("{:?} '{}'", t.ty, t.value),
            });
        }
        Ok(FnDecl { name, params, return_type, body, expr_body, is_async, is_unsafe, receiver })
    }

    fn parse_params(&mut self) -> Result<Vec<Param>, ParseError> {
        let mut params = Vec::new();
        while !self.at(TokenType::RParen) {
            let mut mutable = false;
            if self.at(TokenType::Var) { mutable = true; self.eat(TokenType::Var)?; }
            let name = self.eat(TokenType::Ident)?.value;
            self.eat(TokenType::Colon)?;
            let ty = self.parse_type()?;
            let default = if self.maybe(TokenType::Assign).is_some() { Some(self.parse_expr()?) } else { None };
            params.push(Param { name, ty, mutable, default });
            if self.maybe(TokenType::Comma).is_none() { break; }
        }
        Ok(params)
    }

    fn parse_type(&mut self) -> Result<TypeNode, ParseError> {
        let mut t = if self.at(TokenType::Star) {
            self.eat(TokenType::Star)?;
            TypeNode::Pointer { inner: Box::new(self.parse_type()?), nullable: false }
        } else {
            let name = self.eat(TokenType::Ident)?.value;
            if self.maybe(TokenType::LBracket).is_some() {
                let mut args = Vec::new();
                if !self.at(TokenType::RBracket) {
                    args.push(self.parse_type()?);
                    while self.maybe(TokenType::Comma).is_some() { args.push(self.parse_type()?); }
                }
                self.eat(TokenType::RBracket)?;
                TypeNode::Generic { name, args, nullable: false }
            } else {
                TypeNode::Named { name, nullable: false }
            }
        };
        if self.maybe(TokenType::Question).is_some() {
            t = match t {
                TypeNode::Infer { .. } => TypeNode::Infer { nullable: true },
                TypeNode::Named { name, .. } => TypeNode::Named { name, nullable: true },
                TypeNode::Generic { name, args, .. } => TypeNode::Generic { name, args, nullable: true },
                TypeNode::Pointer { inner, .. } => TypeNode::Pointer { inner, nullable: true },
            };
        }
        Ok(t)
    }

    fn parse_class(&mut self, is_data: bool, is_sealed: bool) -> Result<ClassDecl, ParseError> {
        let name = self.eat(TokenType::Ident)?.value;
        let mut params = Vec::new();
        if self.maybe(TokenType::LParen).is_some() {
            params = self.parse_params()?;
            self.eat(TokenType::RParen)?;
        }
        let mut bases = Vec::new();
        if self.maybe(TokenType::Colon).is_some() {
            bases.push(self.eat(TokenType::Ident)?.value);
            self.skip_optional_parens();
            while self.maybe(TokenType::Comma).is_some() {
                bases.push(self.eat(TokenType::Ident)?.value);
                self.skip_optional_parens();
            }
        }
        let mut members = Vec::new();
        let mut subclasses = Vec::new();
        if is_sealed && self.at(TokenType::LBrace) && self.peek(1).ty == TokenType::Class {
            self.eat(TokenType::LBrace)?;
            while !self.at(TokenType::RBrace) {
                self.eat(TokenType::Class)?;
                subclasses.push(self.parse_class(false, false)?);
            }
            self.eat(TokenType::RBrace)?;
        } else if self.maybe(TokenType::LBrace).is_some() {
            while !self.at(TokenType::RBrace) {
                if self.at(TokenType::Fn) {
                    members.push(self.parse_fn(false, false)?);
                } else {
                    let t = self.cur().clone();
                    return Err(ParseError::Unexpected {
                        line: t.line, col: t.col,
                        msg: "expected fn inside class body".into(),
                        got: format!("{:?} '{}'", t.ty, t.value),
                    });
                }
            }
            self.eat(TokenType::RBrace)?;
        }
        Ok(ClassDecl { name, is_data, is_sealed, params, bases, members, subclasses })
    }

    fn skip_optional_parens(&mut self) {
        if self.at(TokenType::LParen) {
            self.eat(TokenType::LParen).ok();
            while !self.at(TokenType::RParen) && !self.at(TokenType::Eof) { self.pos += 1; }
            self.eat(TokenType::RParen).ok();
        }
    }

    fn parse_interface(&mut self) -> Result<InterfaceDecl, ParseError> {
        self.eat(TokenType::Interface)?;
        let name = self.eat(TokenType::Ident)?.value;
        self.eat(TokenType::LBrace)?;
        let mut methods = Vec::new();
        while !self.at(TokenType::RBrace) { methods.push(self.parse_fn(false, false)?); }
        self.eat(TokenType::RBrace)?;
        Ok(InterfaceDecl { name, methods })
    }

    fn parse_object(&mut self) -> Result<ObjectDecl, ParseError> {
        self.eat(TokenType::Object)?;
        let name = self.eat(TokenType::Ident)?.value;
        self.eat(TokenType::LBrace)?;
        let mut members = Vec::new();
        while !self.at(TokenType::RBrace) { members.push(self.parse_fn(false, false)?); }
        self.eat(TokenType::RBrace)?;
        Ok(ObjectDecl { name, members })
    }

    fn parse_driver(&mut self) -> Result<DriverDecl, ParseError> {
        self.eat(TokenType::Driver)?;
        let name = self.eat(TokenType::Ident)?.value;
        self.eat(TokenType::Colon)?;
        let implements = self.eat(TokenType::Ident)?.value;
        self.eat(TokenType::LBrace)?;
        let mut members = Vec::new();
        while !self.at(TokenType::RBrace) { members.push(self.parse_fn(false, false)?); }
        self.eat(TokenType::RBrace)?;
        Ok(DriverDecl { name, implements, members })
    }

    // Statements
    fn parse_stmts(&mut self) -> Result<Vec<Stmt>, ParseError> {
        let mut out = Vec::new();
        while !self.at(TokenType::RBrace) && !self.at(TokenType::Eof) {
            out.push(self.parse_stmt()?);
        }
        Ok(out)
    }

    fn parse_stmt(&mut self) -> Result<Stmt, ParseError> {
        match self.cur().ty {
            TokenType::Let => self.parse_let(),
            TokenType::Const => self.parse_const(),
            TokenType::Var => self.parse_var(),
            TokenType::Return => self.parse_return(),
            TokenType::If => self.parse_if(),
            TokenType::For => self.parse_for(),
            TokenType::Match => self.parse_match(),
            TokenType::Print => self.parse_print(),
            TokenType::Unsafe => self.parse_unsafe(),
            _ => self.parse_expr_or_assign(),
        }
    }

    fn parse_let(&mut self) -> Result<Stmt, ParseError> {
        self.eat(TokenType::Let)?;
        let name = self.eat(TokenType::Ident)?.value;
        let ty = if self.maybe(TokenType::Colon).is_some() { self.parse_type()? } else { TypeNode::infer() };
        let init = if self.maybe(TokenType::Assign).is_some() { Some(self.parse_expr()?) } else { None };
        Ok(Stmt::Let { name, ty, init })
    }
    fn parse_const(&mut self) -> Result<Stmt, ParseError> {
        self.eat(TokenType::Const)?;
        let name = self.eat(TokenType::Ident)?.value;
        let ty = if self.maybe(TokenType::Colon).is_some() { self.parse_type()? } else { TypeNode::infer() };
        let init = if self.maybe(TokenType::Assign).is_some() { Some(self.parse_expr()?) } else { None };
        Ok(Stmt::Const { name, ty, init })
    }
    fn parse_var(&mut self) -> Result<Stmt, ParseError> {
        self.eat(TokenType::Var)?;
        let name = self.eat(TokenType::Ident)?.value;
        let ty = if self.maybe(TokenType::Colon).is_some() { self.parse_type()? } else { TypeNode::infer() };
        let init = if self.maybe(TokenType::Assign).is_some() { Some(self.parse_expr()?) } else { None };
        Ok(Stmt::Var { name, ty, init })
    }
    fn parse_return(&mut self) -> Result<Stmt, ParseError> {
        self.eat(TokenType::Return)?;
        if self.at(TokenType::RBrace) || self.at(TokenType::Semicolon) || self.at(TokenType::Eof) {
            return Ok(Stmt::Return { value: None });
        }
        Ok(Stmt::Return { value: Some(self.parse_expr()?) })
    }
    fn parse_if(&mut self) -> Result<Stmt, ParseError> {
        self.eat(TokenType::If)?;
        let cond = self.parse_expr()?;
        self.eat(TokenType::LBrace)?;
        let then = self.parse_stmts()?;
        self.eat(TokenType::RBrace)?;
        let mut els = None;
        if self.maybe(TokenType::Else).is_some() {
            if self.at(TokenType::If) {
                els = Some(vec![self.parse_if()?]);
            } else {
                self.eat(TokenType::LBrace)?;
                els = Some(self.parse_stmts()?);
                self.eat(TokenType::RBrace)?;
            }
        }
        Ok(Stmt::If { cond, then, els })
    }
    fn parse_for(&mut self) -> Result<Stmt, ParseError> {
        self.eat(TokenType::For)?;
        let var = self.eat(TokenType::Ident)?.value;
        self.eat(TokenType::In)?;
        let iter = self.parse_expr()?;
        self.eat(TokenType::LBrace)?;
        let body = self.parse_stmts()?;
        self.eat(TokenType::RBrace)?;
        Ok(Stmt::For { var, iter, body })
    }
    fn parse_match(&mut self) -> Result<Stmt, ParseError> {
        self.eat(TokenType::Match)?;
        let expr = self.parse_expr()?;
        self.eat(TokenType::LBrace)?;
        let mut cases = Vec::new();
        while !self.at(TokenType::RBrace) {
            let mut pattern = String::new();
            let mut bindings = Vec::new();
            if self.at(TokenType::Ident) {
                pattern.push_str(&self.eat(TokenType::Ident)?.value);
                while self.maybe(TokenType::Dot).is_some() {
                    pattern.push('.');
                    pattern.push_str(&self.eat(TokenType::Ident)?.value);
                }
                if self.maybe(TokenType::LParen).is_some() {
                    if !self.at(TokenType::RParen) {
                        bindings.push(self.eat(TokenType::Ident)?.value);
                        while self.maybe(TokenType::Comma).is_some() { bindings.push(self.eat(TokenType::Ident)?.value); }
                    }
                    self.eat(TokenType::RParen)?;
                }
            } else if self.at(TokenType::Star) {
                self.eat(TokenType::Star)?;
                pattern = "_".into();
            }
            self.eat(TokenType::FatArrow)?;
            let body = if self.maybe(TokenType::LBrace).is_some() {
                let b = self.parse_stmts()?;
                self.eat(TokenType::RBrace)?;
                b
            } else {
                // Single statement body — wraps any statement (assignment, expr, etc.)
                vec![self.parse_stmt()?]
            };
            cases.push(MatchCase { pattern, bindings, body });
        }
        self.eat(TokenType::RBrace)?;
        Ok(Stmt::Match { expr, cases })
    }
    fn parse_print(&mut self) -> Result<Stmt, ParseError> {
        self.eat(TokenType::Print)?;
        self.eat(TokenType::LParen)?;
        let mut args = Vec::new();
        if !self.at(TokenType::RParen) {
            args.push(self.parse_expr()?);
            while self.maybe(TokenType::Comma).is_some() { args.push(self.parse_expr()?); }
        }
        self.eat(TokenType::RParen)?;
        Ok(Stmt::Print { args })
    }
    fn parse_unsafe(&mut self) -> Result<Stmt, ParseError> {
        self.eat(TokenType::Unsafe)?;
        self.eat(TokenType::LBrace)?;
        let body = self.parse_stmts()?;
        self.eat(TokenType::RBrace)?;
        Ok(Stmt::Unsafe { body })
    }
    fn parse_expr_or_assign(&mut self) -> Result<Stmt, ParseError> {
        let expr = self.parse_expr()?;
        let op = match self.cur().ty {
            TokenType::Assign => Some(AssignOp::Assign),
            TokenType::PlusEq => Some(AssignOp::PlusEq),
            TokenType::MinusEq => Some(AssignOp::MinusEq),
            TokenType::StarEq => Some(AssignOp::StarEq),
            TokenType::SlashEq => Some(AssignOp::SlashEq),
            _ => None,
        };
        if let Some(op) = op {
            self.eat(self.cur().ty.clone())?;
            let value = self.parse_expr()?;
            Ok(Stmt::Assign { target: expr, op, value })
        } else {
            Ok(Stmt::Expr { expr })
        }
    }

    // Expressions — precedence climbing
    fn parse_expr(&mut self) -> Result<Expr, ParseError> { self.parse_elvis() }
    fn parse_elvis(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_or()?;
        while self.at(TokenType::Question) && self.peek(1).ty == TokenType::Colon {
            self.eat(TokenType::Question)?; self.eat(TokenType::Colon)?;
            let right = self.parse_or()?;
            left = Expr::Elvis { left: Box::new(left), right: Box::new(right) };
        }
        Ok(left)
    }
    fn parse_or(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_and()?;
        while self.at(TokenType::Or) {
            self.eat(TokenType::Or)?;
            left = Expr::Binary { op: "||".into(), left: Box::new(left), right: Box::new(self.parse_and()?) };
        }
        Ok(left)
    }
    fn parse_and(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_eq()?;
        while self.at(TokenType::And) {
            self.eat(TokenType::And)?;
            left = Expr::Binary { op: "&&".into(), left: Box::new(left), right: Box::new(self.parse_eq()?) };
        }
        Ok(left)
    }
    fn parse_eq(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_cmp()?;
        while matches!(self.cur().ty, TokenType::Eq | TokenType::Neq) {
            let op = self.eat(self.cur().ty.clone())?.value;
            left = Expr::Binary { op, left: Box::new(left), right: Box::new(self.parse_cmp()?) };
        }
        Ok(left)
    }
    fn parse_cmp(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_is()?;
        while matches!(self.cur().ty, TokenType::LT | TokenType::LE | TokenType::GT | TokenType::GE) {
            let op = self.eat(self.cur().ty.clone())?.value;
            left = Expr::Binary { op, left: Box::new(left), right: Box::new(self.parse_is()?) };
        }
        Ok(left)
    }
    fn parse_is(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_add()?;
        while self.at(TokenType::Ident) && self.cur().value == "is" {
            self.eat(TokenType::Ident)?;
            let type_name = self.eat(TokenType::Ident)?.value;
            left = Expr::Cast { expr: Box::new(left), type_name };
        }
        Ok(left)
    }
    fn parse_add(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_mul()?;
        while matches!(self.cur().ty, TokenType::Plus | TokenType::Minus) {
            let op = self.eat(self.cur().ty.clone())?.value;
            left = Expr::Binary { op, left: Box::new(left), right: Box::new(self.parse_mul()?) };
        }
        Ok(left)
    }
    fn parse_mul(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_unary()?;
        while matches!(self.cur().ty, TokenType::Star | TokenType::Slash | TokenType::Percent) {
            let op = self.eat(self.cur().ty.clone())?.value;
            left = Expr::Binary { op, left: Box::new(left), right: Box::new(self.parse_unary()?) };
        }
        Ok(left)
    }
    fn parse_unary(&mut self) -> Result<Expr, ParseError> {
        if matches!(self.cur().ty, TokenType::Not | TokenType::Minus) {
            let op = self.eat(self.cur().ty.clone())?.value;
            return Ok(Expr::Unary { op, operand: Box::new(self.parse_unary()?) });
        }
        if self.at(TokenType::Amp) {
            self.eat(TokenType::Amp)?;
            return Ok(Expr::Unary { op: "&".into(), operand: Box::new(self.parse_unary()?) });
        }
        self.parse_postfix()
    }
    fn parse_postfix(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_primary()?;
        loop {
            if self.at(TokenType::Dot) {
                self.eat(TokenType::Dot)?;
                let name = self.eat(TokenType::Ident)?.value;
                if self.at(TokenType::LParen) {
                    self.eat(TokenType::LParen)?;
                    let mut args = Vec::new();
                    if !self.at(TokenType::RParen) {
                        args.push(self.parse_expr()?);
                        while self.maybe(TokenType::Comma).is_some() { args.push(self.parse_expr()?); }
                    }
                    self.eat(TokenType::RParen)?;
                    expr = Expr::Method { recv: Box::new(expr), name, args };
                } else {
                    expr = Expr::Field { recv: Box::new(expr), name };
                }
            } else if self.at(TokenType::Question) && self.peek(1).ty == TokenType::Dot {
                self.eat(TokenType::Question)?; self.eat(TokenType::Dot)?;
                let name = self.eat(TokenType::Ident)?.value;
                if self.at(TokenType::LParen) {
                    self.eat(TokenType::LParen)?;
                    let mut args = Vec::new();
                    if !self.at(TokenType::RParen) {
                        args.push(self.parse_expr()?);
                        while self.maybe(TokenType::Comma).is_some() { args.push(self.parse_expr()?); }
                    }
                    self.eat(TokenType::RParen)?;
                    expr = Expr::SafeCall { recv: Box::new(expr), name, args };
                } else {
                    expr = Expr::SafeCall { recv: Box::new(expr), name, args: Vec::new() };
                }
            } else if self.at(TokenType::LParen) {
                self.eat(TokenType::LParen)?;
                let mut args = Vec::new();
                if !self.at(TokenType::RParen) {
                    args.push(self.parse_expr()?);
                    while self.maybe(TokenType::Comma).is_some() { args.push(self.parse_expr()?); }
                }
                self.eat(TokenType::RParen)?;
                expr = Expr::Call { callee: Box::new(expr), args };
            } else { break; }
        }
        Ok(expr)
    }
    fn parse_primary(&mut self) -> Result<Expr, ParseError> {
        let tok = self.cur().clone();
        match tok.ty {
            TokenType::IntLiteral => { self.eat(TokenType::IntLiteral)?; Ok(Expr::Int { value: tok.value }) }
            TokenType::FloatLiteral => { self.eat(TokenType::FloatLiteral)?; Ok(Expr::Float { value: tok.value }) }
            TokenType::StringLiteral => { self.eat(TokenType::StringLiteral)?; Ok(Expr::String { value: tok.value }) }
            TokenType::CharLiteral => { self.eat(TokenType::CharLiteral)?; Ok(Expr::String { value: tok.value }) }
            TokenType::LParen => {
                self.eat(TokenType::LParen)?;
                let e = self.parse_expr()?;
                self.eat(TokenType::RParen)?;
                Ok(e)
            }
            TokenType::Ident | TokenType::Print | TokenType::Range => {
                let is_print = self.at(TokenType::Print);
                self.eat(self.cur().ty.clone())?;
                let value = tok.value;
                if value == "true" || value == "false" { return Ok(Expr::Bool { value: value == "true" }); }
                if value == "null" { return Ok(Expr::Null); }
                if value == "this" { return Ok(Expr::This); }
                if self.at(TokenType::LParen) && value.chars().next().map_or(false, |c| c.is_uppercase()) {
                    self.eat(TokenType::LParen)?;
                    let mut args = Vec::new();
                    if !self.at(TokenType::RParen) {
                        args.push(self.parse_expr()?);
                        while self.maybe(TokenType::Comma).is_some() { args.push(self.parse_expr()?); }
                    }
                    self.eat(TokenType::RParen)?;
                    return Ok(Expr::New { type_name: value, args });
                }
                if is_print && self.at(TokenType::LParen) {
                    self.eat(TokenType::LParen)?;
                    let mut args = Vec::new();
                    if !self.at(TokenType::RParen) {
                        args.push(self.parse_expr()?);
                        while self.maybe(TokenType::Comma).is_some() { args.push(self.parse_expr()?); }
                    }
                    self.eat(TokenType::RParen)?;
                    return Ok(Expr::Call { callee: Box::new(Expr::Ident { name: "println".into() }), args });
                }
                Ok(Expr::Ident { name: value })
            }
            _ => Err(ParseError::Unexpected {
                line: tok.line, col: tok.col,
                msg: "unexpected token in expression".into(),
                got: format!("{:?} '{}'", tok.ty, tok.value),
            }),
        }
    }
}

pub fn parse(src: &str) -> Result<Program, ParseError> {
    let toks = tokenize(src)?;
    Parser::new(toks).parse_program()
}
