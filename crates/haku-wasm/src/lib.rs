#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use core::{alloc::Layout, slice};

use alloc::{boxed::Box, vec::Vec};
use haku::{
    ast::Ast,
    bytecode::{Chunk, Defs, DefsImage},
    compiler::{compile_expr, ClosureSpec, CompileError, Compiler, Source},
    diagnostic::Diagnostic,
    lexer::{lex, Lexer},
    parser::{self, IntoAstError, Parser},
    render::{
        tiny_skia::{Pixmap, PremultipliedColorU8},
        Renderer, RendererLimits,
    },
    source::SourceCode,
    system::{ChunkId, System, SystemImage},
    token::Lexis,
    value::{Closure, Ref, Value},
    vm::{Exception, Vm, VmImage, VmLimits},
};
use log::{debug, info};

pub mod logging;
#[cfg(not(feature = "std"))]
mod panicking;

#[global_allocator]
static ALLOCATOR: dlmalloc::GlobalDlmalloc = dlmalloc::GlobalDlmalloc;

#[no_mangle]
unsafe extern "C" fn haku_alloc(size: usize, align: usize) -> *mut u8 {
    alloc::alloc::alloc(Layout::from_size_align(size, align).unwrap())
}

#[no_mangle]
unsafe extern "C" fn haku_free(ptr: *mut u8, size: usize, align: usize) {
    alloc::alloc::dealloc(ptr, Layout::from_size_align(size, align).unwrap())
}

#[derive(Debug, Clone, Copy)]
struct Limits {
    max_source_code_len: usize,
    max_chunks: usize,
    max_defs: usize,
    max_tokens: usize,
    max_parser_events: usize,
    ast_capacity: usize,
    chunk_capacity: usize,
    stack_capacity: usize,
    call_stack_capacity: usize,
    ref_capacity: usize,
    fuel: usize,
    memory: usize,
    pixmap_stack_capacity: usize,
    transform_stack_capacity: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_source_code_len: 65536,
            max_chunks: 2,
            max_defs: 256,
            max_tokens: 1024,
            max_parser_events: 1024,
            ast_capacity: 1024,
            chunk_capacity: 65536,
            stack_capacity: 1024,
            call_stack_capacity: 256,
            ref_capacity: 2048,
            fuel: 65536,
            memory: 1024 * 1024,
            pixmap_stack_capacity: 4,
            transform_stack_capacity: 16,
        }
    }
}

#[no_mangle]
extern "C" fn haku_limits_new() -> *mut Limits {
    let ptr = Box::leak(Box::new(Limits::default())) as *mut _;
    debug!("created limits: {ptr:?}");
    ptr
}

#[no_mangle]
unsafe extern "C" fn haku_limits_destroy(limits: *mut Limits) {
    debug!("destroying limits: {limits:?}");
    drop(Box::from_raw(limits))
}

macro_rules! limit_setter {
    ($name:tt) => {
        paste::paste! {
            #[no_mangle]
            unsafe extern "C" fn [<haku_limits_set_ $name>](limits: *mut Limits, value: usize) {
                debug!("set limit {} = {value}", stringify!($name));

                let limits = &mut *limits;
                limits.$name = value;
            }
        }
    };
}

limit_setter!(max_source_code_len);
limit_setter!(max_chunks);
limit_setter!(max_defs);
limit_setter!(max_tokens);
limit_setter!(max_parser_events);
limit_setter!(ast_capacity);
limit_setter!(chunk_capacity);
limit_setter!(stack_capacity);
limit_setter!(call_stack_capacity);
limit_setter!(ref_capacity);
limit_setter!(fuel);
limit_setter!(memory);
limit_setter!(pixmap_stack_capacity);
limit_setter!(transform_stack_capacity);

#[derive(Debug, Clone)]
struct Instance {
    limits: Limits,

    system: System,
    system_image: SystemImage,
    defs: Defs,
    defs_image: DefsImage,
    vm: Vm,
    vm_image: VmImage,

    value: Value,
    exception: Option<Exception>,
}

#[no_mangle]
unsafe extern "C" fn haku_instance_new(limits: *const Limits) -> *mut Instance {
    let limits = *limits;
    debug!("creating new instance with limits: {limits:?}");

    let system = System::new(limits.max_chunks);

    let defs = Defs::new(limits.max_defs);
    let vm = Vm::new(
        &defs,
        &VmLimits {
            stack_capacity: limits.stack_capacity,
            call_stack_capacity: limits.call_stack_capacity,
            ref_capacity: limits.ref_capacity,
            fuel: limits.fuel,
            memory: limits.memory,
        },
    );

    let system_image = system.image();
    let defs_image = defs.image();
    let vm_image = vm.image();

    let instance = Box::new(Instance {
        limits,
        system,
        system_image,
        defs,
        defs_image,
        vm,
        vm_image,
        value: Value::Nil,
        exception: None,
    });

    let ptr = Box::leak(instance) as *mut _;
    debug!("created instance: {ptr:?}");
    ptr
}

#[no_mangle]
unsafe extern "C" fn haku_instance_destroy(instance: *mut Instance) {
    debug!("destroying instance: {instance:?}");
    drop(Box::from_raw(instance));
}

#[no_mangle]
unsafe extern "C" fn haku_reset(instance: *mut Instance) {
    debug!("resetting instance: {instance:?}");
    let instance = &mut *instance;
    instance.system.restore_image(&instance.system_image);
    instance.defs.restore_image(&instance.defs_image);
}

#[no_mangle]
unsafe extern "C" fn haku_reset_vm(instance: *mut Instance) {
    debug!("resetting instance VM: {instance:?}");
    let instance = &mut *instance;
    instance.vm.restore_image(&instance.vm_image);
}

#[no_mangle]
unsafe extern "C" fn haku_has_exception(instance: *mut Instance) -> bool {
    (*instance).exception.is_some()
}

#[no_mangle]
unsafe extern "C" fn haku_exception_message(instance: *const Instance) -> *const u8 {
    (*instance).exception.as_ref().unwrap().message.as_ptr()
}

#[no_mangle]
unsafe extern "C" fn haku_exception_message_len(instance: *const Instance) -> u32 {
    (*instance).exception.as_ref().unwrap().message.len() as u32
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
enum StatusCode {
    Ok,
    SourceCodeTooLong,
    TooManyTokens,
    TooManyAstNodes,
    TooManyParserEvents,
    ParserUnbalancedEvents,
    ChunkTooBig,
    DiagnosticsEmitted,
    TooManyChunks,
    OutOfRefSlots,
    EvalException,
    RenderException,
}

#[no_mangle]
extern "C" fn haku_is_ok(code: StatusCode) -> bool {
    code == StatusCode::Ok
}

#[no_mangle]
extern "C" fn haku_is_diagnostics_emitted(code: StatusCode) -> bool {
    code == StatusCode::DiagnosticsEmitted
}

#[no_mangle]
extern "C" fn haku_is_exception(code: StatusCode) -> bool {
    matches!(
        code,
        StatusCode::EvalException | StatusCode::RenderException
    )
}

#[no_mangle]
extern "C" fn haku_status_string(code: StatusCode) -> *const i8 {
    match code {
        StatusCode::Ok => c"ok",
        StatusCode::SourceCodeTooLong => c"source code is too long",
        StatusCode::TooManyTokens => c"source code has too many tokens",
        StatusCode::TooManyAstNodes => c"source code has too many AST nodes",
        StatusCode::TooManyParserEvents => c"source code has too many parser events",
        StatusCode::ParserUnbalancedEvents => c"parser produced unbalanced events",
        StatusCode::ChunkTooBig => c"compiled bytecode is too large",
        StatusCode::DiagnosticsEmitted => c"diagnostics were emitted",
        StatusCode::TooManyChunks => c"too many registered bytecode chunks",
        StatusCode::OutOfRefSlots => c"out of ref slots (did you forget to restore the VM image?)",
        StatusCode::EvalException => c"an exception occurred while evaluating your code",
        StatusCode::RenderException => c"an exception occurred while rendering your brush",
    }
    .as_ptr()
}

#[derive(Debug, Default)]
enum BrushState {
    #[default]
    Default,
    Ready(ChunkId, ClosureSpec),
}

#[derive(Debug, Default)]
struct Brush {
    diagnostics: Vec<Diagnostic>,
    state: BrushState,
}

#[no_mangle]
extern "C" fn haku_brush_new() -> *mut Brush {
    let ptr = Box::leak(Box::new(Brush::default())) as *mut _;
    debug!("created brush: {ptr:?}");
    ptr
}

#[no_mangle]
unsafe extern "C" fn haku_brush_destroy(brush: *mut Brush) {
    debug!("destroying brush: {brush:?}");
    drop(Box::from_raw(brush))
}

#[no_mangle]
unsafe extern "C" fn haku_num_diagnostics(brush: *const Brush) -> u32 {
    (*brush).diagnostics.len() as u32
}

#[no_mangle]
unsafe extern "C" fn haku_diagnostic_start(brush: *const Brush, index: u32) -> u32 {
    (*brush).diagnostics[index as usize].span().start
}

#[no_mangle]
unsafe extern "C" fn haku_diagnostic_end(brush: *const Brush, index: u32) -> u32 {
    (*brush).diagnostics[index as usize].span().end
}

#[no_mangle]
unsafe extern "C" fn haku_diagnostic_message(brush: *const Brush, index: u32) -> *const u8 {
    (*brush).diagnostics[index as usize].message().as_ptr()
}

#[no_mangle]
unsafe extern "C" fn haku_diagnostic_message_len(brush: *const Brush, index: u32) -> u32 {
    (*brush).diagnostics[index as usize].message().len() as u32
}

#[no_mangle]
unsafe extern "C" fn haku_compile_brush(
    instance: *mut Instance,
    out_brush: *mut Brush,
    code_len: u32,
    code: *const u8,
) -> StatusCode {
    info!("compiling brush");

    let instance = &mut *instance;
    let brush = &mut *out_brush;

    *brush = Brush::default();

    let code = core::str::from_utf8(slice::from_raw_parts(code, code_len as usize))
        .expect("invalid UTF-8");
    let Some(code) = SourceCode::limited_len(code, instance.limits.max_source_code_len as u32)
    else {
        return StatusCode::SourceCodeTooLong;
    };

    debug!("compiling: lexing");

    let mut lexer = Lexer::new(Lexis::new(instance.limits.max_tokens), code);
    if lex(&mut lexer).is_err() {
        info!("compiling failed: too many tokens");
        return StatusCode::TooManyTokens;
    };

    debug!(
        "compiling: lexed successfully to {} tokens",
        lexer.lexis.len()
    );
    debug!("compiling: parsing");

    let mut ast = Ast::new(instance.limits.ast_capacity);
    let mut parser = Parser::new(
        &lexer.lexis,
        &haku::parser::ParserLimits {
            max_events: instance.limits.max_parser_events,
        },
    );
    parser::toplevel(&mut parser);
    let (root, mut parser_diagnostics) = match parser.into_ast(&mut ast) {
        Ok((r, d)) => (r, d),
        Err(IntoAstError::NodeAlloc(_)) => {
            info!("compiling failed: too many AST nodes");
            return StatusCode::TooManyAstNodes;
        }
        Err(IntoAstError::TooManyEvents) => {
            info!("compiling failed: too many parser events");
            return StatusCode::TooManyParserEvents;
        }
        Err(IntoAstError::UnbalancedEvents) => {
            info!("compiling failed: parser produced unbalanced events");
            return StatusCode::ParserUnbalancedEvents;
        }
    };

    debug!(
        "compiling: parsed successfully into {} AST nodes",
        ast.len()
    );

    let src = Source {
        code,
        ast: &ast,
        system: &instance.system,
    };

    let mut chunk = Chunk::new(instance.limits.chunk_capacity).unwrap();
    let mut compiler = Compiler::new(&mut instance.defs, &mut chunk);
    if let Err(error) = compile_expr(&mut compiler, &src, root) {
        match error {
            CompileError::Emit => {
                info!("compiling failed: chunk overflowed while emitting code");
                return StatusCode::ChunkTooBig;
            }
        }
    }
    let closure_spec = compiler.closure_spec();

    let mut diagnostics = lexer.diagnostics;
    diagnostics.append(&mut parser_diagnostics);
    diagnostics.append(&mut compiler.diagnostics);
    if !diagnostics.is_empty() {
        brush.diagnostics = diagnostics;
        debug!("compiling failed: diagnostics were emitted");
        return StatusCode::DiagnosticsEmitted;
    }

    debug!(
        "compiling: chunk has {} bytes of bytecode",
        chunk.bytecode.len()
    );
    debug!("compiling: {closure_spec:?}");

    let chunk_id = match instance.system.add_chunk(chunk) {
        Ok(chunk_id) => chunk_id,
        Err(_) => return StatusCode::TooManyChunks,
    };
    brush.state = BrushState::Ready(chunk_id, closure_spec);

    info!("brush compiled into {chunk_id:?}");

    StatusCode::Ok
}

struct PixmapLock {
    pixmap: Pixmap,
}

#[no_mangle]
extern "C" fn haku_pixmap_new(width: u32, height: u32) -> *mut PixmapLock {
    let ptr = Box::leak(Box::new(PixmapLock {
        pixmap: Pixmap::new(width, height).expect("invalid pixmap size"),
    })) as *mut _;
    debug!("created pixmap with size {width}x{height}: {ptr:?}");
    ptr
}

#[no_mangle]
unsafe extern "C" fn haku_pixmap_destroy(pixmap: *mut PixmapLock) {
    debug!("destroying pixmap: {pixmap:?}");
    drop(Box::from_raw(pixmap))
}

#[no_mangle]
unsafe extern "C" fn haku_pixmap_data(pixmap: *mut PixmapLock) -> *mut u8 {
    let pixmap = &mut (*pixmap).pixmap;
    pixmap.pixels_mut().as_mut_ptr() as *mut u8
}

#[no_mangle]
unsafe extern "C" fn haku_pixmap_clear(pixmap: *mut PixmapLock) {
    let pixmap = &mut (*pixmap).pixmap;
    pixmap.pixels_mut().fill(PremultipliedColorU8::TRANSPARENT);
}

#[no_mangle]
unsafe extern "C" fn haku_eval_brush(instance: *mut Instance, brush: *const Brush) -> StatusCode {
    let instance = &mut *instance;
    let brush = &*brush;

    let BrushState::Ready(chunk_id, closure_spec) = brush.state else {
        panic!("brush is not compiled and ready to be used");
    };

    debug!("applying defs");
    instance.vm.apply_defs(&instance.defs);

    let Ok(closure_id) = instance
        .vm
        .create_ref(Ref::Closure(Closure::chunk(chunk_id, closure_spec)))
    else {
        return StatusCode::OutOfRefSlots;
    };

    debug!("resetting exception");
    instance.exception = None;
    instance.value = match instance.vm.run(&instance.system, closure_id) {
        Ok(value) => value,
        Err(exn) => {
            debug!("setting exception {exn:?}");
            instance.exception = Some(exn);
            return StatusCode::EvalException;
        }
    };

    StatusCode::Ok
}

#[no_mangle]
unsafe extern "C" fn haku_render_value(
    instance: *mut Instance,
    pixmap: *mut PixmapLock,
    translation_x: f32,
    translation_y: f32,
) -> StatusCode {
    let instance = &mut *instance;
    debug!("resetting exception");
    instance.exception = None;

    debug!("will render value: {:?}", instance.value);

    let pixmap_locked = &mut (*pixmap).pixmap;

    let mut renderer = Renderer::new(
        pixmap_locked,
        &RendererLimits {
            pixmap_stack_capacity: instance.limits.pixmap_stack_capacity,
            transform_stack_capacity: instance.limits.transform_stack_capacity,
        },
    );
    renderer.translate(translation_x, translation_y);
    match renderer.render(&instance.vm, instance.value) {
        Ok(()) => (),
        Err(exn) => {
            instance.exception = Some(exn);
            instance.vm.restore_image(&instance.vm_image);
            return StatusCode::RenderException;
        }
    }

    StatusCode::Ok
}
