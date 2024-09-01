use core::{
    error::Error,
    fmt::{self, Display},
};

use alloc::vec::Vec;

use crate::{
    ast::{Ast, NodeId, NodeKind},
    bytecode::{Chunk, DefError, Defs, EmitError, Opcode, CAPTURE_CAPTURE, CAPTURE_LOCAL},
    diagnostic::Diagnostic,
    source::SourceCode,
    system::{System, SystemFnArity},
};

pub struct Source<'a> {
    pub code: &'a SourceCode,
    pub ast: &'a Ast,
    pub system: &'a System,
}

#[derive(Debug, Clone, Copy)]
struct Local<'a> {
    name: &'a str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Variable {
    Local(u8),
    Captured(u8),
}

struct Scope<'a> {
    locals: Vec<Local<'a>>,
    captures: Vec<Variable>,
}

pub struct Compiler<'a> {
    pub defs: &'a mut Defs,
    pub chunk: &'a mut Chunk,
    pub diagnostics: Vec<Diagnostic>,
    scopes: Vec<Scope<'a>>,
}

#[derive(Debug, Clone, Copy)]
pub struct ClosureSpec {
    pub(crate) local_count: u8,
}

impl<'a> Compiler<'a> {
    pub fn new(defs: &'a mut Defs, chunk: &'a mut Chunk) -> Self {
        Self {
            defs,
            chunk,
            diagnostics: Vec::with_capacity(16),
            scopes: Vec::from_iter([Scope {
                locals: Vec::new(),
                captures: Vec::new(),
            }]),
        }
    }

    fn emit(&mut self, diagnostic: Diagnostic) {
        if self.diagnostics.len() < self.diagnostics.capacity() {
            self.diagnostics.push(diagnostic);
        }
    }

    pub fn closure_spec(&self) -> ClosureSpec {
        ClosureSpec {
            local_count: self
                .scopes
                .last()
                .unwrap()
                .locals
                .len()
                .try_into()
                .unwrap_or_default(),
        }
    }
}

type CompileResult<T = ()> = Result<T, CompileError>;

pub fn compile_expr<'a>(c: &mut Compiler<'a>, src: &Source<'a>, node_id: NodeId) -> CompileResult {
    match src.ast.kind(node_id) {
        // The nil node is special, as it inhabits node ID 0.
        NodeKind::Nil => {
            unreachable!("Nil node should never be emitted (ParenEmpty is used for nil literals)")
        }
        // Tokens are trivia and should never be emitted---they're only useful for error reporting.
        NodeKind::Token => unreachable!("Token node should never be emitted"),
        // Op nodes are only used to provide a searching anchor for the operator in Unary and Binary.
        NodeKind::Op => unreachable!("Op node should never be emitted"),
        // Params nodes are only used to provide a searching anchor for Lambda parameters.
        NodeKind::Params => unreachable!("Param node should never be emitted"),
        // Param nodes are only used to provide a searching anchor for identifiers in Params nodes,
        // as they may also contain commas and other trivia.
        NodeKind::Param => unreachable!("Param node should never be emitted"),

        NodeKind::Color => unsupported(c, src, node_id, "color literals are not implemented yet"),

        NodeKind::Ident => compile_ident(c, src, node_id),
        NodeKind::Number => compile_number(c, src, node_id),
        NodeKind::Tag => compile_tag(c, src, node_id),
        NodeKind::List => unsupported(c, src, node_id, "list literals are not implemented yet"),

        NodeKind::Unary => compile_unary(c, src, node_id),
        NodeKind::Binary => compile_binary(c, src, node_id),
        NodeKind::Call => compile_call(c, src, node_id),
        NodeKind::Paren => compile_paren(c, src, node_id),
        NodeKind::ParenEmpty => compile_nil(c),
        NodeKind::Lambda => compile_lambda(c, src, node_id),
        NodeKind::If => compile_if(c, src, node_id),
        NodeKind::Let => compile_let(c, src, node_id),

        NodeKind::Toplevel => compile_toplevel(c, src, node_id),

        // Error nodes are ignored, because for each error node an appropriate parser
        // diagnostic is emitted anyways.
        NodeKind::Error => Ok(()),
    }
}

fn unsupported(c: &mut Compiler, src: &Source, node_id: NodeId, message: &str) -> CompileResult {
    c.emit(Diagnostic::error(src.ast.span(node_id), message));
    Ok(())
}

fn compile_nil(c: &mut Compiler) -> CompileResult {
    c.chunk.emit_opcode(Opcode::Nil)?;

    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CaptureError;

fn find_variable(
    c: &mut Compiler,
    name: &str,
    scope_index: usize,
) -> Result<Option<Variable>, CaptureError> {
    let scope = &c.scopes[scope_index];
    if let Some(index) = scope.locals.iter().rposition(|l| l.name == name) {
        let index = u8::try_from(index).expect("a function must not declare more than 256 locals");
        Ok(Some(Variable::Local(index)))
    } else if scope_index > 0 {
        // Search upper scope if not found.
        if let Some(variable) = find_variable(c, name, scope_index - 1)? {
            let scope = &mut c.scopes[scope_index];
            let capture_index = scope
                .captures
                .iter()
                .position(|c| c == &variable)
                .unwrap_or_else(|| {
                    let new_index = scope.captures.len();
                    scope.captures.push(variable);
                    new_index
                });
            let capture_index = u8::try_from(capture_index).map_err(|_| CaptureError)?;
            Ok(Some(Variable::Captured(capture_index)))
        } else {
            Ok(None)
        }
    } else {
        Ok(None)
    }
}

fn compile_ident<'a>(c: &mut Compiler<'a>, src: &Source<'a>, node_id: NodeId) -> CompileResult {
    let span = src.ast.span(node_id);
    let name = span.slice(src.code);

    match find_variable(c, name, c.scopes.len() - 1) {
        Ok(Some(Variable::Local(index))) => {
            c.chunk.emit_opcode(Opcode::Local)?;
            c.chunk.emit_u8(index)?;
        }
        Ok(Some(Variable::Captured(index))) => {
            c.chunk.emit_opcode(Opcode::Capture)?;
            c.chunk.emit_u8(index)?;
        }
        Ok(None) => {
            if let Some(def_id) = c.defs.get(name) {
                c.chunk.emit_opcode(Opcode::Def)?;
                c.chunk.emit_u16(def_id.to_u16())?;
            } else {
                c.emit(Diagnostic::error(span, "undefined variable"));
            }
        }
        Err(CaptureError) => {
            c.emit(Diagnostic::error(
                span,
                "too many variables captured from outer functions in this scope",
            ));
        }
    };

    Ok(())
}

fn compile_number(c: &mut Compiler, src: &Source, node_id: NodeId) -> CompileResult {
    let literal = src.ast.span(node_id).slice(src.code);
    let float: f32 = literal
        .parse()
        .expect("the parser should've gotten us a string parsable by the stdlib");

    c.chunk.emit_opcode(Opcode::Number)?;
    c.chunk.emit_f32(float)?;

    Ok(())
}

fn compile_tag(c: &mut Compiler, src: &Source, node_id: NodeId) -> CompileResult {
    let tag = src.ast.span(node_id).slice(src.code);

    match tag {
        "False" => {
            c.chunk.emit_opcode(Opcode::False)?;
        }
        "True" => {
            c.chunk.emit_opcode(Opcode::True)?;
        }
        _ => {
            c.emit(Diagnostic::error(src.ast.span(node_id), "uppercased identifiers are reserved for future use; please start your identifiers with a lowercase letter instead"));
        }
    }

    Ok(())
}

fn compile_unary<'a>(c: &mut Compiler<'a>, src: &Source<'a>, node_id: NodeId) -> CompileResult {
    let mut walk = src.ast.walk(node_id);
    let Some(op) = walk.node() else { return Ok(()) };
    let Some(expr) = walk.node() else {
        return Ok(());
    };

    if src.ast.kind(op) != NodeKind::Op {
        return Ok(());
    }
    let name = src.ast.span(op).slice(src.code);

    compile_expr(c, src, expr)?;
    if let Some(index) = (src.system.resolve_fn)(SystemFnArity::Unary, name) {
        let argument_count = 1;
        c.chunk.emit_opcode(Opcode::System)?;
        c.chunk.emit_u8(index)?;
        c.chunk.emit_u8(argument_count)?;
    } else {
        c.emit(Diagnostic::error(
            src.ast.span(op),
            "this unary operator is currently unimplemented",
        ));
    }

    Ok(())
}

fn compile_binary<'a>(c: &mut Compiler<'a>, src: &Source<'a>, node_id: NodeId) -> CompileResult {
    let mut walk = src.ast.walk(node_id);
    let Some(left) = walk.node() else {
        return Ok(());
    };
    let Some(op) = walk.node() else { return Ok(()) };
    let Some(right) = walk.node() else {
        return Ok(());
    };

    if src.ast.kind(op) != NodeKind::Op {
        return Ok(());
    }
    let name = src.ast.span(op).slice(src.code);

    if name == "=" {
        c.emit(Diagnostic::error(
            src.ast.span(op),
            "defs `a = b` may only appear at the top level",
        ));
        return Ok(());
    }

    compile_expr(c, src, left)?;
    compile_expr(c, src, right)?;
    if let Some(index) = (src.system.resolve_fn)(SystemFnArity::Binary, name) {
        let argument_count = 2;
        c.chunk.emit_opcode(Opcode::System)?;
        c.chunk.emit_u8(index)?;
        c.chunk.emit_u8(argument_count)?;
    } else {
        c.emit(Diagnostic::error(
            src.ast.span(op),
            "this unary operator is currently unimplemented",
        ));
    }

    Ok(())
}

fn compile_call<'a>(c: &mut Compiler<'a>, src: &Source<'a>, node_id: NodeId) -> CompileResult {
    let mut walk = src.ast.walk(node_id);
    let Some(func) = walk.node() else {
        return Ok(());
    };
    let name = src.ast.span(func).slice(src.code);

    let mut argument_count = 0;
    while let Some(arg) = walk.node() {
        compile_expr(c, src, arg)?;
        argument_count += 1;
    }

    let argument_count = u8::try_from(argument_count).unwrap_or_else(|_| {
        c.emit(Diagnostic::error(
            src.ast.span(node_id),
            "function call has too many arguments",
        ));
        0
    });

    if let (NodeKind::Ident, Some(index)) = (
        src.ast.kind(func),
        (src.system.resolve_fn)(SystemFnArity::Nary, name),
    ) {
        c.chunk.emit_opcode(Opcode::System)?;
        c.chunk.emit_u8(index)?;
        c.chunk.emit_u8(argument_count)?;
    } else {
        // This is a bit of an oddity: we only emit the function expression _after_ the arguments,
        // but since the language is effectless this doesn't matter in practice.
        // It makes for a bit less code in the VM, since there's no need to find the function
        // down the stack - it's always on top.
        compile_expr(c, src, func)?;
        c.chunk.emit_opcode(Opcode::Call)?;
        c.chunk.emit_u8(argument_count)?;
    }

    Ok(())
}

fn compile_paren<'a>(c: &mut Compiler<'a>, src: &Source<'a>, node_id: NodeId) -> CompileResult {
    let Some(inner) = src.ast.walk(node_id).node() else {
        return Ok(());
    };

    compile_expr(c, src, inner)?;

    Ok(())
}

fn compile_if<'a>(c: &mut Compiler<'a>, src: &Source<'a>, node_id: NodeId) -> CompileResult {
    let mut walk = src.ast.walk(node_id);

    let Some(condition) = walk.node() else {
        return Ok(());
    };
    let Some(if_true) = walk.node() else {
        return Ok(());
    };
    let Some(if_false) = walk.node() else {
        return Ok(());
    };

    compile_expr(c, src, condition)?;

    c.chunk.emit_opcode(Opcode::JumpIfNot)?;
    let false_jump_offset_offset = c.chunk.emit_u16(0)?;

    compile_expr(c, src, if_true)?;
    c.chunk.emit_opcode(Opcode::Jump)?;
    let true_jump_offset_offset = c.chunk.emit_u16(0)?;

    let false_jump_offset = c.chunk.offset();
    c.chunk
        .patch_offset(false_jump_offset_offset, false_jump_offset);
    compile_expr(c, src, if_false)?;

    let true_jump_offset = c.chunk.offset();
    c.chunk
        .patch_offset(true_jump_offset_offset, true_jump_offset);

    Ok(())
}

fn compile_let<'a>(c: &mut Compiler<'a>, src: &Source<'a>, node_id: NodeId) -> CompileResult {
    let mut walk = src.ast.walk(node_id);

    let Some(ident) = walk.node() else {
        return Ok(());
    };
    let Some(expr) = walk.node() else {
        return Ok(());
    };
    let Some(then) = walk.node() else {
        return Ok(());
    };

    compile_expr(c, src, expr)?;
    let name = src.ast.span(ident).slice(src.code);
    let scope = c.scopes.last_mut().unwrap();
    let index = if scope.locals.len() >= u8::MAX as usize {
        c.emit(Diagnostic::error(
            src.ast.span(ident),
            "too many names bound in this function at a single time",
        ));

        // Don't emit the expression, because it will most likely contain errors due to this
        // `let` failing.
        return Ok(());
    } else {
        let index = scope.locals.len();
        scope.locals.push(Local { name });
        index as u8
    };
    c.chunk.emit_opcode(Opcode::SetLocal)?;
    c.chunk.emit_u8(index)?;

    compile_expr(c, src, then)?;

    Ok(())
}

fn compile_lambda<'a>(c: &mut Compiler<'a>, src: &Source<'a>, node_id: NodeId) -> CompileResult {
    let mut walk = src.ast.walk(node_id);
    let Some(params) = walk.node() else {
        return Ok(());
    };
    let Some(body) = walk.node() else {
        return Ok(());
    };

    let mut locals = Vec::new();
    let mut params_walk = src.ast.walk(params);
    while let Some(param) = params_walk.node() {
        locals.push(Local {
            name: src.ast.span(param).slice(src.code),
        });
    }

    let param_count = u8::try_from(locals.len()).unwrap_or_else(|_| {
        c.emit(Diagnostic::error(
            src.ast.span(params),
            "too many function parameters",
        ));
        0
    });

    c.chunk.emit_opcode(Opcode::Function)?;
    c.chunk.emit_u8(param_count)?;
    let after_offset = c.chunk.emit_u16(0)?;

    c.scopes.push(Scope {
        locals,
        captures: Vec::new(),
    });
    compile_expr(c, src, body)?;
    c.chunk.emit_opcode(Opcode::Return)?;

    let after = u16::try_from(c.chunk.bytecode.len()).expect("chunk is too large");
    c.chunk.patch_u16(after_offset, after);

    let scope = c.scopes.pop().unwrap();
    let local_count = u8::try_from(scope.locals.len()).unwrap_or_else(|_| {
        c.emit(Diagnostic::error(
            src.ast.span(body),
            "function contains too many local variables",
        ));
        0
    });
    let capture_count = u8::try_from(scope.captures.len()).unwrap_or_else(|_| {
        c.emit(Diagnostic::error(
            src.ast.span(body),
            "function refers to too many variables from its outer functions",
        ));
        0
    });
    c.chunk.emit_u8(local_count)?;
    c.chunk.emit_u8(capture_count)?;
    for capture in scope.captures {
        match capture {
            // TODO: There's probably a more clever way to encode these than wasting an entire byte
            // on what's effectively just a bool per each capture.
            Variable::Local(index) => {
                c.chunk.emit_u8(CAPTURE_LOCAL)?;
                c.chunk.emit_u8(index)?;
            }
            Variable::Captured(index) => {
                c.chunk.emit_u8(CAPTURE_CAPTURE)?;
                c.chunk.emit_u8(index)?;
            }
        }
    }

    Ok(())
}

fn compile_toplevel<'a>(c: &mut Compiler<'a>, src: &Source<'a>, node_id: NodeId) -> CompileResult {
    def_prepass(c, src, node_id)?;

    let mut walk = src.ast.walk(node_id);
    let mut result_expr = None;
    while let Some(toplevel_expr) = walk.node() {
        if let Some(result_expr) = result_expr {
            // TODO: This diagnostic should show you the expression after the result.
            c.emit(Diagnostic::error(
                src.ast.span(result_expr),
                "the result value must be the last thing in the program",
            ));
        }

        match compile_toplevel_expr(c, src, toplevel_expr)? {
            ToplevelExpr::Def => (),
            ToplevelExpr::Result if result_expr.is_none() => result_expr = Some(toplevel_expr),
            ToplevelExpr::Result => (),
        }
    }

    if result_expr.is_none() {
        c.chunk.emit_opcode(Opcode::Nil)?;
    }
    c.chunk.emit_opcode(Opcode::Return)?;

    Ok(())
}

fn def_prepass<'a>(c: &mut Compiler<'a>, src: &Source<'a>, toplevel: NodeId) -> CompileResult {
    let mut walk = src.ast.walk(toplevel);

    // This is a bit of a pattern matching tapeworm, but Rust unfortunately doesn't have `if let`
    // chains yet to make this more readable.
    while let Some(binary) = walk.node_of(NodeKind::Binary) {
        let mut binary_walk = src.ast.walk(binary);
        if let (Some(ident), Some(op)) = (binary_walk.node(), binary_walk.get(NodeKind::Op)) {
            if src.ast.span(op).slice(src.code) == "=" {
                let name = src.ast.span(ident).slice(src.code);
                match c.defs.add(name) {
                    Ok(_) => (),
                    Err(DefError::Exists) => c.emit(Diagnostic::error(
                        src.ast.span(ident),
                        "a def with this name already exists",
                    )),
                    Err(DefError::OutOfSpace) => {
                        c.emit(Diagnostic::error(src.ast.span(binary), "too many defs"))
                    }
                }
            }
        }
    }

    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ToplevelExpr {
    Def,
    Result,
}

fn compile_toplevel_expr<'a>(
    c: &mut Compiler<'a>,
    src: &Source<'a>,
    node_id: NodeId,
) -> CompileResult<ToplevelExpr> {
    if src.ast.kind(node_id) == NodeKind::Binary {
        if let Some(op) = src.ast.walk(node_id).get(NodeKind::Op) {
            if src.ast.span(op).slice(src.code) == "=" {
                compile_def(c, src, node_id)?;
                return Ok(ToplevelExpr::Def);
            }
        }
    }

    compile_expr(c, src, node_id)?;
    Ok(ToplevelExpr::Result)
}

fn compile_def<'a>(c: &mut Compiler<'a>, src: &Source<'a>, node_id: NodeId) -> CompileResult {
    let mut walk = src.ast.walk(node_id);
    let Some(left) = walk.node() else {
        return Ok(());
    };
    let Some(_op) = walk.node() else {
        return Ok(());
    };
    let Some(right) = walk.node() else {
        return Ok(());
    };

    if src.ast.kind(left) != NodeKind::Ident {
        c.emit(Diagnostic::error(
            src.ast.span(left),
            "def name (identifier) expected",
        ));
    }

    let name = src.ast.span(left).slice(src.code);
    // NOTE: def_prepass collects all definitions beforehand.
    // In case a def ends up not existing, that means we ran out of space for defs - so emit a
    // zero def instead.
    let def_id = c.defs.get(name).unwrap_or_default();

    compile_expr(c, src, right)?;
    c.chunk.emit_opcode(Opcode::SetDef)?;
    c.chunk.emit_u16(def_id.to_u16())?;

    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompileError {
    Emit,
}

impl From<EmitError> for CompileError {
    fn from(_: EmitError) -> Self {
        Self::Emit
    }
}

impl Display for CompileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            CompileError::Emit => "bytecode is too big",
        })
    }
}

impl Error for CompileError {}
