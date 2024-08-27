use alloc::string::String;
use core::fmt::Write;

use crate::{ast::NodeKind, source::SourceCode};

use super::{Ast, NodeId};

pub fn dump(ast: &Ast, node: NodeId, code: Option<&SourceCode>) -> String {
    let mut result = String::new();

    fn rec(ast: &Ast, node: NodeId, code: Option<&SourceCode>, result: &mut String, depth: usize) {
        for _ in 0..depth {
            result.push_str("    ");
        }

        write!(result, "{:?} @ {:?}", ast.kind(node), ast.span(node)).unwrap();
        if let Some(code) = code {
            if ast.kind(node) == NodeKind::Token {
                write!(result, " {:?}", ast.span(node).slice(code)).unwrap();
            }
        }
        writeln!(result).unwrap();
        for &child in ast.children(node) {
            rec(ast, child, code, result, depth + 1);
        }
    }

    rec(ast, node, code, &mut result, 0);

    // Remove the trailing newline.
    result.pop();

    result
}
