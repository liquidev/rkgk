use std::error::Error;

use haku::{
    bytecode::{Chunk, Defs},
    compiler::{compile_expr, Compiler, Source},
    sexp::{self, Ast, Parser},
    system::System,
    value::{BytecodeLoc, Closure, FunctionName, Ref, RefId, Value},
    vm::{Vm, VmLimits},
};

fn eval(code: &str) -> Result<Value, Box<dyn Error>> {
    let mut system = System::new(1);

    let ast = Ast::new(1024);
    let mut parser = Parser::new(ast, code);
    let root = sexp::parse_toplevel(&mut parser);
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
    let defs = compiler.defs;

    for diagnostic in &compiler.diagnostics {
        println!(
            "{}..{}: {}",
            diagnostic.span.start, diagnostic.span.end, diagnostic.message
        );
    }

    if !compiler.diagnostics.is_empty() {
        panic!("compiler diagnostics were emitted")
    }

    let limits = VmLimits {
        stack_capacity: 256,
        call_stack_capacity: 256,
        ref_capacity: 256,
        fuel: 32768,
        memory: 1024,
    };
    let mut vm = Vm::new(defs, &limits);
    let chunk_id = system.add_chunk(chunk)?;
    println!("bytecode: {:?}", system.chunk(chunk_id));

    let closure = vm.create_ref(Ref::Closure(Closure {
        start: BytecodeLoc {
            chunk_id,
            offset: 0,
        },
        name: FunctionName::Anonymous,
        param_count: 0,
        captures: Vec::new(),
    }))?;
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
    assert_eq!(eval("false").unwrap(), Value::False);
    assert_eq!(eval("true").unwrap(), Value::True);
}

#[test]
fn function_nil() {
    assert_eq!(eval("(fn () ())").unwrap(), Value::Ref(RefId::from_u32(1)));
}

#[test]
fn function_nil_call() {
    assert_eq!(eval("((fn () ()))").unwrap(), Value::Nil);
}

#[test]
fn function_arithmetic() {
    expect_number("((fn (x) (+ x 2)) 2)", 4.0, 0.0001);
}

#[test]
fn function_let() {
    expect_number("((fn (add-two) (add-two 2)) (fn (x) (+ x 2)))", 4.0, 0.0001);
}

#[test]
fn function_closure() {
    expect_number("(((fn (x) (fn (y) (+ x y))) 2) 2)", 4.0, 0.0001);
}

#[test]
fn if_literal() {
    expect_number("(if 1 1 2)", 1.0, 0.0001);
    expect_number("(if () 1 2)", 2.0, 0.0001);
    expect_number("(if false 1 2)", 2.0, 0.0001);
    expect_number("(if true 1 2)", 1.0, 0.0001);
}

#[test]
fn def_simple() {
    let code = r#"
        (def x 1)
        (def y 2)
        (+ x y)
    "#;
    expect_number(code, 3.0, 0.0001);
}

#[test]
fn def_fib_recursive() {
    let code = r#"
        (def fib
            (fn (n)
                (if (< n 2)
                    n
                    (+ (fib (- n 1)) (fib (- n 2))))))

        (fib 10)
    "#;
    expect_number(code, 55.0, 0.0001);
}

#[test]
fn def_mutually_recursive() {
    let code = r#"
        (def f
            (fn (x)
                (if (< x 10)
                    (g (+ x 1))
                    x)))

        (def g
            (fn (x)
                (if (< x 10)
                    (f (* x 2))
                    x)))

        (f 0)
    "#;
    expect_number(code, 14.0, 0.0001);
}

#[test]
fn def_botsbuildbots() {
    let result = eval("(def botsbuildbots (fn () (botsbuildbots))) (botsbuildbots)");
    if let Err(error) = result {
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
        (let ((x 1))
            (+ x 1))
    "#;
    expect_number(code, 2.0, 0.0001);
}

#[test]
fn let_many() {
    let code = r#"
        (let ((x 1)
              (y 2))
            (+ x y))
    "#;
    expect_number(code, 3.0, 0.0001);
}

#[test]
fn let_sequence() {
    let code = r#"
        (let ((x 1)
              (y (+ x 1)))
            (+ x y))
    "#;
    expect_number(code, 3.0, 0.0001);
}

#[test]
fn let_subexpr() {
    let code = r#"
        (+
            (let ((x 1)
                  (y 2))
                (* x y)))
    "#;
    expect_number(code, 2.0, 0.0001);
}

#[test]
fn let_empty() {
    let code = r#"
        (let () 1)
    "#;
    expect_number(code, 1.0, 0.0001);
}

#[test]
fn let_subexpr_empty() {
    let code = r#"
        (+ (let () 1) (let () 1))
    "#;
    expect_number(code, 2.0, 0.0001);
}

#[test]
fn let_subexpr_many() {
    let code = r#"
        (+
            (let ((x 1)
                  (y 2))
                (* x y))
            (let () 1)
            (let ((x 1)) x))
    "#;
    expect_number(code, 3.0, 0.0001);
}

#[test]
fn system_arithmetic() {
    expect_number("(+ 1 2 3 4)", 10.0, 0.0001);
    expect_number("(+ (* 2 1) 1 (/ 6 2) (- 10 3))", 13.0, 0.0001);
}

#[test]
fn practical_fib_recursive() {
    let code = r#"
        ((fn (fib)
            (fib fib 10))

         (fn (fib n)
             (if (< n 2)
                 n
                 (+ (fib fib (- n 1)) (fib fib (- n 2))))))
    "#;
    expect_number(code, 55.0, 0.0001);
}
