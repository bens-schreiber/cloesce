use std::path::PathBuf;

use compiler_test::COMPREHENSIVE_SRC;
use frontend::{
    Ast,
    err::DisplayError,
    formatter,
    lexer::{self, FileTable, LexTarget, LexedFile},
    parser,
};

fn lex_parse<'src>(src: &'src str) -> (Ast<'src>, Vec<LexedFile<'src>>, FileTable<'src>) {
    let source = LexTarget {
        src,
        path: PathBuf::from("<test>"),
    };

    let (lex_results, file_table) = lexer::lex(vec![source]).unwrap_or_else(|(errors, ft)| {
        errors.display_error(&ft);
        panic!("lexing should succeed");
    });

    let ast = parser::parse(&lex_results, &file_table).unwrap_or_else(|err| {
        err.display_error(&file_table);
        panic!("parse should succeed");
    });

    (ast, lex_results, file_table)
}

#[test]
fn format_non_lossy() {
    // Arrange
    let (parse_ast, lex_results, _) = lex_parse(COMPREHENSIVE_SRC);
    let comment_map = &lex_results[0].comment_map;

    // Act
    let formatted = formatter::format(&parse_ast, comment_map, COMPREHENSIVE_SRC);
    let (reparse_ast, _, _) = lex_parse(&formatted);

    // Assert
    assert_eq!(
        parse_ast.blocks.len(),
        reparse_ast.blocks.len(),
        "block count mismatch"
    );
}

#[test]
fn format_idempotent() {
    // Arrange
    let (parse_ast, lex_results, _) = lex_parse(COMPREHENSIVE_SRC);
    let comment_map = &lex_results[0].comment_map;

    // Act
    let formatted = formatter::format(&parse_ast, comment_map, COMPREHENSIVE_SRC);
    let (reparse_ast, relex_results, _) = lex_parse(&formatted);
    let reformatted = formatter::format(&reparse_ast, &relex_results[0].comment_map, &formatted);

    // Assert
    assert_eq!(
        formatted, reformatted,
        "formatting should be consistent on already formatted code"
    );
}

#[test]
fn comments_retained() {
    // Arrange
    let src = r#"
    // 0
    d1 {
        //1
        db
        //A
    } //3
    //B

    //4
    //6
    model BasicModel for db { //C
        //7
        primary { //D
            // gt above
            [gt 0] // gt side
            // gt below
            // lt above
            [lt 100] // lt side
            // lt below
            id: int //8
        } //9

        //14
        foreign OneToManyModel::id { //E
            fk_to_model //15
            //16
        } //19

        //F
        one OneToManyModel::id(fk_to_model) { //G
            oneToOneNav //18
        }
        //H
    } //20

    //21
    [internal] //22
    // 23
    source InternalSource for Model { //I
        include {
            //24
            nested_include {
                //25



                deeper_nested_include
                //26
            }
        }
    }
    "#;

    let (parse_ast, lex_results, _) = lex_parse(src);
    let comment_map = &lex_results[0].comment_map;
    let expected_retained = comment_map.entries.len();

    // Act
    let formatted = formatter::format(&parse_ast, comment_map, src);
    let (_, res, _) = lex_parse(&formatted);

    // // Assert
    assert_eq!(
        res[0].comment_map.entries.len(),
        expected_retained,
        "should retain all comments"
    );

    insta::assert_snapshot!(formatted);
}

#[test]
fn target_keys_round_trip() {
    // Arrange: every spelling of the shared `::` initializer.
    let src = r#"durable ShardedDo {
    shard {
        tenant: int
        org: string
    }
}

durable GlobalDo {}

model A for ShardedDo::{ tenant, org(orgName) } {}

model B for GlobalDo {}

api A {
    get aliased -> json {
        t: int

        inject {
            ShardedDo::{ tenant(t), org(t) }
        }
    }

    get bare -> json {
        tenant: int

        inject {
            ShardedDo::tenant
        }
    }

    get global -> json {
        inject {
            GlobalDo::{}
        }
    }
}
"#;

    // Act
    let (ast, lex_results, _) = lex_parse(src);
    let formatted = formatter::format(&ast, &lex_results[0].comment_map, src);

    // Assert: each `::` spelling survives verbatim. In particular the alias-less
    // `::tenant` is not expanded, and `GlobalDo::{}` keeps its explicit empty braces
    // rather than collapsing to a bare binding.
    for spelling in [
        "model A for ShardedDo::{ tenant, org(orgName) } {}",
        "model B for GlobalDo {}",
        "ShardedDo::{ tenant(t), org(t) }",
        "ShardedDo::tenant\n",
        "GlobalDo::{}",
    ] {
        assert!(
            formatted.contains(spelling),
            "expected `{spelling}` in:\n{formatted}"
        );
    }

    // And formatting is idempotent over all of them.
    let (reparsed, relex, _) = lex_parse(&formatted);
    assert_eq!(
        formatter::format(&reparsed, &relex[0].comment_map, &formatted),
        formatted
    );
}
