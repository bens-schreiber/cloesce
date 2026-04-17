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
    // Arrange
    let (parse_ast, lex_result) = lex_parse(COMPREHENSIVE_SRC);
    let comment_map = &lex_result.results[0].comment_map;

    // Act
    let formatted = Formatter::format(&parse_ast, comment_map, COMPREHENSIVE_SRC);
    let (reparse_ast, _) = lex_parse(&formatted);

    // Assert
    assert_eq!(
        parse_ast.blocks.len(),
        reparse_ast.blocks.len(),
        "block count mismatch"
    );
    insta::assert_snapshot!(formatted);
}

#[test]
fn format_idempotent() {
    // Arrange
    let (parse_ast, lex_result) = lex_parse(COMPREHENSIVE_SRC);
    let comment_map = &lex_result.results[0].comment_map;

    // Act
    let formatted = Formatter::format(&parse_ast, comment_map, COMPREHENSIVE_SRC);
    let (reparse_ast, relex_result) = lex_parse(&formatted);
    let reformatted = Formatter::format(
        &reparse_ast,
        &relex_result.results[0].comment_map,
        &formatted,
    );

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
    env {
        //1
        d1 { 
            //2
            db 
            //A
        }//3
        //B
    }

    //4
    [use db] //5
    //6
    model BasicModel { //C
        //7
        primary { //D
            id: int //8
        } //9

        //14
        foreign (OneToManyModel::id) { //E
            fk_to_model //15
            //16
            nav { //F
                oneToOneNav //18
            } //G
        } //19
        //H
    } //20
    "#;

    let (parse_ast, lex_result) = lex_parse(src);
    let comment_map = &lex_result.results[0].comment_map;
    let expected_retained = comment_map.entries.len();

    // Act
    let formatted = Formatter::format(&parse_ast, comment_map, src);
    let (_, res) = lex_parse(&formatted);

    // // Assert
    assert_eq!(
        res.results[0].comment_map.entries.len(),
        expected_retained,
        "should retain all comments"
    );

    insta::assert_snapshot!(formatted);
}
