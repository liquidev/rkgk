use core::{fmt, ops::Deref};

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: u32,
    pub end: u32,
}

impl Span {
    pub fn new(start: u32, end: u32) -> Self {
        Self { start, end }
    }

    pub fn slice<'a>(&self, source: &'a SourceCode) -> &'a str {
        &source.code[self.start as usize..self.end as usize]
    }
}

impl fmt::Debug for Span {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}..{}", self.start, self.end)
    }
}

/// Source code string with a verified size limit.
/// An exact size limit is not enforced by this type - it only ensures the string isn't longer than
/// intended, to not stall the parser for an unexpected amount of time.
#[derive(Debug, PartialEq, Eq)]
#[repr(transparent)]
pub struct SourceCode {
    code: str,
}

impl SourceCode {
    pub fn limited_len(code: &str, max_len: u32) -> Option<&Self> {
        if code.len() <= max_len as usize {
            Some(Self::unlimited_len(code))
        } else {
            None
        }
    }

    pub fn unlimited_len(code: &str) -> &Self {
        // SAFETY: SourceCode is a transparent wrapper around str, so converting between them is safe.
        unsafe { core::mem::transmute(code) }
    }
}

impl Deref for SourceCode {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.code
    }
}
