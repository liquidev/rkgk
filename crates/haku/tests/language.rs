use std::error::Error;

use haku::{
    ast::{dump::dump, Ast},
    bytecode::{Chunk, Defs},
    compiler::{compile_expr, Compiler, Source},
    lexer::{lex, Lexer},
    parser::{self, Parser, ParserLimits},
    source::SourceCode,
    system::System,
    token::Lexis,
    value::{Closure, Ref, RefId, Value},
    vm::{Vm, VmLimits},
};

fn eval(code: &str) -> Result<Value, Box<dyn Error>> {
    let mut system = System::new(1);

    let code = SourceCode::unlimited_len(code);

    let mut lexer = Lexer::new(Lexis::new(1024), code);
    lex(&mut lexer)?;

    let mut ast = Ast::new(1024);
    let mut parser = Parser::new(&lexer.lexis, &ParserLimits { max_events: 1024 });
    parser::toplevel(&mut parser);
    let (root, mut parser_diagnostics) = parser.into_ast(&mut ast)?;
    println!("{}", dump(&ast, root, Some(code)));
    let src = Source {
        code,
        ast: &ast,
        system: &system,
    };

    let mut defs = Defs::new(256);
    let mut chunk = Chunk::new(65536).unwrap();
    let mut compiler = Compiler::new(&mut defs, &mut chunk);
    compile_expr(&mut compiler, &src, root)?;
    let closure_spec = compiler.closure_spec();
    let defs = compiler.defs;

    let mut diagnostics = lexer.diagnostics;
    diagnostics.append(&mut parser_diagnostics);
    diagnostics.append(&mut compiler.diagnostics);

    for diagnostic in &diagnostics {
        println!(
            "{}..{} {:?}: {}",
            diagnostic.span().start,
            diagnostic.span().end,
            diagnostic.span().slice(code),
            diagnostic.message()
        );
    }

    if !diagnostics.is_empty() {
        panic!("diagnostics were emitted")
    }

    let limits = VmLimits {
        stack_capacity: 1024,
        call_stack_capacity: 256,
        ref_capacity: 256,
        fuel: 32768,
        memory: 1024,
    };
    let mut vm = Vm::new(defs, &limits);
    let chunk_id = system.add_chunk(chunk)?;
    println!("bytecode: {:?}", system.chunk(chunk_id));
    println!("closure spec: {closure_spec:?}");

    let closure = vm.create_ref(Ref::Closure(Closure::chunk(chunk_id, closure_spec)))?;
    let result = vm.run(&system, closure)?;

    println!("used fuel: {}", limits.fuel - vm.remaining_fuel());

    Ok(result)
}

#[track_caller]
fn expect_number(code: &str, number: f32, epsilon: f32) {
    match eval(code) {
        Ok(Value::Number(n)) => assert!((n - number).abs() < epsilon, "expected {number}, got {n}"),
        other => panic!("expected ok/numeric result, got {other:?}"),
    }
}

#[test]
fn literal_nil() {
    assert_eq!(eval("()").unwrap(), Value::Nil);
}

#[test]
fn literal_number() {
    expect_number("123", 123.0, 0.0001);
}

#[test]
fn literal_bool() {
    assert_eq!(eval("False").unwrap(), Value::False);
    assert_eq!(eval("True").unwrap(), Value::True);
}

#[test]
fn function_nil() {
    assert_eq!(
        eval(r#" \_ -> () "#).unwrap(),
        Value::Ref(RefId::from_u32(1))
    );
}

#[test]
fn function_nil_call() {
    assert_eq!(eval(r#"(\_ -> ()) ()"#).unwrap(), Value::Nil);
}

#[test]
fn function_arithmetic() {
    expect_number(r#"(\x -> x + 2) 2"#, 4.0, 0.0001);
}

#[test]
fn function_let() {
    expect_number(r#"(\addTwo -> addTwo 2) \x -> x + 2"#, 4.0, 0.0001);
}

#[test]
fn function_closure() {
    expect_number(r#"((\x -> \y -> x + y) 2) 2"#, 4.0, 0.0001);
}

#[test]
fn if_literal() {
    expect_number("if (1) 1 else 2", 1.0, 0.0001);
    expect_number("if (()) 1 else 2", 2.0, 0.0001);
    expect_number("if (False) 1 else 2", 2.0, 0.0001);
    expect_number("if (True) 1 else 2", 1.0, 0.0001);
}

#[test]
fn def_simple() {
    let code = r#"
        x = 1
        y = 2
        x + y
    "#;
    expect_number(code, 3.0, 0.0001);
}

#[test]
fn def_fib_recursive() {
    let code = r#"
        fib = \n ->
            if (n < 2)
                n
            else
                fib (n - 1) + fib (n - 2)
    
        fib 10
    "#;
    expect_number(code, 55.0, 0.0001);
}

#[test]
fn def_mutually_recursive() {
    let code = r#"
        f = \x ->
            if (x < 10)
                g (x + 1)
            else
                x

        g = \x ->
            if (x < 10)
                f (x * 2)
            else
                x

        f 0
    "#;
    expect_number(code, 14.0, 0.0001);
}

#[test]
fn def_botsbuildbots() {
    let code = r#"
        botsbuildbots = \_ -> botsbuildbots ()
        botsbuildbots ()
    "#;
    if let Err(error) = eval(code) {
        assert_eq!(
            error.to_string(),
            "Exception {\n    message: \"too much recursion\",\n}"
        );
    } else {
        panic!("error expected");
    }
}

#[test]
fn let_single() {
    let code = r#"
        let x = 1
        x + 1
    "#;
    expect_number(code, 2.0, 0.0001);
}

#[test]
fn let_many() {
    let code = r#"
        let x = 1
        let y = 2
        x + y
    "#;
    expect_number(code, 3.0, 0.0001);
}

#[test]
fn let_sequence() {
    let code = r#"
        let x = 1
        let y = x + 1
        x + y
    "#;
    expect_number(code, 3.0, 0.0001);
}

#[test]
fn let_subexpr() {
    let code = r#"
        (let x = 1
         let y = 2
         x * y) + 2
    "#;
    expect_number(code, 4.0, 0.0001);
}

#[test]
fn let_subexpr_two() {
    let code = r#"
        (let x = 1
         2) +
        (let x = 1
         x)
    "#;
    expect_number(code, 3.0, 0.0001);
}

#[test]
fn let_subexpr_many() {
    let code = r#"
        (let x = 1
         let y = 2
         x * y) +
        (let x = 1
         2) +
        (let x = 1
         x)
    "#;
    expect_number(code, 5.0, 0.0001);
}

#[test]
fn system_arithmetic() {
    expect_number("1 + 2 + 3 + 4", 10.0, 0.0001);
    expect_number("(2 * 1) + 1 + (6 / 2) + (10 - 3)", 13.0, 0.0001);
}

#[test]
fn issue_78() {
    let code = r#"
        f = \_ ->
            let x = 1
            x + x

        [
            f ()
            f ()
        ]
    "#;
    assert_eq!(eval(code).unwrap(), Value::Ref(RefId::from_u32(2)))
}
