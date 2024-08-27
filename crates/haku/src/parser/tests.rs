use alloc::{format, string::String};

use crate::{
    ast::{dump::dump, Ast, NodeId},
    lexer::{lex, Lexer},
    parser::expr,
    source::SourceCode,
    token::Lexis,
};

use super::{toplevel, Parser, ParserLimits};

fn parse(s: &str, f: fn(&mut Parser)) -> (Ast, NodeId) {
    let mut lexer = Lexer::new(Lexis::new(1024), SourceCode::unlimited_len(s));
    lex(&mut lexer).expect("too many tokens");

    let mut parser = Parser::new(&lexer.lexis, &ParserLimits { max_events: 1024 });
    f(&mut parser);

    if !parser.diagnostics.is_empty() {
        panic!("parser emitted diagnostics: {:#?}", parser.diagnostics);
    }

    let mut ast = Ast::new(1024);
    let (root, _) = parser.into_ast(&mut ast).unwrap();
    (ast, root)
}

fn ast(s: &str, f: fn(&mut Parser)) -> String {
    let (ast, root) = parse(s, f);
    // The extra newline is mostly so that it's easier to make the string literals look nice.
    format!("\n{}", dump(&ast, root, None))
}

#[track_caller]
fn assert_ast_eq(s: &str, f: fn(&mut Parser), ast_s: &str) {
    let got = ast(s, f);
    if ast_s != got {
        panic!("AST mismatch. expected:\n{ast_s}\n\ngot:\n{got}\n");
    }
}

#[test]
fn one_literals() {
    assert_ast_eq(
        "1",
        expr,
        "
Number @ 0..1
    Token @ 0..1",
    );

    assert_ast_eq(
        "ExampleTag123",
        expr,
        "
Tag @ 0..13
    Token @ 0..13",
    );

    assert_ast_eq(
        "example_ident123",
        expr,
        "
Ident @ 0..16
    Token @ 0..16",
    );

    assert_ast_eq(
        "#000",
        expr,
        "
Color @ 0..4
    Token @ 0..4",
    );

    assert_ast_eq(
        "#000F",
        expr,
        "
Color @ 0..5
    Token @ 0..5",
    );

    assert_ast_eq(
        "#058EF0",
        expr,
        "
Color @ 0..7
    Token @ 0..7",
    );

    assert_ast_eq(
        "#058EF0FF",
        expr,
        "
Color @ 0..9
    Token @ 0..9",
    );
}

#[test]
fn list() {
    assert_ast_eq(
        "[]",
        expr,
        "
List @ 0..2
    Token @ 0..1
    Token @ 1..2",
    );

    assert_ast_eq(
        "[1]",
        expr,
        "
List @ 0..3
    Token @ 0..1
    Number @ 1..2
        Token @ 1..2
    Token @ 2..3",
    );

    assert_ast_eq(
        "[1, 2]",
        expr,
        "
List @ 0..6
    Token @ 0..1
    Number @ 1..2
        Token @ 1..2
    Token @ 2..3
    Number @ 4..5
        Token @ 4..5
    Token @ 5..6",
    );

    assert_ast_eq(
        "[
             1
             2
         ]",
        expr,
        "
List @ 0..42
    Token @ 0..1
    Token @ 1..2
    Number @ 15..16
        Token @ 15..16
    Token @ 16..17
    Number @ 30..31
        Token @ 30..31
    Token @ 31..32
    Token @ 41..42",
    );
}

#[test]
fn unary() {
    assert_ast_eq(
        "-1",
        expr,
        "
Unary @ 0..2
    Op @ 0..1
        Token @ 0..1
    Number @ 1..2
        Token @ 1..2",
    );

    assert_ast_eq(
        "!1",
        expr,
        "
Unary @ 0..2
    Op @ 0..1
        Token @ 0..1
    Number @ 1..2
        Token @ 1..2",
    );
}

#[test]
fn binary_single() {
    assert_ast_eq(
        "1 + 1",
        expr,
        "
Binary @ 0..5
    Number @ 0..1
        Token @ 0..1
    Op @ 2..3
        Token @ 2..3
    Number @ 4..5
        Token @ 4..5",
    );

    assert_ast_eq(
        "1 - 1",
        expr,
        "
Binary @ 0..5
    Number @ 0..1
        Token @ 0..1
    Op @ 2..3
        Token @ 2..3
    Number @ 4..5
        Token @ 4..5",
    );

    assert_ast_eq(
        "1 * 1",
        expr,
        "
Binary @ 0..5
    Number @ 0..1
        Token @ 0..1
    Op @ 2..3
        Token @ 2..3
    Number @ 4..5
        Token @ 4..5",
    );

    assert_ast_eq(
        "1 / 1",
        expr,
        "
Binary @ 0..5
    Number @ 0..1
        Token @ 0..1
    Op @ 2..3
        Token @ 2..3
    Number @ 4..5
        Token @ 4..5",
    );

    assert_ast_eq(
        "1 < 1",
        expr,
        "
Binary @ 0..5
    Number @ 0..1
        Token @ 0..1
    Op @ 2..3
        Token @ 2..3
    Number @ 4..5
        Token @ 4..5",
    );

    assert_ast_eq(
        "1 > 1",
        expr,
        "
Binary @ 0..5
    Number @ 0..1
        Token @ 0..1
    Op @ 2..3
        Token @ 2..3
    Number @ 4..5
        Token @ 4..5",
    );

    assert_ast_eq(
        "1 == 1",
        expr,
        "
Binary @ 0..6
    Number @ 0..1
        Token @ 0..1
    Op @ 2..4
        Token @ 2..4
    Number @ 5..6
        Token @ 5..6",
    );

    assert_ast_eq(
        "1 != 1",
        expr,
        "
Binary @ 0..6
    Number @ 0..1
        Token @ 0..1
    Op @ 2..4
        Token @ 2..4
    Number @ 5..6
        Token @ 5..6",
    );

    assert_ast_eq(
        "1 <= 1",
        expr,
        "
Binary @ 0..6
    Number @ 0..1
        Token @ 0..1
    Op @ 2..4
        Token @ 2..4
    Number @ 5..6
        Token @ 5..6",
    );

    assert_ast_eq(
        "1 >= 1",
        expr,
        "
Binary @ 0..6
    Number @ 0..1
        Token @ 0..1
    Op @ 2..4
        Token @ 2..4
    Number @ 5..6
        Token @ 5..6",
    );

    assert_ast_eq(
        "1 = 1",
        expr,
        "
Binary @ 0..5
    Number @ 0..1
        Token @ 0..1
    Op @ 2..3
        Token @ 2..3
    Number @ 4..5
        Token @ 4..5",
    );
}

#[test]
fn binary_precedence() {
    assert_ast_eq(
        "1 + 1 + 1",
        expr,
        "
Binary @ 0..9
    Binary @ 0..5
        Number @ 0..1
            Token @ 0..1
        Op @ 2..3
            Token @ 2..3
        Number @ 4..5
            Token @ 4..5
    Op @ 6..7
        Token @ 6..7
    Number @ 8..9
        Token @ 8..9",
    );

    assert_ast_eq(
        "1 * 1 + 1",
        expr,
        "
Binary @ 0..9
    Binary @ 0..5
        Number @ 0..1
            Token @ 0..1
        Op @ 2..3
            Token @ 2..3
        Number @ 4..5
            Token @ 4..5
    Op @ 6..7
        Token @ 6..7
    Number @ 8..9
        Token @ 8..9",
    );

    assert_ast_eq(
        "1 + 1 * 1",
        expr,
        "
Binary @ 0..9
    Number @ 0..1
        Token @ 0..1
    Op @ 2..3
        Token @ 2..3
    Binary @ 4..9
        Number @ 4..5
            Token @ 4..5
        Op @ 6..7
            Token @ 6..7
        Number @ 8..9
            Token @ 8..9",
    );

    assert_ast_eq(
        "1 < 1 + 1",
        expr,
        "
Binary @ 0..9
    Number @ 0..1
        Token @ 0..1
    Op @ 2..3
        Token @ 2..3
    Binary @ 4..9
        Number @ 4..5
            Token @ 4..5
        Op @ 6..7
            Token @ 6..7
        Number @ 8..9
            Token @ 8..9",
    );

    assert_ast_eq(
        "1 + 1 < 1",
        expr,
        "
Binary @ 0..9
    Binary @ 0..5
        Number @ 0..1
            Token @ 0..1
        Op @ 2..3
            Token @ 2..3
        Number @ 4..5
            Token @ 4..5
    Op @ 6..7
        Token @ 6..7
    Number @ 8..9
        Token @ 8..9",
    );

    assert_ast_eq(
        "1 + 1 * 1 < 1",
        expr,
        "
Binary @ 0..13
    Binary @ 0..9
        Number @ 0..1
            Token @ 0..1
        Op @ 2..3
            Token @ 2..3
        Binary @ 4..9
            Number @ 4..5
                Token @ 4..5
            Op @ 6..7
                Token @ 6..7
            Number @ 8..9
                Token @ 8..9
    Op @ 10..11
        Token @ 10..11
    Number @ 12..13
        Token @ 12..13",
    );

    assert_ast_eq(
        "1 * 1 + 1 < 1",
        expr,
        "
Binary @ 0..13
    Binary @ 0..9
        Binary @ 0..5
            Number @ 0..1
                Token @ 0..1
            Op @ 2..3
                Token @ 2..3
            Number @ 4..5
                Token @ 4..5
        Op @ 6..7
            Token @ 6..7
        Number @ 8..9
            Token @ 8..9
    Op @ 10..11
        Token @ 10..11
    Number @ 12..13
        Token @ 12..13",
    );
}

#[test]
fn binary_cont() {
    assert_ast_eq(
        "1 +
           1",
        expr,
        "
Binary @ 0..16
    Number @ 0..1
        Token @ 0..1
    Op @ 2..3
        Token @ 2..3
    Token @ 3..4
    Number @ 15..16
        Token @ 15..16",
    );

    assert_ast_eq(
        "1 +

           1",
        expr,
        "
Binary @ 0..17
    Number @ 0..1
        Token @ 0..1
    Op @ 2..3
        Token @ 2..3
    Token @ 3..4
    Number @ 16..17
        Token @ 16..17",
    );
}

#[test]
fn paren_empty() {
    assert_ast_eq(
        "()",
        expr,
        "
ParenEmpty @ 0..2
    Token @ 0..1
    Token @ 1..2",
    );
}

#[test]
fn paren() {
    assert_ast_eq(
        "(1)",
        expr,
        "
Paren @ 0..3
    Token @ 0..1
    Number @ 1..2
        Token @ 1..2
    Token @ 2..3",
    );

    assert_ast_eq(
        "(1 + 1) * 1",
        expr,
        "
Binary @ 0..11
    Paren @ 0..7
        Token @ 0..1
        Binary @ 1..6
            Number @ 1..2
                Token @ 1..2
            Op @ 3..4
                Token @ 3..4
            Number @ 5..6
                Token @ 5..6
        Token @ 6..7
    Op @ 8..9
        Token @ 8..9
    Number @ 10..11
        Token @ 10..11",
    );

    assert_ast_eq(
        "1 * (1 + 1)",
        expr,
        "
Binary @ 0..11
    Number @ 0..1
        Token @ 0..1
    Op @ 2..3
        Token @ 2..3
    Paren @ 4..11
        Token @ 4..5
        Binary @ 5..10
            Number @ 5..6
                Token @ 5..6
            Op @ 7..8
                Token @ 7..8
            Number @ 9..10
                Token @ 9..10
        Token @ 10..11",
    );

    assert_ast_eq(
        "(
             1 +
             1   
         )",
        expr,
        "
Paren @ 0..47
    Token @ 0..1
    Token @ 1..2
    Binary @ 15..33
        Number @ 15..16
            Token @ 15..16
        Op @ 17..18
            Token @ 17..18
        Token @ 18..19
        Number @ 32..33
            Token @ 32..33
    Token @ 36..37
    Token @ 46..47",
    );
}

#[test]
fn infix_call() {
    assert_ast_eq(
        "f x y",
        toplevel,
        "
Toplevel @ 0..5
    Call @ 0..5
        Ident @ 0..1
            Token @ 0..1
        Ident @ 2..3
            Token @ 2..3
        Ident @ 4..5
            Token @ 4..5",
    );

    assert_ast_eq(
        "sin 1 + cos 2",
        toplevel,
        "
Toplevel @ 0..13
    Binary @ 0..13
        Call @ 0..5
            Ident @ 0..3
                Token @ 0..3
            Number @ 4..5
                Token @ 4..5
        Op @ 6..7
            Token @ 6..7
        Call @ 8..13
            Ident @ 8..11
                Token @ 8..11
            Number @ 12..13
                Token @ 12..13",
    );
}

#[test]
fn infix_call_unary_arg() {
    assert_ast_eq(
        // NOTE: The whitespace here is misleading.
        // This is a binary `-`.
        "f -1",
        toplevel,
        "
Toplevel @ 0..4
    Binary @ 0..4
        Ident @ 0..1
            Token @ 0..1
        Op @ 2..3
            Token @ 2..3
        Number @ 3..4
            Token @ 3..4",
    );

    assert_ast_eq(
        "f (-1)",
        toplevel,
        "
Toplevel @ 0..6
    Call @ 0..6
        Ident @ 0..1
            Token @ 0..1
        Paren @ 2..6
            Token @ 2..3
            Unary @ 3..5
                Op @ 3..4
                    Token @ 3..4
                Number @ 4..5
                    Token @ 4..5
            Token @ 5..6",
    );
}

#[test]
fn lambda() {
    assert_ast_eq(
        r#" \_ -> () "#,
        toplevel,
        "
Toplevel @ 1..9
    Lambda @ 1..9
        Token @ 1..2
        Params @ 2..3
            Param @ 2..3
                Token @ 2..3
        Token @ 4..6
        ParenEmpty @ 7..9
            Token @ 7..8
            Token @ 8..9",
    );

    assert_ast_eq(
        r#" \x -> x "#,
        toplevel,
        "
Toplevel @ 1..8
    Lambda @ 1..8
        Token @ 1..2
        Params @ 2..3
            Param @ 2..3
                Token @ 2..3
        Token @ 4..6
        Ident @ 7..8
            Token @ 7..8",
    );

    assert_ast_eq(
        r#" \x, y -> x + y "#,
        toplevel,
        "
Toplevel @ 1..15
    Lambda @ 1..15
        Token @ 1..2
        Params @ 2..6
            Param @ 2..3
                Token @ 2..3
            Token @ 3..4
            Param @ 5..6
                Token @ 5..6
        Token @ 7..9
        Binary @ 10..15
            Ident @ 10..11
                Token @ 10..11
            Op @ 12..13
                Token @ 12..13
            Ident @ 14..15
                Token @ 14..15",
    );

    assert_ast_eq(
        r#" \x, y ->
              x + y "#,
        toplevel,
        "
Toplevel @ 1..29
    Lambda @ 1..29
        Token @ 1..2
        Params @ 2..6
            Param @ 2..3
                Token @ 2..3
            Token @ 3..4
            Param @ 5..6
                Token @ 5..6
        Token @ 7..9
        Token @ 9..10
        Binary @ 24..29
            Ident @ 24..25
                Token @ 24..25
            Op @ 26..27
                Token @ 26..27
            Ident @ 28..29
                Token @ 28..29",
    );

    assert_ast_eq(
        r#" f \x -> g \y -> x + y "#,
        toplevel,
        "
Toplevel @ 1..22
    Call @ 1..22
        Ident @ 1..2
            Token @ 1..2
        Lambda @ 3..22
            Token @ 3..4
            Params @ 4..5
                Param @ 4..5
                    Token @ 4..5
            Token @ 6..8
            Call @ 9..22
                Ident @ 9..10
                    Token @ 9..10
                Lambda @ 11..22
                    Token @ 11..12
                    Params @ 12..13
                        Param @ 12..13
                            Token @ 12..13
                    Token @ 14..16
                    Binary @ 17..22
                        Ident @ 17..18
                            Token @ 17..18
                        Op @ 19..20
                            Token @ 19..20
                        Ident @ 21..22
                            Token @ 21..22",
    );

    assert_ast_eq(
        r#" f \x ->
            g \y ->
              x + y "#,
        toplevel,
        "
Toplevel @ 1..48
    Call @ 1..48
        Ident @ 1..2
            Token @ 1..2
        Lambda @ 3..48
            Token @ 3..4
            Params @ 4..5
                Param @ 4..5
                    Token @ 4..5
            Token @ 6..8
            Token @ 8..9
            Call @ 21..48
                Ident @ 21..22
                    Token @ 21..22
                Lambda @ 23..48
                    Token @ 23..24
                    Params @ 24..25
                        Param @ 24..25
                            Token @ 24..25
                    Token @ 26..28
                    Token @ 28..29
                    Binary @ 43..48
                        Ident @ 43..44
                            Token @ 43..44
                        Op @ 45..46
                            Token @ 45..46
                        Ident @ 47..48
                            Token @ 47..48",
    );
}

#[test]
fn if_expr() {
    assert_ast_eq(
        r#" if (true) 1 else 2 "#,
        toplevel,
        "
Toplevel @ 1..19
    If @ 1..19
        Token @ 1..3
        Token @ 4..5
        Ident @ 5..9
            Token @ 5..9
        Token @ 9..10
        Number @ 11..12
            Token @ 11..12
        Token @ 13..17
        Number @ 18..19
            Token @ 18..19",
    );

    assert_ast_eq(
        r#" if (true)
                1
            else
                2 "#,
        toplevel,
        "
Toplevel @ 1..63
    If @ 1..63
        Token @ 1..3
        Token @ 4..5
        Ident @ 5..9
            Token @ 5..9
        Token @ 9..10
        Token @ 10..11
        Number @ 27..28
            Token @ 27..28
        Token @ 28..29
        Token @ 41..45
        Token @ 45..46
        Number @ 62..63
            Token @ 62..63",
    );
}

#[test]
fn let_expr() {
    assert_ast_eq(
        r#" let x = 1
            x "#,
        toplevel,
        "
Toplevel @ 1..24
    Let @ 1..24
        Token @ 1..4
        Ident @ 5..6
            Token @ 5..6
        Token @ 7..8
        Number @ 9..10
            Token @ 9..10
        Token @ 10..11
        Ident @ 23..24
            Token @ 23..24",
    );

    assert_ast_eq(
        r#" let x = 1
            let y = 2
            x + y "#,
        toplevel,
        "
Toplevel @ 1..50
    Let @ 1..50
        Token @ 1..4
        Ident @ 5..6
            Token @ 5..6
        Token @ 7..8
        Number @ 9..10
            Token @ 9..10
        Token @ 10..11
        Let @ 23..50
            Token @ 23..26
            Ident @ 27..28
                Token @ 27..28
            Token @ 29..30
            Number @ 31..32
                Token @ 31..32
            Token @ 32..33
            Binary @ 45..50
                Ident @ 45..46
                    Token @ 45..46
                Op @ 47..48
                    Token @ 47..48
                Ident @ 49..50
                    Token @ 49..50",
    )
}
