use super::{Ast, NodeId, NodeKind};

impl Ast {
    pub fn child(&self, parent: NodeId, kind: NodeKind) -> Option<NodeId> {
        self.children(parent)
            .iter()
            .find(|&&child| self.kind(child) == kind)
            .copied()
    }

    pub fn walk(&self, parent: NodeId) -> Walk<'_> {
        Walk {
            ast: self,
            parent,
            index: 0,
        }
    }
}

/// An iterator over a node's children, with convenience methods for accessing those children.
#[derive(Clone)]
pub struct Walk<'a> {
    ast: &'a Ast,
    parent: NodeId,
    index: usize,
}

impl<'a> Walk<'a> {
    /// Walk to the first non-Nil, non-Error, non-Token node.
    pub fn node(&mut self) -> Option<NodeId> {
        while let Some(id) = self.next() {
            if !matches!(
                self.ast.kind(id),
                NodeKind::Nil | NodeKind::Token | NodeKind::Error
            ) {
                return Some(id);
            }
        }

        None
    }

    /// Walk to the next [`node`][`Self::node`] of the given kind.
    pub fn node_of(&mut self, kind: NodeKind) -> Option<NodeId> {
        while let Some(id) = self.node() {
            if self.ast.kind(id) == kind {
                return Some(id);
            }
        }

        None
    }

    /// Find the first node of the given kind. This does not advance the iterator.
    pub fn get(&self, kind: NodeKind) -> Option<NodeId> {
        self.clone().find(|&id| self.ast.kind(id) == kind)
    }
}

impl<'a> Iterator for Walk<'a> {
    type Item = NodeId;

    fn next(&mut self) -> Option<Self::Item> {
        let children = self.ast.children(self.parent);
        if self.index < children.len() {
            let index = self.index;
            self.index += 1;
            Some(children[index])
        } else {
            None
        }
    }
}
