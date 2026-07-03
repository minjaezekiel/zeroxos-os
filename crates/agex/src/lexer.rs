//! agex lexer — tokenizes source into typed tokens.

use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenType {
    // Keywords
    Let, Const, Var, Fn, Return, If, Else, For, In, Range, Match,
    Class, Data, Sealed, Interface, Object, Driver, Async, Await,
    Unsafe, Extern, Import, Print, Interrupt, Capability,
    // Literals
    IntLiteral, FloatLiteral, StringLiteral, CharLiteral,
    Ident,
    // Operators
    Arrow,        // ->
    FatArrow,     // =>
    Assign,       // =
    PlusEq, MinusEq, StarEq, SlashEq,
    Plus, Minus, Star, Slash, Percent,
    Eq, Neq, LT, LE, GT, GE,
    And, Or, Not,
    Amp, Pipe,
    Question,
    Ellipsis,
    // Punctuation
    LBrace, RBrace, LParen, RParen, LBracket, RBracket,
    Comma, Colon, Semicolon, Dot,
    // Special
    Eof,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub ty: TokenType,
    pub value: String,
    pub line: usize,
    pub col: usize,
}

#[derive(Debug, Error)]
pub enum LexerError {
    #[error("unexpected character '{char}' at {line}:{col}")]
    UnexpectedChar { char: char, line: usize, col: usize },
    #[error("unterminated string at {line}:{col}")]
    UnterminatedString { line: usize, col: usize },
}

const KEYWORDS: &[(&str, TokenType)] = &[
    ("let", TokenType::Let),
    ("const", TokenType::Const),
    ("var", TokenType::Var),
    ("fn", TokenType::Fn),
    ("return", TokenType::Return),
    ("if", TokenType::If),
    ("else", TokenType::Else),
    ("for", TokenType::For),
    ("in", TokenType::In),
    ("range", TokenType::Range),
    ("match", TokenType::Match),
    ("class", TokenType::Class),
    ("data", TokenType::Data),
    ("sealed", TokenType::Sealed),
    ("interface", TokenType::Interface),
    ("object", TokenType::Object),
    ("driver", TokenType::Driver),
    ("async", TokenType::Async),
    ("await", TokenType::Await),
    ("unsafe", TokenType::Unsafe),
    ("extern", TokenType::Extern),
    ("import", TokenType::Import),
    ("print", TokenType::Print),
    ("interrupt", TokenType::Interrupt),
    ("capability", TokenType::Capability),
];

pub fn tokenize(src: &str) -> Result<Vec<Token>, LexerError> {
    let chars: Vec<char> = src.chars().collect();
    let mut tokens = Vec::new();
    let mut i = 0;
    let mut line = 1;
    let mut col = 1;

    while i < chars.len() {
        let c = chars[i];

        // Whitespace
        if c == '\n' {
            line += 1; col = 1; i += 1; continue;
        }
        if c.is_whitespace() {
            col += 1; i += 1; continue;
        }

        // Line comment
        if c == '/' && i + 1 < chars.len() && chars[i + 1] == '/' {
            while i < chars.len() && chars[i] != '\n' { i += 1; }
            continue;
        }
        // Block comment
        if c == '/' && i + 1 < chars.len() && chars[i + 1] == '*' {
            i += 2; col += 2;
            while i + 1 < chars.len() && !(chars[i] == '*' && chars[i + 1] == '/') {
                if chars[i] == '\n' { line += 1; col = 1; } else { col += 1; }
                i += 1;
            }
            if i + 1 < chars.len() { i += 2; col += 2; }
            continue;
        }

        // Numbers
        if c.is_ascii_digit() {
            let start_line = line; let start_col = col;
            let mut num = String::new();
            let mut is_float = false;
            while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '_') {
                num.push(chars[i]); col += 1; i += 1;
            }
            if i < chars.len() && chars[i] == '.' && i + 1 < chars.len() && chars[i + 1].is_ascii_digit() {
                is_float = true;
                num.push('.'); col += 1; i += 1;
                while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '_') {
                    num.push(chars[i]); col += 1; i += 1;
                }
            }
            // type suffix (u8, i32, f64, etc.)
            while i < chars.len() && chars[i].is_ascii_alphanumeric() {
                num.push(chars[i]); col += 1; i += 1;
            }
            tokens.push(Token {
                ty: if is_float { TokenType::FloatLiteral } else { TokenType::IntLiteral },
                value: num,
                line: start_line,
                col: start_col,
            });
            continue;
        }

        // Strings
        if c == '"' {
            let start_line = line; let start_col = col;
            i += 1; col += 1;
            let mut s = String::new();
            while i < chars.len() && chars[i] != '"' {
                if chars[i] == '\\' && i + 1 < chars.len() {
                    let esc = chars[i + 1];
                    s.push(match esc {
                        'n' => '\n', 't' => '\t', 'r' => '\r',
                        '\\' => '\\', '"' => '"', '0' => '\0',
                        other => other,
                    });
                    i += 2; col += 2;
                } else {
                    if chars[i] == '\n' { line += 1; col = 1; } else { col += 1; }
                    s.push(chars[i]); i += 1;
                }
            }
            if i >= chars.len() {
                return Err(LexerError::UnterminatedString { line: start_line, col: start_col });
            }
            i += 1; col += 1;
            tokens.push(Token { ty: TokenType::StringLiteral, value: s, line: start_line, col: start_col });
            continue;
        }

        // Char literals
        if c == '\'' {
            let start_line = line; let start_col = col;
            i += 1; col += 1;
            let mut s = String::new();
            while i < chars.len() && chars[i] != '\'' {
                if chars[i] == '\\' && i + 1 < chars.len() {
                    let esc = chars[i + 1];
                    s.push(match esc {
                        'n' => '\n', 't' => '\t', 'r' => '\r',
                        '\\' => '\\', '\'' => '\'', '0' => '\0',
                        other => other,
                    });
                    i += 2; col += 2;
                } else {
                    s.push(chars[i]); i += 1; col += 1;
                }
            }
            if i < chars.len() { i += 1; col += 1; }
            tokens.push(Token { ty: TokenType::CharLiteral, value: s, line: start_line, col: start_col });
            continue;
        }

        // Identifiers / keywords
        if c.is_ascii_alphabetic() || c == '_' {
            let start_line = line; let start_col = col;
            let mut ident = String::new();
            while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                ident.push(chars[i]); col += 1; i += 1;
            }
            let ty = KEYWORDS.iter()
                .find(|(k, _)| *k == ident)
                .map(|(_, t)| t.clone())
                .unwrap_or(TokenType::Ident);
            tokens.push(Token { ty, value: ident, line: start_line, col: start_col });
            continue;
        }

        // Operators and punctuation
        let start_line = line; let start_col = col;
        let three: String = chars[i..].iter().take(3).collect();
        if three == "..." {
            tokens.push(Token { ty: TokenType::Ellipsis, value: "...".into(), line: start_line, col: start_col });
            i += 3; col += 3; continue;
        }
        let two: String = chars[i..].iter().take(2).collect();
        let two_ty = match two.as_str() {
            "->" => Some(TokenType::Arrow),
            "=>" => Some(TokenType::FatArrow),
            "==" => Some(TokenType::Eq),
            "!=" => Some(TokenType::Neq),
            "<=" => Some(TokenType::LE),
            ">=" => Some(TokenType::GE),
            "&&" => Some(TokenType::And),
            "||" => Some(TokenType::Or),
            "+=" => Some(TokenType::PlusEq),
            "-=" => Some(TokenType::MinusEq),
            "*=" => Some(TokenType::StarEq),
            "/=" => Some(TokenType::SlashEq),
            _ => None,
        };
        if let Some(ty) = two_ty {
            tokens.push(Token { ty, value: two, line: start_line, col: start_col });
            i += 2; col += 2; continue;
        }
        let one_ty = match c {
            '{' => Some(TokenType::LBrace),
            '}' => Some(TokenType::RBrace),
            '(' => Some(TokenType::LParen),
            ')' => Some(TokenType::RParen),
            '[' => Some(TokenType::LBracket),
            ']' => Some(TokenType::RBracket),
            ',' => Some(TokenType::Comma),
            ':' => Some(TokenType::Colon),
            ';' => Some(TokenType::Semicolon),
            '.' => Some(TokenType::Dot),
            '?' => Some(TokenType::Question),
            '=' => Some(TokenType::Assign),
            '+' => Some(TokenType::Plus),
            '-' => Some(TokenType::Minus),
            '*' => Some(TokenType::Star),
            '/' => Some(TokenType::Slash),
            '%' => Some(TokenType::Percent),
            '<' => Some(TokenType::LT),
            '>' => Some(TokenType::GT),
            '!' => Some(TokenType::Not),
            '&' => Some(TokenType::Amp),
            '|' => Some(TokenType::Pipe),
            _ => None,
        };
        if let Some(ty) = one_ty {
            tokens.push(Token { ty, value: c.to_string(), line: start_line, col: start_col });
            i += 1; col += 1; continue;
        }

        return Err(LexerError::UnexpectedChar { char: c, line, col });
    }

    tokens.push(Token { ty: TokenType::Eof, value: String::new(), line, col });
    Ok(tokens)
}
