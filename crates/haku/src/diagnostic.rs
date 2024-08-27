use alloc::string::String;

use crate::source::Span;

#[derive(Debug, Clone)]
pub struct Diagnostic {
    span: Span,
    message: String,
}

impl Diagnostic {
    pub fn error(span: Span, message: impl Into<String>) -> Self {
        Self {
            span,
            message: message.into(),
        }
    }

    pub fn span(&self) -> Span {
        self.span
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}
