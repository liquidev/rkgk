// NOTE: This is a very bad CLI. I only use it for debugging haku with LLDB.
// Sorry that it doesn't actually do anything!

use std::{error::Error, fmt::Display, io::BufRead};

use haku::{
    ast::{dump::dump, Ast},
    lexer::{lex, Lexer},
    parser::{expr, Parser, ParserLimits},
    source::SourceCode,
    token::Lexis,
    value::Value,
};

fn eval(code: &str) -> Result<Value, Box<dyn Error>> {
    let code = SourceCode::unlimited_len(code);
    let mut lexer = Lexer::new(Lexis::new(1024), code);
    lex(&mut lexer).expect("too many tokens");

    let mut parser = Parser::new(&lexer.lexis, &ParserLimits { max_events: 1024 });
    expr(&mut parser);

    let mut ast = Ast::new(1024);
    let (root, _) = parser.into_ast(&mut ast).unwrap();

    eprintln!("{}", dump(&ast, root, Some(code)));

    Ok(Value::Nil)
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
