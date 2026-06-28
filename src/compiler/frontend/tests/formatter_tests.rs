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
