use std::path::PathBuf;

use compiler_test::COMPREHENSIVE_SRC;
use frontend::{
    ParseAst,
    err::DisplayError,
    formatter::Formatter,
    lexer::{CloesceLexer, LexResult, LexTarget},
    parser::CloesceParser,
};

fn lex_parse(src: &str) -> (ParseAst<'_>, LexResult<'_>) {
    let source = LexTarget {
        src,
        path: PathBuf::from("<test>"),
    };

    let lexed = CloesceLexer::lex(vec![source]);
    if lexed.has_errors() {
        lexed.display_error(&lexed.file_table);
        panic!("lexing should succeed");
    }

    let parse_ast = CloesceParser::parse(&lexed.results, &lexed.file_table);
    if parse_ast.has_errors() {
        parse_ast.display_error(&lexed.file_table);
        panic!("parse should succeed");
    }

    (parse_ast.ast, lexed)
}

#[test]
fn format_non_lossy() {
    let (parse_ast, lex_result) = lex_parse(COMPREHENSIVE_SRC);
    let comment_map = &lex_result.results[0].comment_map;

    let formatted = Formatter::format(&parse_ast, comment_map, COMPREHENSIVE_SRC);
    let (reparse_ast, _) = lex_parse(&formatted);

    assert_eq!(
        parse_ast.blocks.len(),
        reparse_ast.blocks.len(),
        "block count mismatch"
    );
}

#[test]
fn format_idempotent() {
    let (parse_ast, lex_result) = lex_parse(COMPREHENSIVE_SRC);
    let comment_map = &lex_result.results[0].comment_map;

    let formatted = Formatter::format(&parse_ast, comment_map, COMPREHENSIVE_SRC);
    let (reparse_ast, relex_result) = lex_parse(&formatted);
    let reformatted = Formatter::format(
        &reparse_ast,
        &relex_result.results[0].comment_map,
        &formatted,
    );

    assert_eq!(
        formatted, reformatted,
        "formatting should be consistent on already formatted code"
    );
}

#[test]
fn format_leading_trailing_comments() {
    let src = r#"
// Leading comment for A
model A {
    // Leading comment for field1
    field1: string // Trailing comment for field1
    // Leading comment for field2
    field2: int // Trailing comment for field2
}
    "#;

    let (parse_ast, lex_result) = lex_parse(src);
    let comment_map = &lex_result.results[0].comment_map;

    let formatted = Formatter::format(&parse_ast, comment_map, src);
    let (reparse_ast, relex_result) = lex_parse(&formatted);
    let reformatted = Formatter::format(
        &reparse_ast,
        &relex_result.results[0].comment_map,
        &formatted,
    );

    assert_eq!(
        formatted, reformatted,
        "formatting should preserve leading and trailing comments"
    );
}

#[test]
fn format_comments_in_primary_block() {
    let src = r#"
model User {
    primary {
        // Leading comment for id
        id: int // Trailing comment for id
        // Leading comment for companyId
        companyId: int
    }
}
    "#;

    let (parse_ast, lex_result) = lex_parse(src);
    let comment_map = &lex_result.results[0].comment_map;

    let formatted = Formatter::format(&parse_ast, comment_map, src);
    let (reparse_ast, relex_result) = lex_parse(&formatted);
    let reformatted = Formatter::format(
        &reparse_ast,
        &relex_result.results[0].comment_map,
        &formatted,
    );

    assert_eq!(
        formatted, reformatted,
        "comments inside primary block should be preserved idempotently"
    );
}

#[test]
fn format_comments_in_paginated_block() {
    let src = r#"
[use myKv]
model Feed {
    paginated {
        // KV entry for feed
        kv (myKv, "feed/{id}") {
            item: string
        }
    }
}
    "#;

    let (parse_ast, lex_result) = lex_parse(src);
    let comment_map = &lex_result.results[0].comment_map;

    let formatted = Formatter::format(&parse_ast, comment_map, src);
    let (reparse_ast, relex_result) = lex_parse(&formatted);
    let reformatted = Formatter::format(
        &reparse_ast,
        &relex_result.results[0].comment_map,
        &formatted,
    );

    assert_eq!(
        formatted, reformatted,
        "comments inside paginated block should be preserved idempotently"
    );
}

#[test]
fn format_multiple_leading_comments() {
    let src = r#"
// First leading comment
// Second leading comment
model A {
    x: int
}
    "#;

    let (parse_ast, lex_result) = lex_parse(src);
    let comment_map = &lex_result.results[0].comment_map;

    let formatted = Formatter::format(&parse_ast, comment_map, src);
    let (reparse_ast, relex_result) = lex_parse(&formatted);
    let reformatted = Formatter::format(
        &reparse_ast,
        &relex_result.results[0].comment_map,
        &formatted,
    );

    assert_eq!(
        formatted, reformatted,
        "multiple consecutive leading comments should be preserved idempotently"
    );
}

#[test]
fn format_comments_in_env_block() {
    let src = r#"
// env describes wrangler bindings
env {
    // d1 databases
    d1 {
        db
        // db2 is commented out
    }

    // r2 buckets
    r2 {
        bucket
    }
}
    "#;

    let (parse_ast, lex_result) = lex_parse(src);
    let comment_map = &lex_result.results[0].comment_map;

    let formatted = Formatter::format(&parse_ast, comment_map, src);
    let (reparse_ast, relex_result) = lex_parse(&formatted);
    let reformatted = Formatter::format(
        &reparse_ast,
        &relex_result.results[0].comment_map,
        &formatted,
    );

    assert_eq!(
        formatted, reformatted,
        "comments inside env block should be preserved idempotently"
    );
}
