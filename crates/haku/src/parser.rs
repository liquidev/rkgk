use core::{cell::Cell, error::Error, fmt};

use alloc::vec::Vec;

use crate::{
    ast::{Ast, NodeAllocError, NodeId, NodeKind},
    diagnostic::Diagnostic,
    source::Span,
    token::{Lexis, TokenKind, TokenKindSet},
};

#[derive(Debug, Clone, Copy)]
pub struct ParserLimits {
    pub max_events: usize,
}

pub struct Parser<'a> {
    tokens: &'a Lexis,
    events: Vec<Event>,
    position: u32,
    fuel: Cell<u32>,
    pub diagnostics: Vec<Diagnostic>,
}

struct Event {
    kind: EventKind,

    #[cfg(debug_assertions)]
    from: &'static core::panic::Location<'static>,
}

#[derive(Debug)]
enum EventKind {
    Open { kind: NodeKind },
    Close,
    Advance,
}

struct Open {
    index: Option<usize>,
}

struct Closed {
    index: Option<usize>,
}

impl<'a> Parser<'a> {
    const FUEL: u32 = 256;

    pub fn new(input: &'a Lexis, limits: &ParserLimits) -> Self {
        assert!(limits.max_events < u32::MAX as usize);

        Self {
            tokens: input,
            events: Vec::with_capacity(limits.max_events),
            position: 0,
            diagnostics: Vec::with_capacity(16),
            fuel: Cell::new(Self::FUEL),
        }
    }

    #[track_caller]
    fn event(&mut self, event: EventKind) -> Option<usize> {
        if self.events.len() < self.events.capacity() {
            let index = self.events.len();
            self.events.push(Event::new(event));
            Some(index)
        } else {
            None
        }
    }

    #[track_caller]
    fn open(&mut self) -> Open {
        Open {
            index: self.event(EventKind::Open {
                kind: NodeKind::Error,
            }),
        }
    }

    #[track_caller]
    fn open_before(&mut self, closed: Closed) -> Open {
        if let Some(index) = closed.index {
            if self.events.len() < self.events.capacity() {
                self.events.insert(
                    index,
                    Event::new(EventKind::Open {
                        kind: NodeKind::Error,
                    }),
                );
                return Open { index: Some(index) };
            }
        }
        Open { index: None }
    }

    #[track_caller]
    fn close(&mut self, open: Open, kind: NodeKind) -> Closed {
        if let Some(index) = open.index {
            self.events[index].kind = EventKind::Open { kind };
            self.event(EventKind::Close);
            Closed { index: Some(index) }
        } else {
            Closed { index: None }
        }
    }

    fn is_eof(&self) -> bool {
        self.peek() == TokenKind::Eof
    }

    fn advance(&mut self) {
        if !self.is_eof() {
            self.position += 1;
            self.event(EventKind::Advance);
            self.fuel.set(Self::FUEL);
        }
    }

    #[track_caller]
    fn peek(&self) -> TokenKind {
        assert_ne!(self.fuel.get(), 0, "parser is stuck");
        self.fuel.set(self.fuel.get() - 1);

        self.tokens.kind(self.position)
    }

    fn span(&self) -> Span {
        self.tokens.span(self.position)
    }

    fn emit(&mut self, diagnostic: Diagnostic) {
        if self.diagnostics.len() < self.diagnostics.capacity() {
            self.diagnostics.push(diagnostic);
        }
    }

    #[track_caller]
    fn advance_with_error(&mut self) -> Closed {
        let opened = self.open();
        self.advance();
        self.close(opened, NodeKind::Error)
    }

    fn optional_newline(&mut self) -> bool {
        if self.peek() == TokenKind::Newline {
            self.advance();
            true
        } else {
            false
        }
    }

    pub fn into_ast(self, ast: &mut Ast) -> Result<(NodeId, Vec<Diagnostic>), IntoAstError> {
        let mut token = 0;
        let mut events = self.events;
        let mut stack = Vec::new();

        struct StackEntry {
            node_id: NodeId,
            // TODO: This should probably be optimized to use a shared stack.
            children: Vec<NodeId>,
        }

        // Remove the last Close to keep a single node on the stack.
        assert!(matches!(
            events.pop(),
            Some(Event {
                kind: EventKind::Close,
                ..
            })
        ));

        for event in events {
            match event.kind {
                EventKind::Open { kind } => {
                    stack.push(StackEntry {
                        node_id: ast.alloc(kind, self.tokens.span(token))?,
                        children: Vec::new(),
                    });
                }
                EventKind::Close => {
                    let end_span = self.tokens.span(token.saturating_sub(1));
                    let stack_entry = stack.pop().unwrap();
                    ast.alloc_children(stack_entry.node_id, &stack_entry.children);
                    ast.extend_span(stack_entry.node_id, end_span.end);
                    stack.last_mut().unwrap().children.push(stack_entry.node_id);
                }
                EventKind::Advance => {
                    let span = self.tokens.span(token);
                    let node_id = ast.alloc(NodeKind::Token, span)?;
                    stack
                        .last_mut()
                        .expect("advance() may only be used in an open node")
                        .children
                        .push(node_id);
                    token += 1;
                }
            }
        }

        if stack.len() != 1 {
            // This means we had too many events emitted and they are no longer balanced.
            return Err(IntoAstError::UnbalancedEvents);
        }
        // assert_eq!(token, self.tokens.len());

        let end_span = self.tokens.span(token.saturating_sub(1));
        let stack_entry = stack.pop().unwrap();
        ast.alloc_children(stack_entry.node_id, &stack_entry.children);
        ast.extend_span(stack_entry.node_id, end_span.end);

        Ok((stack_entry.node_id, self.diagnostics))
    }
}

impl<'a> fmt::Debug for Parser<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Parser")
            .field("events", &self.events)
            .finish_non_exhaustive()
    }
}

impl Event {
    #[track_caller]
    fn new(kind: EventKind) -> Event {
        Event {
            kind,
            #[cfg(debug_assertions)]
            from: core::panic::Location::caller(),
        }
    }
}

impl fmt::Debug for Event {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        #[cfg(debug_assertions)]
        {
            write!(
                f,
                "{:?} @ {}:{}:{}",
                self.kind,
                self.from.file(),
                self.from.line(),
                self.from.column()
            )?;
        }

        #[cfg(not(debug_assertions))]
        {
            write!(f, "{:?}", self.kind,)?;
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntoAstError {
    NodeAlloc(NodeAllocError),
    UnbalancedEvents,
}

impl From<NodeAllocError> for IntoAstError {
    fn from(value: NodeAllocError) -> Self {
        Self::NodeAlloc(value)
    }
}

impl fmt::Display for IntoAstError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IntoAstError::NodeAlloc(e) => fmt::Display::fmt(e, f),
            IntoAstError::UnbalancedEvents => f.write_str("parser produced unbalanced events"),
        }
    }
}

impl Error for IntoAstError {}

enum Tighter {
    Left,
    Right,
}

fn tighter(left: TokenKind, right: TokenKind) -> Tighter {
    fn tightness(kind: TokenKind) -> Option<usize> {
        match kind {
            TokenKind::Equal => Some(0),
            TokenKind::EqualEqual
            | TokenKind::NotEqual
            | TokenKind::Less
            | TokenKind::LessEqual
            | TokenKind::Greater
            | TokenKind::GreaterEqual => Some(1),
            TokenKind::Plus | TokenKind::Minus => Some(2),
            TokenKind::Star | TokenKind::Slash => Some(3),
            _ if PREFIX_TOKENS.contains(kind) => Some(4),
            _ => None,
        }
    }

    let Some(right_tightness) = tightness(right) else {
        return Tighter::Left;
    };
    let Some(left_tightness) = tightness(left) else {
        assert!(left == TokenKind::Eof);
        return Tighter::Right;
    };

    if right_tightness > left_tightness {
        Tighter::Right
    } else {
        Tighter::Left
    }
}

fn precedence_parse(p: &mut Parser, left: TokenKind) {
    let mut lhs = prefix(p);

    loop {
        let right = p.peek();
        match tighter(left, right) {
            Tighter::Left => break,
            Tighter::Right => {
                let o = p.open_before(lhs);
                let kind = infix(p, right);
                lhs = p.close(o, kind);
            }
        }
    }
}

fn one(p: &mut Parser, kind: NodeKind) -> Closed {
    let o = p.open();
    p.advance();
    p.close(o, kind)
}

fn list(p: &mut Parser) -> Closed {
    let o = p.open();
    let lspan = p.span();
    p.advance(); // [
    p.optional_newline();

    loop {
        match p.peek() {
            TokenKind::Eof => {
                p.emit(Diagnostic::error(lspan, "missing `]` to close this list"));
                break;
            }

            TokenKind::RBrack => {
                p.advance();
                break;
            }

            _ => (),
        }

        expr(p);

        match p.peek() {
            TokenKind::Comma | TokenKind::Newline => {
                p.advance();
                continue;
            }

            TokenKind::RBrack => {
                p.advance();
                break;
            }

            _ => {
                let span = p.span();
                p.emit(Diagnostic::error(
                    span,
                    "comma `,` or new line expected after list element",
                ));
                p.advance_with_error();
            }
        }
    }

    p.close(o, NodeKind::List)
}

fn unary(p: &mut Parser) -> Closed {
    let o = p.open();

    let op = p.open();
    p.advance();
    p.close(op, NodeKind::Op);

    prefix(p);

    p.close(o, NodeKind::Unary)
}

fn paren(p: &mut Parser) -> Closed {
    let o = p.open();
    let lspan = p.span();
    p.advance(); // (
    if p.peek() == TokenKind::RParen {
        p.advance(); // )
        p.close(o, NodeKind::ParenEmpty)
    } else {
        p.optional_newline();
        expr(p);
        p.optional_newline();
        if p.peek() != TokenKind::RParen {
            p.emit(Diagnostic::error(lspan, "missing closing parenthesis `)`"));
            p.advance_with_error();
        } else {
            p.advance();
        }
        p.close(o, NodeKind::Paren)
    }
}

fn param(p: &mut Parser) {
    let o = p.open();

    if let TokenKind::Ident | TokenKind::Underscore = p.peek() {
        p.advance();
    } else {
        let span = p.span();
        p.emit(Diagnostic::error(
            span,
            "parameter names must be identifiers or `_`",
        ));
        p.advance_with_error();
    }

    p.close(o, NodeKind::Param);
}

fn lambda(p: &mut Parser) -> Closed {
    let o = p.open();
    p.advance(); // backslash

    let params = p.open();
    loop {
        param(p);
        match p.peek() {
            TokenKind::Comma => {
                p.advance();
                continue;
            }

            TokenKind::RArrow => break,

            _ => {
                let span = p.span();
                p.emit(Diagnostic::error(
                    span,
                    "`,` or `->` expected after function parameter",
                ));
                p.advance_with_error();
                break;
            }
        }
    }
    p.close(params, NodeKind::Params);

    // NOTE: Can be false if there are some stray tokens.
    // We prefer to bail early and let the rest of the program parse.
    if p.peek() == TokenKind::RArrow {
        p.advance();
        p.optional_newline();
        expr(p);
    }

    p.close(o, NodeKind::Lambda)
}

fn if_expr(p: &mut Parser) -> Closed {
    let o = p.open();

    p.advance(); // if
    if p.peek() != TokenKind::LParen {
        let span = p.span();
        p.emit(Diagnostic::error(
            span,
            "the condition in an `if` expression must be surrounded with parentheses",
        ));
        // NOTE: Don't advance, it's more likely the programmer expected no parentheses to be needed.
    }
    p.advance();
    expr(p); // Condition
    if p.peek() != TokenKind::RParen {
        let span = p.span();
        p.emit(Diagnostic::error(
            span,
            "missing closing parenthesis after `if` condition",
        ));
    }
    p.advance();
    p.optional_newline();

    expr(p); // True branch
    p.optional_newline();

    if p.peek() != TokenKind::Else {
        let span = p.span();
        p.emit(Diagnostic::error(
            span,
            "`if` expression is missing an `else` clause",
        ));
    }
    p.advance();
    p.optional_newline();

    expr(p); // False branch

    p.close(o, NodeKind::If)
}

fn let_expr(p: &mut Parser) -> Closed {
    let o = p.open();

    p.advance(); // let

    if p.peek() == TokenKind::Ident {
        let ident = p.open();
        p.advance();
        p.close(ident, NodeKind::Ident);
    } else {
        let span = p.span();
        p.emit(Diagnostic::error(span, "`let` variable name expected"));
        p.advance_with_error();
    }

    if p.peek() == TokenKind::Equal {
        p.advance();
    } else {
        let span = p.span();
        p.emit(Diagnostic::error(span, "`=` expected after variable name"));
        p.advance_with_error();
    }

    expr(p);

    if p.peek() == TokenKind::Newline {
        p.advance();
    } else {
        let span = p.span();
        p.emit(Diagnostic::error(
            span,
            "new line expected after `let` expression",
        ));
        p.advance_with_error();
    }

    expr(p);

    p.close(o, NodeKind::Let)
}

const PREFIX_TOKENS: TokenKindSet = TokenKindSet::new(&[
    TokenKind::Ident,
    TokenKind::Tag,
    TokenKind::Number,
    TokenKind::Color,
    // NOTE: This is ambiguous in function calls.
    // In that case, the infix operator takes precedence (because the `match` arms for the infix op
    // come first.)
    TokenKind::Minus,
    TokenKind::Not,
    TokenKind::LParen,
    TokenKind::Backslash,
    TokenKind::If,
    TokenKind::Let,
    TokenKind::LBrack,
]);

fn prefix(p: &mut Parser) -> Closed {
    match p.peek() {
        TokenKind::Ident => one(p, NodeKind::Ident),
        TokenKind::Tag => one(p, NodeKind::Tag),
        TokenKind::Number => one(p, NodeKind::Number),
        TokenKind::Color => one(p, NodeKind::Color),
        TokenKind::LBrack => list(p),

        TokenKind::Minus | TokenKind::Not => unary(p),
        TokenKind::LParen => paren(p),
        TokenKind::Backslash => lambda(p),
        TokenKind::If => if_expr(p),
        TokenKind::Let => let_expr(p),

        _ => {
            assert!(
                !PREFIX_TOKENS.contains(p.peek()),
                "{:?} found in PREFIX_TOKENS",
                p.peek()
            );

            let span = p.span();
            p.emit(Diagnostic::error(
                span,
                "an expression was expected, but this token does not start one",
            ));
            p.advance_with_error()
        }
    }
}

fn infix(p: &mut Parser, op: TokenKind) -> NodeKind {
    match op {
        TokenKind::Plus
        | TokenKind::Minus
        | TokenKind::Star
        | TokenKind::Slash
        | TokenKind::EqualEqual
        | TokenKind::NotEqual
        | TokenKind::Less
        | TokenKind::LessEqual
        | TokenKind::Greater
        | TokenKind::GreaterEqual
        | TokenKind::Equal => infix_binary(p, op),

        _ if PREFIX_TOKENS.contains(op) => infix_call(p),

        _ => panic!("unhandled infix operator {op:?}"),
    }
}

fn infix_binary(p: &mut Parser, op: TokenKind) -> NodeKind {
    let o = p.open();
    p.advance();
    p.close(o, NodeKind::Op);

    if p.peek() == TokenKind::Newline {
        p.advance();
    }

    precedence_parse(p, op);
    NodeKind::Binary
}

fn infix_call(p: &mut Parser) -> NodeKind {
    while PREFIX_TOKENS.contains(p.peek()) {
        prefix(p);
    }

    NodeKind::Call
}

pub fn expr(p: &mut Parser) {
    precedence_parse(p, TokenKind::Eof)
}

pub fn toplevel(p: &mut Parser) {
    let o = p.open();
    p.optional_newline();
    while p.peek() != TokenKind::Eof {
        expr(p);

        match p.peek() {
            TokenKind::Newline => {
                p.advance();
                continue;
            }

            TokenKind::Eof => break,

            _ => {
                let span = p.span();
                p.emit(Diagnostic::error(
                    span,
                    "newline expected after toplevel expression",
                ))
            }
        }
    }
    p.close(o, NodeKind::Toplevel);
}

#[cfg(test)]
mod tests;
