//! High-level wrapper for Haku.

// TODO: This should be used as the basis for haku-wasm as well as haku tests in the future to
// avoid duplicating code.

use eyre::{bail, Context, OptionExt};
use haku::{
    ast::Ast,
    bytecode::{Chunk, Defs, DefsImage},
    compiler::{ClosureSpec, Compiler, Source},
    lexer::{lex, Lexer},
    parser::{self, Parser, ParserLimits},
    render::{tiny_skia::Pixmap, Renderer, RendererLimits},
    source::SourceCode,
    system::{ChunkId, System, SystemImage},
    token::Lexis,
    value::{Closure, Ref, Value},
    vm::{Vm, VmImage, VmLimits},
};
use serde::{Deserialize, Serialize};
use tracing::{info, instrument, Level};

use crate::schema::Vec2;

#[derive(Debug, Clone, Deserialize, Serialize)]
// NOTE: For serialization, this struct does _not_ have serde(rename_all = "camelCase") on it,
// because we do some dynamic typing magic over on the JavaScript side to automatically call all
// the appropriate functions for setting these limits on the client side.
pub struct Limits {
    pub max_source_code_len: u32,
    pub max_chunks: usize,
    pub max_defs: usize,
    pub max_tokens: usize,
    pub max_parser_events: usize,
    pub ast_capacity: usize,
    pub chunk_capacity: usize,
    pub stack_capacity: usize,
    pub call_stack_capacity: usize,
    pub ref_capacity: usize,
    pub fuel: usize,
    pub memory: usize,
    pub pixmap_stack_capacity: usize,
    pub transform_stack_capacity: usize,
}

pub struct Haku {
    limits: Limits,

    system: System,
    system_image: SystemImage,
    defs: Defs,
    defs_image: DefsImage,
    vm: Vm,
    vm_image: VmImage,

    brush: Option<(ChunkId, ClosureSpec)>,
}

impl Haku {
    pub fn new(limits: Limits) -> Self {
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

        Self {
            limits,
            system,
            system_image,
            defs,
            defs_image,
            vm,
            vm_image,
            brush: None,
        }
    }

    fn reset(&mut self) {
        self.system.restore_image(&self.system_image);
        self.defs.restore_image(&self.defs_image);
    }

    #[instrument(skip(self, code), err)]
    pub fn set_brush(&mut self, code: &str) -> eyre::Result<()> {
        info!(?code);

        self.reset();

        let code = SourceCode::limited_len(code, self.limits.max_source_code_len)
            .ok_or_eyre("source code is too long")?;

        let mut lexer = Lexer::new(Lexis::new(self.limits.max_tokens), code);
        lex(&mut lexer)?;

        let mut parser = Parser::new(
            &lexer.lexis,
            &ParserLimits {
                max_events: self.limits.max_parser_events,
            },
        );
        parser::toplevel(&mut parser);
        let mut ast = Ast::new(self.limits.ast_capacity);
        let (root, parser_diagnostics) = parser.into_ast(&mut ast)?;

        let src = Source {
            code,
            ast: &ast,
            system: &self.system,
        };

        let mut chunk = Chunk::new(self.limits.chunk_capacity)
            .expect("chunk capacity must be representable as a 16-bit number");
        let mut compiler = Compiler::new(&mut self.defs, &mut chunk);
        haku::compiler::compile_expr(&mut compiler, &src, root)
            .context("failed to compile the chunk")?;
        let closure_spec = compiler.closure_spec();

        if !lexer.diagnostics.is_empty()
            || !parser_diagnostics.is_empty()
            || !compiler.diagnostics.is_empty()
        {
            info!(?lexer.diagnostics, ?parser_diagnostics, ?compiler.diagnostics, "diagnostics were emitted");
            bail!("diagnostics were emitted");
        }

        let chunk_id = self.system.add_chunk(chunk).context("too many chunks")?;
        self.brush = Some((chunk_id, closure_spec));

        info!("brush set successfully");

        Ok(())
    }

    #[instrument(skip(self), err(level = Level::INFO))]
    pub fn eval_brush(&mut self) -> eyre::Result<Value> {
        let (chunk_id, closure_spec) = self
            .brush
            .ok_or_eyre("brush is not compiled and ready to be used")?;

        self.vm.apply_defs(&self.defs);

        let closure_id = self
            .vm
            .create_ref(Ref::Closure(Closure::chunk(chunk_id, closure_spec)))
            .context("not enough ref slots to create initial closure")?;

        let scribble = self
            .vm
            .run(&self.system, closure_id)
            .context("an exception occurred while evaluating the scribble")?;

        Ok(scribble)
    }

    #[instrument(skip(self, pixmap, value, translation), err(level = Level::INFO))]
    pub fn render_value(
        &self,
        pixmap: &mut Pixmap,
        value: Value,
        translation: Vec2,
    ) -> eyre::Result<()> {
        let mut renderer = Renderer::new(
            pixmap,
            &RendererLimits {
                pixmap_stack_capacity: self.limits.pixmap_stack_capacity,
                transform_stack_capacity: self.limits.transform_stack_capacity,
            },
        );
        renderer.translate(translation.x, translation.y);
        let result = renderer.render(&self.vm, value);

        result.context("an exception occurred while rendering the scribble")
    }

    pub fn reset_vm(&mut self) {
        self.vm.restore_image(&self.vm_image);
    }
}
