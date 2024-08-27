use core::{error::Error, fmt::Display};

use alloc::vec::Vec;

use crate::source::Span;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    Eof,

    Ident,
    Tag,
    Number,
    Color,

    // Operators
    Plus,
    Minus,
    Star,
    Slash,
    EqualEqual,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    Not,

    // Punctuation
    Newline,
    LParen,
    RParen,
    LBrack,
    RBrack,
    Comma,
    Equal,
    Backslash,
    RArrow,

    // Keywords
    Underscore,
    And,
    Or,
    If,
    Else,
    Let,

    // NOTE: This must be kept last for TokenSet to work correctly.
    Error,
}

#[derive(Debug, Clone)]
pub struct Lexis {
    pub kinds: Vec<TokenKind>,
    pub spans: Vec<Span>,
}

impl Lexis {
    pub fn new(capacity: usize) -> Self {
        assert!(capacity < u32::MAX as usize);

        Self {
            kinds: Vec::with_capacity(capacity),
            spans: Vec::with_capacity(capacity),
        }
    }

    pub fn len(&self) -> u32 {
        self.kinds.len() as u32
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn push(&mut self, kind: TokenKind, span: Span) -> Result<(), TokenAllocError> {
        if self.kinds.len() >= self.kinds.capacity() {
            return Err(TokenAllocError);
        }

        self.kinds.push(kind);
        self.spans.push(span);

        Ok(())
    }

    pub fn kind(&self, position: u32) -> TokenKind {
        self.kinds[position as usize]
    }

    pub fn span(&self, position: u32) -> Span {
        self.spans[position as usize]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TokenAllocError;

impl Display for TokenAllocError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("too many tokens")
    }
}

impl Error for TokenAllocError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TokenKindSet {
    bits: [u32; Self::WORDS],
}

impl TokenKindSet {
    const WORDS: usize = ((TokenKind::Error as u32 + u32::BITS - 1) / (u32::BITS)) as usize;

    const fn word(kind: TokenKind) -> usize {
        (kind as u32 / u32::BITS) as usize
    }

    const fn bit(kind: TokenKind) -> u32 {
        1 << (kind as u32 % u32::BITS)
    }

    pub const fn new(elems: &[TokenKind]) -> Self {
        let mut set = Self {
            bits: [0; Self::WORDS],
        };
        let mut i = 0;
        while i < elems.len() {
            set = set.include(elems[i]);
            i += 1;
        }
        set
    }

    pub const fn include(mut self, kind: TokenKind) -> Self {
        self.bits[Self::word(kind)] |= Self::bit(kind);
        self
    }

    pub fn contains(&self, kind: TokenKind) -> bool {
        self.bits[Self::word(kind)] & Self::bit(kind) != 0
    }
}
