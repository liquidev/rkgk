// NOTE: This is a very bad CLI.
// Sorry!

use std::{error::Error, fmt::Display, io::BufRead};

use haku::{
    bytecode::{Chunk, Defs},
    compiler::{compile_expr, Compiler, Source},
    sexp::{parse_toplevel, Ast, Parser},
    system::System,
    value::{BytecodeLoc, Closure, FunctionName, Ref, Value},
    vm::{Vm, VmLimits},
};

fn eval(code: &str) -> Result<Value, Box<dyn Error>> {
    let mut system = System::new(1);

    let ast = Ast::new(1024);
    let mut parser = Parser::new(ast, code);
    let root = parse_toplevel(&mut parser);
    let ast = parser.ast;
    let src = Source {
        code,
        ast: &ast,
        system: &system,
    };

    let mut defs = Defs::new(256);
    let mut chunk = Chunk::new(65536).unwrap();
    let mut compiler = Compiler::new(&mut defs, &mut chunk);
    compile_expr(&mut compiler, &src, root)?;
    let diagnostics = compiler.diagnostics;
    let defs = compiler.defs;
    println!("{chunk:?}");

    for diagnostic in &diagnostics {
        eprintln!(
            "{}..{}: {}",
            diagnostic.span.start, diagnostic.span.end, diagnostic.message
        );
    }

    if !diagnostics.is_empty() {
        return Err(Box::new(DiagnosticsEmitted));
    }

    let mut vm = Vm::new(
        defs,
        &VmLimits {
            stack_capacity: 256,
            call_stack_capacity: 256,
            ref_capacity: 256,
            fuel: 32768,
            memory: 1024,
        },
    );
    let chunk_id = system.add_chunk(chunk)?;
    let closure = vm.create_ref(Ref::Closure(Closure {
        start: BytecodeLoc {
            chunk_id,
            offset: 0,
        },
        name: FunctionName::Anonymous,
        param_count: 0,
        captures: Vec::new(),
    }))?;
    Ok(vm.run(&system, closure)?)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct DiagnosticsEmitted;

impl Display for DiagnosticsEmitted {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("diagnostics were emitted")
    }
}

impl Error for DiagnosticsEmitted {}

fn main() -> Result<(), Box<dyn Error>> {
    let stdin = std::io::stdin();
    for line in stdin.lock().lines() {
        let line = line?;
        match eval(&line) {
            Ok(value) => println!("{value:?}"),
            Err(error) => eprintln!("error: {error}"),
        }
    }

    Ok(())
}
