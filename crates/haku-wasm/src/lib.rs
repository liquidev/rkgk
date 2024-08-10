#![no_std]

extern crate alloc;

use core::{alloc::Layout, ffi::CStr, slice, str};

use alloc::{boxed::Box, vec::Vec};
use haku::{
    bytecode::{Chunk, Defs, DefsImage},
    compiler::{compile_expr, CompileError, Compiler, Diagnostic, Source},
    render::{Bitmap, Renderer, RendererLimits},
    sexp::{self, parse_toplevel, Ast, Parser},
    system::{ChunkId, System, SystemImage},
    value::{BytecodeLoc, Closure, FunctionName, Ref, Value},
    vm::{Exception, Vm, VmImage, VmLimits},
};
use log::info;

pub mod logging;
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
    max_chunks: usize,
    max_defs: usize,
    ast_capacity: usize,
    chunk_capacity: usize,
    stack_capacity: usize,
    call_stack_capacity: usize,
    ref_capacity: usize,
    fuel: usize,
    bitmap_stack_capacity: usize,
    transform_stack_capacity: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_chunks: 2,
            max_defs: 256,
            ast_capacity: 1024,
            chunk_capacity: 65536,
            stack_capacity: 1024,
            call_stack_capacity: 256,
            ref_capacity: 2048,
            fuel: 65536,
            bitmap_stack_capacity: 4,
            transform_stack_capacity: 16,
        }
    }
}

#[derive(Debug, Clone)]
struct Instance {
    limits: Limits,

    system: System,
    system_image: SystemImage,
    defs: Defs,
    defs_image: DefsImage,
    vm: Vm,
    vm_image: VmImage,
    exception: Option<Exception>,
}

#[no_mangle]
unsafe extern "C" fn haku_instance_new() -> *mut Instance {
    // TODO: This should be a parameter.
    let limits = Limits::default();
    let system = System::new(limits.max_chunks);

    let defs = Defs::new(limits.max_defs);
    let vm = Vm::new(
        &defs,
        &VmLimits {
            stack_capacity: limits.stack_capacity,
            call_stack_capacity: limits.call_stack_capacity,
            ref_capacity: limits.ref_capacity,
            fuel: limits.fuel,
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
        exception: None,
    });
    Box::leak(instance)
}

#[no_mangle]
unsafe extern "C" fn haku_instance_destroy(instance: *mut Instance) {
    drop(Box::from_raw(instance));
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
extern "C" fn haku_status_string(code: StatusCode) -> *const i8 {
    match code {
        StatusCode::Ok => c"ok",
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
    Ready(ChunkId),
}

#[derive(Debug, Default)]
struct Brush {
    diagnostics: Vec<Diagnostic>,
    state: BrushState,
}

#[no_mangle]
extern "C" fn haku_brush_new() -> *mut Brush {
    Box::leak(Box::new(Brush::default()))
}

#[no_mangle]
unsafe extern "C" fn haku_brush_destroy(brush: *mut Brush) {
    drop(Box::from_raw(brush))
}

#[no_mangle]
unsafe extern "C" fn haku_num_diagnostics(brush: *const Brush) -> u32 {
    (*brush).diagnostics.len() as u32
}

#[no_mangle]
unsafe extern "C" fn haku_diagnostic_start(brush: *const Brush, index: u32) -> u32 {
    (*brush).diagnostics[index as usize].span.start as u32
}

#[no_mangle]
unsafe extern "C" fn haku_diagnostic_end(brush: *const Brush, index: u32) -> u32 {
    (*brush).diagnostics[index as usize].span.end as u32
}

#[no_mangle]
unsafe extern "C" fn haku_diagnostic_message(brush: *const Brush, index: u32) -> *const u8 {
    (*brush).diagnostics[index as usize].message.as_ptr()
}

#[no_mangle]
unsafe extern "C" fn haku_diagnostic_message_len(brush: *const Brush, index: u32) -> u32 {
    (*brush).diagnostics[index as usize].message.len() as u32
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

    let ast = Ast::new(instance.limits.ast_capacity);
    let mut parser = Parser::new(ast, code);
    let root = parse_toplevel(&mut parser);
    let ast = parser.ast;

    let src = Source {
        code,
        ast: &ast,
        system: &instance.system,
    };

    let mut chunk = Chunk::new(instance.limits.chunk_capacity).unwrap();
    let mut compiler = Compiler::new(&mut instance.defs, &mut chunk);
    if let Err(error) = compile_expr(&mut compiler, &src, root) {
        match error {
            CompileError::Emit => return StatusCode::ChunkTooBig,
        }
    }

    if !compiler.diagnostics.is_empty() {
        brush.diagnostics = compiler.diagnostics;
        return StatusCode::DiagnosticsEmitted;
    }

    let chunk_id = match instance.system.add_chunk(chunk) {
        Ok(chunk_id) => chunk_id,
        Err(_) => return StatusCode::TooManyChunks,
    };
    brush.state = BrushState::Ready(chunk_id);

    info!("brush compiled into {chunk_id:?}");

    StatusCode::Ok
}

struct BitmapLock {
    bitmap: Option<Bitmap>,
}

#[no_mangle]
extern "C" fn haku_bitmap_new(width: u32, height: u32) -> *mut BitmapLock {
    Box::leak(Box::new(BitmapLock {
        bitmap: Some(Bitmap::new(width, height)),
    }))
}

#[no_mangle]
unsafe extern "C" fn haku_bitmap_destroy(bitmap: *mut BitmapLock) {
    drop(Box::from_raw(bitmap))
}

#[no_mangle]
unsafe extern "C" fn haku_bitmap_data(bitmap: *mut BitmapLock) -> *mut u8 {
    let bitmap = (*bitmap)
        .bitmap
        .as_mut()
        .expect("bitmap is already being rendered to");

    bitmap.pixels[..].as_mut_ptr() as *mut u8
}

#[no_mangle]
unsafe extern "C" fn haku_render_brush(
    instance: *mut Instance,
    brush: *const Brush,
    bitmap: *mut BitmapLock,
) -> StatusCode {
    let instance = &mut *instance;
    let brush = &*brush;

    let BrushState::Ready(chunk_id) = brush.state else {
        panic!("brush is not compiled and ready to be used");
    };

    let Ok(closure_id) = instance.vm.create_ref(Ref::Closure(Closure {
        start: BytecodeLoc {
            chunk_id,
            offset: 0,
        },
        name: FunctionName::Anonymous,
        param_count: 0,
        captures: Vec::new(),
    })) else {
        return StatusCode::OutOfRefSlots;
    };

    let scribble = match instance.vm.run(&instance.system, closure_id) {
        Ok(value) => value,
        Err(exn) => {
            instance.exception = Some(exn);
            return StatusCode::EvalException;
        }
    };

    let bitmap_locked = (*bitmap)
        .bitmap
        .take()
        .expect("bitmap is already being rendered to");

    let mut renderer = Renderer::new(
        bitmap_locked,
        &RendererLimits {
            bitmap_stack_capacity: instance.limits.bitmap_stack_capacity,
            transform_stack_capacity: instance.limits.transform_stack_capacity,
        },
    );
    match renderer.render(&instance.vm, scribble) {
        Ok(()) => (),
        Err(exn) => {
            instance.exception = Some(exn);
            return StatusCode::RenderException;
        }
    }

    let bitmap_locked = renderer.finish();

    (*bitmap).bitmap = Some(bitmap_locked);
    instance.vm.restore_image(&instance.vm_image);

    StatusCode::Ok
}
