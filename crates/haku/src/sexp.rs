use core::{cell::Cell, fmt};

use alloc::vec::Vec;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    pub fn slice<'a>(&self, source: &'a str) -> &'a str {
        &source[self.start..self.end]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId(usize);

impl NodeId {
    pub const NIL: NodeId = NodeId(0);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeKind {
    Nil,
    Eof,

    // Atoms
    Ident,
    Number,

    List(NodeId, NodeId),
    Toplevel(NodeId),

    Error(&'static str),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Node {
    pub span: Span,
    pub kind: NodeKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ast {
    pub nodes: Vec<Node>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AstWriteMode {
    Compact,
    Spans,
}

impl Ast {
    pub fn new(capacity: usize) -> Self {
        assert!(capacity >= 1, "there must be space for at least a nil node");

        let mut ast = Self {
            nodes: Vec::with_capacity(capacity),
        };

        ast.alloc(Node {
            span: Span::new(0, 0),
            kind: NodeKind::Nil,
        })
        .unwrap();

        ast
    }

    pub fn alloc(&mut self, node: Node) -> Result<NodeId, NodeAllocError> {
        if self.nodes.len() >= self.nodes.capacity() {
            return Err(NodeAllocError);
        }

        let index = self.nodes.len();
        self.nodes.push(node);
        Ok(NodeId(index))
    }

    pub fn get(&self, node_id: NodeId) -> &Node {
        &self.nodes[node_id.0]
    }

    pub fn get_mut(&mut self, node_id: NodeId) -> &mut Node {
        &mut self.nodes[node_id.0]
    }

    pub fn write(
        &self,
        source: &str,
        node_id: NodeId,
        w: &mut dyn fmt::Write,
        mode: AstWriteMode,
    ) -> fmt::Result {
        #[allow(clippy::too_many_arguments)]
        fn write_list(
            ast: &Ast,
            source: &str,
            w: &mut dyn fmt::Write,
            mode: AstWriteMode,
            mut head: NodeId,
            mut tail: NodeId,
            sep_element: &str,
            sep_tail: &str,
        ) -> fmt::Result {
            loop {
                write_rec(ast, source, w, mode, head)?;
                match ast.get(tail).kind {
                    NodeKind::Nil => break,
                    NodeKind::List(head2, tail2) => {
                        w.write_str(sep_element)?;
                        (head, tail) = (head2, tail2);
                    }
                    _ => {
                        w.write_str(sep_tail)?;
                        write_rec(ast, source, w, mode, tail)?;
                        break;
                    }
                }
            }
            Ok(())
        }

        // NOTE: Separated out to a separate function in case we ever want to introduce auto-indentation.
        fn write_rec(
            ast: &Ast,
            source: &str,
            w: &mut dyn fmt::Write,
            mode: AstWriteMode,
            node_id: NodeId,
        ) -> fmt::Result {
            let node = ast.get(node_id);
            match &node.kind {
                NodeKind::Nil => write!(w, "()")?,
                NodeKind::Eof => write!(w, "<eof>")?,
                NodeKind::Ident | NodeKind::Number => write!(w, "{}", node.span.slice(source))?,

                NodeKind::List(head, tail) => {
                    w.write_char('(')?;
                    write_list(ast, source, w, mode, *head, *tail, " ", " . ")?;
                    w.write_char(')')?;
                }

                NodeKind::Toplevel(list) => {
                    let NodeKind::List(head, tail) = ast.get(*list).kind else {
                        unreachable!("child of Toplevel must be a List");
                    };

                    write_list(ast, source, w, mode, head, tail, "\n", " . ")?;
                }

                NodeKind::Error(message) => write!(w, "#error({message})")?,
            }

            if mode == AstWriteMode::Spans {
                write!(w, "@{}..{}", node.span.start, node.span.end)?;
            }

            Ok(())
        }

        write_rec(self, source, w, mode, node_id)?;

        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeAllocError;

pub struct Parser<'a> {
    pub ast: Ast,
    input: &'a str,
    position: usize,
    fuel: Cell<usize>,
    alloc_error: NodeId,
}

impl<'a> Parser<'a> {
    const FUEL: usize = 256;

    pub fn new(mut ast: Ast, input: &'a str) -> Self {
        let alloc_error = ast
            .alloc(Node {
                span: Span::new(0, 0),
                kind: NodeKind::Error("program is too big"),
            })
            .expect("there is not enough space in the arena for an error node");

        Self {
            ast,
            input,
            position: 0,
            fuel: Cell::new(Self::FUEL),
            alloc_error,
        }
    }

    #[track_caller]
    pub fn current(&self) -> char {
        assert_ne!(self.fuel.get(), 0, "parser is stuck");
        self.fuel.set(self.fuel.get() - 1);

        self.input[self.position..].chars().next().unwrap_or('\0')
    }

    pub fn advance(&mut self) {
        self.position += self.current().len_utf8();
        self.fuel.set(Self::FUEL);
    }

    pub fn alloc(&mut self, expr: Node) -> NodeId {
        self.ast.alloc(expr).unwrap_or(self.alloc_error)
    }
}

pub fn skip_whitespace_and_comments(p: &mut Parser<'_>) {
    loop {
        match p.current() {
            ' ' | '\t' | '\n' => {
                p.advance();
                continue;
            }
            ';' => {
                while p.current() != '\n' && p.current() != '\0' {
                    p.advance();
                }
            }
            _ => break,
        }
    }
}

fn is_decimal_digit(c: char) -> bool {
    c.is_ascii_digit()
}

pub fn parse_number(p: &mut Parser<'_>) -> NodeKind {
    while is_decimal_digit(p.current()) {
        p.advance();
    }
    if p.current() == '.' {
        p.advance();
        if !is_decimal_digit(p.current()) {
            return NodeKind::Error("missing digits after decimal point '.' in number literal");
        }
        while is_decimal_digit(p.current()) {
            p.advance();
        }
    }

    NodeKind::Number
}

fn is_ident(c: char) -> bool {
    // The identifier character set is quite limited to help with easy expansion in the future.
    // Rationale:
    // - alphabet and digits are pretty obvious
    // - '-' and '_' can be used for identifier separators, whichever you prefer.
    // - '+', '-', '*', '/', '^' are for arithmetic.
    // - '=', '!', '<', '>' are fore comparison.
    // - '\' is for builtin string constants, such as \n.
    // For other operators, it's generally clearer to use words (such as `and` and `or`.)
    matches!(c, 'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '+' | '*' | '/' | '\\' | '^' | '!' | '=' | '<' | '>')
}

pub fn parse_ident(p: &mut Parser<'_>) -> NodeKind {
    while is_ident(p.current()) {
        p.advance();
    }

    NodeKind::Ident
}

struct List {
    head: NodeId,
    tail: NodeId,
}

impl List {
    fn new() -> Self {
        Self {
            head: NodeId::NIL,
            tail: NodeId::NIL,
        }
    }

    fn append(&mut self, p: &mut Parser<'_>, node: NodeId) {
        let node_span = p.ast.get(node).span;

        let new_tail = p.alloc(Node {
            span: node_span,
            kind: NodeKind::List(node, NodeId::NIL),
        });
        if self.head == NodeId::NIL {
            self.head = new_tail;
            self.tail = new_tail;
        } else {
            let old_tail = p.ast.get_mut(self.tail);
            let NodeKind::List(expr_before, _) = old_tail.kind else {
                return;
            };
            *old_tail = Node {
                span: Span::new(old_tail.span.start, node_span.end),
                kind: NodeKind::List(expr_before, new_tail),
            };
            self.tail = new_tail;
        }
    }
}

pub fn parse_list(p: &mut Parser<'_>) -> NodeId {
    // This could've been a lot simpler if Rust supported tail recursion.

    let start = p.position;

    p.advance(); // skip past opening parenthesis
    skip_whitespace_and_comments(p);

    let mut list = List::new();

    while p.current() != ')' {
        if p.current() == '\0' {
            return p.alloc(Node {
                span: Span::new(start, p.position),
                kind: NodeKind::Error("missing ')' to close '('"),
            });
        }

        let expr = parse_expr(p);
        skip_whitespace_and_comments(p);

        list.append(p, expr);
    }
    p.advance(); // skip past closing parenthesis

    // If we didn't have any elements, we must not modify the initial Nil with ID 0.
    if list.head == NodeId::NIL {
        list.head = p.alloc(Node {
            span: Span::new(0, 0),
            kind: NodeKind::Nil,
        });
    }

    let end = p.position;
    p.ast.get_mut(list.head).span = Span::new(start, end);

    list.head
}

pub fn parse_expr(p: &mut Parser<'_>) -> NodeId {
    let start = p.position;
    let kind = match p.current() {
        '\0' => NodeKind::Eof,
        c if is_decimal_digit(c) => parse_number(p),
        // NOTE: Because of the `match` order, this prevents identifiers from starting with a digit.
        c if is_ident(c) => parse_ident(p),
        '(' => return parse_list(p),
        _ => {
            p.advance();
            NodeKind::Error("unexpected character")
        }
    };
    let end = p.position;

    p.alloc(Node {
        span: Span::new(start, end),
        kind,
    })
}

pub fn parse_toplevel(p: &mut Parser<'_>) -> NodeId {
    let start = p.position;

    let mut nodes = List::new();

    skip_whitespace_and_comments(p);
    while p.current() != '\0' {
        let expr = parse_expr(p);
        skip_whitespace_and_comments(p);

        nodes.append(p, expr);
    }

    let end = p.position;

    p.alloc(Node {
        span: Span::new(start, end),
        kind: NodeKind::Toplevel(nodes.head),
    })
}

#[cfg(test)]
mod tests {
    use core::error::Error;

    use alloc::{boxed::Box, string::String};

    use super::*;

    #[track_caller]
    fn parse(
        f: fn(&mut Parser<'_>) -> NodeId,
        source: &str,
        expected: &str,
    ) -> Result<(), Box<dyn Error>> {
        let ast = Ast::new(16);
        let mut p = Parser::new(ast, source);
        let node = f(&mut p);
        let ast = p.ast;

        let mut s = String::new();
        ast.write(source, node, &mut s, AstWriteMode::Spans)?;

        assert_eq!(s, expected);

        Ok(())
    }

    #[test]
    fn parse_number() -> Result<(), Box<dyn Error>> {
        parse(parse_expr, "123", "123@0..3")?;
        parse(parse_expr, "123.456", "123.456@0..7")?;
        Ok(())
    }

    #[test]
    fn parse_ident() -> Result<(), Box<dyn Error>> {
        parse(parse_expr, "abc", "abc@0..3")?;
        parse(parse_expr, "abcABC_01234", "abcABC_01234@0..12")?;
        parse(parse_expr, "+-*/\\^!=<>", "+-*/\\^!=<>@0..10")?;
        Ok(())
    }

    #[test]
    fn parse_list() -> Result<(), Box<dyn Error>> {
        parse(parse_expr, "()", "()@0..2")?;
        parse(parse_expr, "(a a)", "(a@1..2 a@3..4)@0..5")?;
        parse(parse_expr, "(a a a)", "(a@1..2 a@3..4 a@5..6)@0..7")?;
        parse(parse_expr, "(() ())", "(()@1..3 ()@4..6)@0..7")?;
        parse(
            parse_expr,
            "(nestedy (nest OwO))",
            "(nestedy@1..8 (nest@10..14 OwO@15..18)@9..19)@0..20",
        )?;
        Ok(())
    }

    #[test]
    fn oom() -> Result<(), Box<dyn Error>> {
        parse(parse_expr, "(a a a a a a a a)", "(a@1..2 a@3..4 a@5..6 a@7..8 a@9..10 a@11..12 a@13..14 . #error(program is too big)@0..0)@0..17")?;
        parse(parse_expr, "(a a a a a a a a a)", "(a@1..2 a@3..4 a@5..6 a@7..8 a@9..10 a@11..12 a@13..14 . #error(program is too big)@0..0)@0..19")?;
        parse(parse_expr, "(a a a a a a a a a a)", "(a@1..2 a@3..4 a@5..6 a@7..8 a@9..10 a@11..12 a@13..14 . #error(program is too big)@0..0)@0..21")?;
        parse(parse_expr, "(a a a a a a a a a a a)", "(a@1..2 a@3..4 a@5..6 a@7..8 a@9..10 a@11..12 a@13..14 . #error(program is too big)@0..0)@0..23")?;
        Ok(())
    }

    #[test]
    fn toplevel() -> Result<(), Box<dyn Error>> {
        parse(
            parse_toplevel,
            r#"
                (hello world)
                (abc)
            "#,
            "(hello@18..23 world@24..29)@17..30\n(abc@48..51)@47..52@0..65",
        )?;
        Ok(())
    }
}
