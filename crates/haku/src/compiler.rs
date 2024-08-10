use core::{
    error::Error,
    fmt::{self, Display},
};

use alloc::vec::Vec;

use crate::{
    bytecode::{Chunk, DefError, DefId, Defs, EmitError, Opcode, CAPTURE_CAPTURE, CAPTURE_LOCAL},
    sexp::{Ast, NodeId, NodeKind, Span},
    system::System,
};

pub struct Source<'a> {
    pub code: &'a str,
    pub ast: &'a Ast,
    pub system: &'a System,
}

#[derive(Debug, Clone, Copy)]
pub struct Diagnostic {
    pub span: Span,
    pub message: &'static str,
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

pub struct Compiler<'a, 'b> {
    pub defs: &'a mut Defs,
    pub chunk: &'b mut Chunk,
    pub diagnostics: Vec<Diagnostic>,
    scopes: Vec<Scope<'a>>,
}

impl<'a, 'b> Compiler<'a, 'b> {
    pub fn new(defs: &'a mut Defs, chunk: &'b mut Chunk) -> Self {
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

    pub fn diagnose(&mut self, diagnostic: Diagnostic) {
        if self.diagnostics.len() >= self.diagnostics.capacity() {
            return;
        }

        if self.diagnostics.len() == self.diagnostics.capacity() - 1 {
            self.diagnostics.push(Diagnostic {
                span: Span::new(0, 0),
                message: "too many diagnostics emitted, stopping", // hello clangd!
            })
        } else {
            self.diagnostics.push(diagnostic);
        }
    }
}

type CompileResult<T = ()> = Result<T, CompileError>;

pub fn compile_expr<'a>(
    c: &mut Compiler<'a, '_>,
    src: &Source<'a>,
    node_id: NodeId,
) -> CompileResult {
    let node = src.ast.get(node_id);
    match node.kind {
        NodeKind::Eof => unreachable!("eof node should never be emitted"),

        NodeKind::Nil => compile_nil(c),
        NodeKind::Ident => compile_ident(c, src, node_id),
        NodeKind::Number => compile_number(c, src, node_id),
        NodeKind::List(_, _) => compile_list(c, src, node_id),
        NodeKind::Toplevel(_) => compile_toplevel(c, src, node_id),

        NodeKind::Error(message) => {
            c.diagnose(Diagnostic {
                span: node.span,
                message,
            });
            Ok(())
        }
    }
}

fn compile_nil(c: &mut Compiler<'_, '_>) -> CompileResult {
    c.chunk.emit_opcode(Opcode::Nil)?;

    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CaptureError;

fn find_variable(
    c: &mut Compiler<'_, '_>,
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

fn compile_ident<'a>(c: &mut Compiler<'a, '_>, src: &Source<'a>, node_id: NodeId) -> CompileResult {
    let ident = src.ast.get(node_id);
    let name = ident.span.slice(src.code);

    match name {
        "false" => _ = c.chunk.emit_opcode(Opcode::False)?,
        "true" => _ = c.chunk.emit_opcode(Opcode::True)?,
        _ => match find_variable(c, name, c.scopes.len() - 1) {
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
                    c.diagnose(Diagnostic {
                        span: ident.span,
                        message: "undefined variable",
                    });
                }
            }
            Err(CaptureError) => {
                c.diagnose(Diagnostic {
                    span: ident.span,
                    message: "too many variables captured from outer functions in this scope",
                });
            }
        },
    }

    Ok(())
}

fn compile_number(c: &mut Compiler<'_, '_>, src: &Source<'_>, node_id: NodeId) -> CompileResult {
    let node = src.ast.get(node_id);

    let literal = node.span.slice(src.code);
    let float: f32 = literal
        .parse()
        .expect("the parser should've gotten us a string parsable by the stdlib");

    c.chunk.emit_opcode(Opcode::Number)?;
    c.chunk.emit_f32(float)?;

    Ok(())
}

fn compile_list<'a>(c: &mut Compiler<'a, '_>, src: &Source<'a>, node_id: NodeId) -> CompileResult {
    let NodeKind::List(function_id, args) = src.ast.get(node_id).kind else {
        unreachable!("compile_list expects a List");
    };

    let function = src.ast.get(function_id);
    let name = function.span.slice(src.code);

    if function.kind == NodeKind::Ident {
        match name {
            "fn" => return compile_fn(c, src, args),
            "if" => return compile_if(c, src, args),
            "let" => return compile_let(c, src, args),
            _ => (),
        };
    }

    let mut argument_count = 0;
    let mut args = args;
    while let NodeKind::List(head, tail) = src.ast.get(args).kind {
        compile_expr(c, src, head)?;
        argument_count += 1;
        args = tail;
    }

    let argument_count = u8::try_from(argument_count).unwrap_or_else(|_| {
        c.diagnose(Diagnostic {
            span: src.ast.get(args).span,
            message: "function call has too many arguments",
        });
        0
    });

    if let (NodeKind::Ident, Some(index)) = (function.kind, (src.system.resolve_fn)(name)) {
        c.chunk.emit_opcode(Opcode::System)?;
        c.chunk.emit_u8(index)?;
        c.chunk.emit_u8(argument_count)?;
    } else {
        // This is a bit of an oddity: we only emit the function expression _after_ the arguments,
        // but since the language is effectless this doesn't matter in practice.
        // It makes for less code in the compiler and the VM.
        compile_expr(c, src, function_id)?;
        c.chunk.emit_opcode(Opcode::Call)?;
        c.chunk.emit_u8(argument_count)?;
    }

    Ok(())
}

struct WalkList {
    current: NodeId,
    ok: bool,
}

impl WalkList {
    fn new(start: NodeId) -> Self {
        Self {
            current: start,
            ok: true,
        }
    }

    fn expect_arg(
        &mut self,
        c: &mut Compiler<'_, '_>,
        src: &Source<'_>,
        message: &'static str,
    ) -> NodeId {
        if !self.ok {
            return NodeId::NIL;
        }

        if let NodeKind::List(expr, tail) = src.ast.get(self.current).kind {
            self.current = tail;
            expr
        } else {
            c.diagnose(Diagnostic {
                span: src.ast.get(self.current).span,
                message,
            });
            self.ok = false;
            NodeId::NIL
        }
    }

    fn expect_nil(&mut self, c: &mut Compiler<'_, '_>, src: &Source<'_>, message: &'static str) {
        if src.ast.get(self.current).kind != NodeKind::Nil {
            c.diagnose(Diagnostic {
                span: src.ast.get(self.current).span,
                message,
            });
            // NOTE: Don't set self.ok to false, since this is not a fatal error.
            // The nodes returned previously are valid and therefore it's safe to operate on them.
            // Just having extra arguments shouldn't inhibit emitting additional diagnostics in
            // the expression.
        }
    }
}

fn compile_if<'a>(c: &mut Compiler<'a, '_>, src: &Source<'a>, args: NodeId) -> CompileResult {
    let mut list = WalkList::new(args);

    let condition = list.expect_arg(c, src, "missing `if` condition");
    let if_true = list.expect_arg(c, src, "missing `if` true branch");
    let if_false = list.expect_arg(c, src, "missing `if` false branch");
    list.expect_nil(c, src, "extra arguments after `if` false branch");

    if !list.ok {
        return Ok(());
    }

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

fn compile_let<'a>(c: &mut Compiler<'a, '_>, src: &Source<'a>, args: NodeId) -> CompileResult {
    let mut list = WalkList::new(args);

    let binding_list = list.expect_arg(c, src, "missing `let` binding list ((x 1) (y 2) ...)");
    let expr = list.expect_arg(c, src, "missing expression to `let` names into");
    list.expect_nil(c, src, "extra arguments after `let` expression");

    if !list.ok {
        return Ok(());
    }

    // NOTE: Our `let` behaves like `let*` from Lisps.
    // This is because this is generally the more intuitive behaviour with how variable declarations
    // work in traditional imperative languages.
    // We do not offer an alternative to Lisp `let` to be as minimal as possible.

    let mut current = binding_list;
    let mut local_count: usize = 0;
    while let NodeKind::List(head, tail) = src.ast.get(current).kind {
        if !matches!(src.ast.get(head).kind, NodeKind::List(_, _)) {
            c.diagnose(Diagnostic {
                span: src.ast.get(head).span,
                message: "`let` binding expected, like (x 1)",
            });
            current = tail;
            continue;
        }

        let mut list = WalkList::new(head);
        let ident = list.expect_arg(c, src, "binding name expected");
        let value = list.expect_arg(c, src, "binding value expected");
        list.expect_nil(c, src, "extra expressions after `let` binding value");

        if src.ast.get(ident).kind != NodeKind::Ident {
            c.diagnose(Diagnostic {
                span: src.ast.get(ident).span,
                message: "binding name must be an identifier",
            });
        }

        // NOTE: Compile expression _before_ putting the value into scope.
        // This is so that the variable cannot refer to itself, as it is yet to be declared.
        compile_expr(c, src, value)?;

        let name = src.ast.get(ident).span.slice(src.code);
        let scope = c.scopes.last_mut().unwrap();
        if scope.locals.len() >= u8::MAX as usize {
            c.diagnose(Diagnostic {
                span: src.ast.get(ident).span,
                message: "too many names bound in this function at a single time",
            });
        } else {
            scope.locals.push(Local { name });
        }

        local_count += 1;
        current = tail;
    }

    compile_expr(c, src, expr)?;

    let scope = c.scopes.last_mut().unwrap();
    scope
        .locals
        .resize_with(scope.locals.len() - local_count, || unreachable!());

    // NOTE: If we reach more than 255 locals declared in our `let`, we should've gotten
    // a diagnostic emitted in the `while` loop beforehand.
    let local_count = u8::try_from(local_count).unwrap_or(0);
    c.chunk.emit_opcode(Opcode::DropLet)?;
    c.chunk.emit_u8(local_count)?;

    Ok(())
}

fn compile_fn<'a>(c: &mut Compiler<'a, '_>, src: &Source<'a>, args: NodeId) -> CompileResult {
    let mut list = WalkList::new(args);

    let param_list = list.expect_arg(c, src, "missing function parameters");
    let body = list.expect_arg(c, src, "missing function body");
    list.expect_nil(c, src, "extra arguments after function body");

    if !list.ok {
        return Ok(());
    }

    let mut locals = Vec::new();
    let mut current = param_list;
    while let NodeKind::List(ident, tail) = src.ast.get(current).kind {
        if let NodeKind::Ident = src.ast.get(ident).kind {
            locals.push(Local {
                name: src.ast.get(ident).span.slice(src.code),
            })
        } else {
            c.diagnose(Diagnostic {
                span: src.ast.get(ident).span,
                message: "function parameters must be identifiers",
            })
        }
        current = tail;
    }

    let param_count = u8::try_from(locals.len()).unwrap_or_else(|_| {
        c.diagnose(Diagnostic {
            span: src.ast.get(param_list).span,
            message: "too many function parameters",
        });
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
    let capture_count = u8::try_from(scope.captures.len()).unwrap_or_else(|_| {
        c.diagnose(Diagnostic {
            span: src.ast.get(body).span,
            message: "function refers to too many variables from the outer function",
        });
        0
    });
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

fn compile_toplevel<'a>(
    c: &mut Compiler<'a, '_>,
    src: &Source<'a>,
    node_id: NodeId,
) -> CompileResult {
    let NodeKind::Toplevel(mut current) = src.ast.get(node_id).kind else {
        unreachable!("compile_toplevel expects a Toplevel");
    };

    def_prepass(c, src, current)?;

    let mut had_result = false;
    while let NodeKind::List(expr, tail) = src.ast.get(current).kind {
        match compile_toplevel_expr(c, src, expr)? {
            ToplevelExpr::Def => (),
            ToplevelExpr::Result => had_result = true,
        }

        if had_result && src.ast.get(tail).kind != NodeKind::Nil {
            c.diagnose(Diagnostic {
                span: src.ast.get(tail).span,
                message: "result value may not be followed by anything else",
            });
            break;
        }

        current = tail;
    }

    if !had_result {
        c.chunk.emit_opcode(Opcode::Nil)?;
    }
    c.chunk.emit_opcode(Opcode::Return)?;

    Ok(())
}

fn def_prepass<'a>(c: &mut Compiler<'a, '_>, src: &Source<'a>, node_id: NodeId) -> CompileResult {
    // This is a bit of a pattern matching tapeworm, but Rust unfortunately doesn't have `if let`
    // chains yet to make this more readable.
    let mut current = node_id;
    while let NodeKind::List(expr, tail) = src.ast.get(current).kind {
        if let NodeKind::List(head_id, tail_id) = src.ast.get(expr).kind {
            let head = src.ast.get(head_id);
            let name = head.span.slice(src.code);
            if head.kind == NodeKind::Ident && name == "def" {
                if let NodeKind::List(ident_id, _) = src.ast.get(tail_id).kind {
                    let ident = src.ast.get(ident_id);
                    if ident.kind == NodeKind::Ident {
                        let name = ident.span.slice(src.code);
                        match c.defs.add(name) {
                            Ok(_) => (),
                            Err(DefError::Exists) => c.diagnose(Diagnostic {
                                span: ident.span,
                                message: "redefinitions of defs are not allowed",
                            }),
                            Err(DefError::OutOfSpace) => c.diagnose(Diagnostic {
                                span: ident.span,
                                message: "too many defs",
                            }),
                        }
                    }
                }
            }
        }

        current = tail;
    }

    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ToplevelExpr {
    Def,
    Result,
}

fn compile_toplevel_expr<'a>(
    c: &mut Compiler<'a, '_>,
    src: &Source<'a>,
    node_id: NodeId,
) -> CompileResult<ToplevelExpr> {
    let node = src.ast.get(node_id);

    if let NodeKind::List(head_id, tail_id) = node.kind {
        let head = src.ast.get(head_id);
        if head.kind == NodeKind::Ident {
            let name = head.span.slice(src.code);
            if name == "def" {
                compile_def(c, src, tail_id)?;
                return Ok(ToplevelExpr::Def);
            }
        }
    }

    compile_expr(c, src, node_id)?;
    Ok(ToplevelExpr::Result)
}

fn compile_def<'a>(c: &mut Compiler<'a, '_>, src: &Source<'a>, args: NodeId) -> CompileResult {
    let mut list = WalkList::new(args);

    let ident = list.expect_arg(c, src, "missing definition name");
    let value = list.expect_arg(c, src, "missing definition value");
    list.expect_nil(c, src, "extra arguments after definition");

    if !list.ok {
        return Ok(());
    }

    let name = src.ast.get(ident).span.slice(src.code);
    // NOTE: def_prepass collects all definitions beforehand.
    // In case a def ends up not existing, that means we ran out of space for defs - so emit a
    // zero def instead.
    let def_id = c.defs.get(name).unwrap_or_default();

    compile_expr(c, src, value)?;
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
