use alloc::vec::Vec;

use crate::{
    diagnostic::Diagnostic,
    source::{SourceCode, Span},
    token::{Lexis, TokenAllocError, TokenKind},
};

pub struct Lexer<'a> {
    pub lexis: Lexis,
    pub diagnostics: Vec<Diagnostic>,
    input: &'a SourceCode,
    position: u32,
}

impl<'a> Lexer<'a> {
    pub fn new(lexis: Lexis, input: &'a SourceCode) -> Self {
        Self {
            lexis,
            diagnostics: Vec::new(),
            input,
            position: 0,
        }
    }

    fn current(&self) -> char {
        self.input[self.position as usize..]
            .chars()
            .next()
            .unwrap_or('\0')
    }

    fn advance(&mut self) {
        self.position += self.current().len_utf8() as u32;
    }

    fn emit(&mut self, diagnostic: Diagnostic) {
        if self.diagnostics.len() < self.diagnostics.capacity() {
            self.diagnostics.push(diagnostic);
        }
    }
}

fn one(l: &mut Lexer<'_>, kind: TokenKind) -> TokenKind {
    l.advance();
    kind
}

fn one_or_two(l: &mut Lexer<'_>, kind1: TokenKind, c2: char, kind2: TokenKind) -> TokenKind {
    l.advance();
    if l.current() == c2 {
        l.advance();
        kind2
    } else {
        kind1
    }
}

fn is_ident_char(c: char) -> bool {
    matches!(c, 'a'..='z' | 'A'..='Z' | '0'..='9' | '_')
}

fn ident(l: &mut Lexer<'_>) -> TokenKind {
    let start = l.position;
    while is_ident_char(l.current()) {
        l.advance();
    }
    let end = l.position;

    match Span::new(start, end).slice(l.input) {
        "_" => TokenKind::Underscore,
        "and" => TokenKind::And,
        "or" => TokenKind::Or,
        "if" => TokenKind::If,
        "else" => TokenKind::Else,
        "let" => TokenKind::Let,
        _ => TokenKind::Ident,
    }
}

fn tag(l: &mut Lexer<'_>) -> TokenKind {
    while is_ident_char(l.current()) {
        l.advance();
    }
    TokenKind::Tag
}

// NOTE: You shouldn't expect that the numbers produced by the lexer are parsable.
fn number(l: &mut Lexer<'_>) -> TokenKind {
    while l.current().is_ascii_digit() {
        l.advance();
    }

    if l.current() == '.' {
        let dot = l.position;
        l.advance();
        if !l.current().is_ascii_digit() {
            l.emit(Diagnostic::error(
                Span::new(dot, l.position),
                "there must be at least a single digit after the decimal point",
            ));
        }
        while l.current().is_ascii_digit() {
            l.advance();
        }
    }

    TokenKind::Number
}

// NOTE: You shouldn't expect that the color literals produced by the lexer are parsable.
fn color(l: &mut Lexer<'_>) -> TokenKind {
    let hash = l.position;
    l.advance(); // #

    if !l.current().is_ascii_hexdigit() {
        l.emit(Diagnostic::error(
            Span::new(hash, l.position),
            "hex digits expected after `#` (color literal)",
        ));
    }

    let start = l.position;
    while l.current().is_ascii_hexdigit() {
        l.advance();
    }
    let len = l.position - start;

    if !matches!(len, 3 | 4 | 6 | 8) {
        l.emit(Diagnostic::error(Span::new(hash, l.position), "incorrect number of digits in color literal (must be #RGB, #RGBA, #RRGGBB, or #RRGGBBAA)"));
    }

    TokenKind::Color
}

fn whitespace_and_comments(l: &mut Lexer<'_>) {
    loop {
        match l.current() {
            '-' => {
                let position = l.position;
                l.advance();
                if l.current() == '-' {
                    while l.current() != '\n' {
                        l.advance();
                    }
                } else {
                    // An unfortunate little bit of backtracking here;
                    // This seems like the simplest possible solution though.
                    // We don't treat comments as a separate token to simplify the parsing phase,
                    // and because of this, handling this at the "real" token level would complicate
                    // things quite a bit.
                    l.position = position;
                    break;
                }
            }

            ' ' | '\r' | '\t' => l.advance(),

            _ => break,
        }
    }
}

fn newline(l: &mut Lexer<'_>) -> (TokenKind, Span) {
    let start = l.position;
    l.advance(); // skip the initial newline
    let end = l.position;

    // Skip additional newlines after this one, to only produce one token.
    // These do not count into this newline's span though.
    loop {
        whitespace_and_comments(l);
        if l.current() == '\n' {
            l.advance();
            continue;
        } else {
            break;
        }
    }

    (TokenKind::Newline, Span::new(start, end))
}

fn token(l: &mut Lexer<'_>) -> (TokenKind, Span) {
    whitespace_and_comments(l);

    let start = l.position;
    let kind = match l.current() {
        '\0' => TokenKind::Eof,

        // NOTE: Order matters here. Numbers and tags take priority over identifers.
        c if c.is_ascii_uppercase() => tag(l),
        c if c.is_ascii_digit() => number(l),
        c if is_ident_char(c) => ident(l),

        '#' => color(l),

        '+' => one(l, TokenKind::Plus),
        '-' => one_or_two(l, TokenKind::Minus, '>', TokenKind::RArrow),
        '*' => one(l, TokenKind::Star),
        '/' => one(l, TokenKind::Slash),
        '=' => one_or_two(l, TokenKind::Equal, '=', TokenKind::EqualEqual),
        '!' => one_or_two(l, TokenKind::Not, '=', TokenKind::NotEqual),
        '<' => one_or_two(l, TokenKind::Less, '=', TokenKind::LessEqual),
        '>' => one_or_two(l, TokenKind::Greater, '=', TokenKind::GreaterEqual),

        '\n' => return newline(l),
        '(' => one(l, TokenKind::LParen),
        ')' => one(l, TokenKind::RParen),
        '[' => one(l, TokenKind::LBrack),
        ']' => one(l, TokenKind::RBrack),
        ',' => one(l, TokenKind::Comma),
        '\\' => one(l, TokenKind::Backslash),

        _ => {
            l.advance();
            l.emit(Diagnostic::error(
                Span::new(start, l.position),
                "unexpected character",
            ));
            TokenKind::Error
        }
    };
    let end = l.position;
    (kind, Span::new(start, end))
}

pub fn lex(l: &mut Lexer<'_>) -> Result<(), TokenAllocError> {
    loop {
        let (kind, span) = token(l);
        l.lexis.push(kind, span)?;
        if kind == TokenKind::Eof {
            break;
        }
    }
    Ok(())
}
