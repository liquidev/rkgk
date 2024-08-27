use core::{error::Error, fmt::Display};

use alloc::vec::Vec;

use crate::source::Span;

pub mod dump;
pub mod walk;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId(u32);

impl NodeId {
    pub const NIL: NodeId = NodeId(0);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeKind {
    Nil,

    Token,

    Ident,
    Tag,
    Number,
    Color,
    List,

    Op,
    Unary,
    Binary,
    Call,
    ParenEmpty,
    Paren,
    Lambda,
    Params,
    Param,
    If,
    Let,

    Toplevel,

    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Node {
    pub span: Span,
    pub kind: NodeKind,
}

#[derive(Debug, Clone)]
pub struct Ast {
    kinds: Vec<NodeKind>,
    spans: Vec<Span>,
    children_spans: Vec<(u32, u32)>,
    children: Vec<NodeId>,
}

impl Ast {
    pub fn new(capacity: usize) -> Self {
        assert!(capacity >= 1, "there must be space for at least a nil node");
        assert!(capacity <= u32::MAX as usize);

        let mut ast = Self {
            kinds: Vec::with_capacity(capacity),
            spans: Vec::with_capacity(capacity),
            children_spans: Vec::with_capacity(capacity),
            children: Vec::new(),
        };

        ast.alloc(NodeKind::Nil, Span::new(0, 0)).unwrap();

        ast
    }

    pub fn alloc(&mut self, kind: NodeKind, span: Span) -> Result<NodeId, NodeAllocError> {
        if self.kinds.len() >= self.kinds.capacity() {
            return Err(NodeAllocError);
        }

        let index = self.kinds.len() as u32;
        self.kinds.push(kind);
        self.spans.push(span);
        self.children_spans.push((0, 0));
        Ok(NodeId(index))
    }

    // NOTE: This never produces a NodeAllocError, because there can more or less only ever be as many children for
    // nodes as there are nodes.
    pub fn alloc_children(&mut self, for_node: NodeId, children: &[NodeId]) {
        let start = self.children.len();
        self.children.extend_from_slice(children);
        let end = self.children.len();
        self.children_spans[for_node.0 as usize] = (start as u32, end as u32);
    }

    pub fn extend_span(&mut self, in_node: NodeId, end: u32) {
        self.spans[in_node.0 as usize].end = end;
    }

    pub fn kind(&self, id: NodeId) -> NodeKind {
        self.kinds[id.0 as usize]
    }

    pub fn span(&self, id: NodeId) -> Span {
        self.spans[id.0 as usize]
    }

    pub fn children(&self, id: NodeId) -> &[NodeId] {
        let (start, end) = self.children_spans[id.0 as usize];
        &self.children[start as usize..end as usize]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeAllocError;

impl Display for NodeAllocError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("too many nodes")
    }
}

impl Error for NodeAllocError {}
