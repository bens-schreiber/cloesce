use std::collections::HashMap;

use ast::{D1Database, KVNamespace, R2Bucket, WranglerSpec};
use compiler_test::lex_and_parse;
use semantic::{SemanticAnalysis, err::CompilerErrorKind};

// TODO: use wrangler defaults
fn create_spec() -> WranglerSpec {
    WranglerSpec {
        d1_databases: vec![D1Database {
            binding: Some("my_d1".into()),
            database_name: None,
            database_id: None,
            migrations_dir: None,
        }],
        kv_namespaces: vec![KVNamespace {
            binding: Some("my_kv".into()),
            id: None,
        }],
        r2_buckets: vec![R2Bucket {
            binding: Some("my_r2".into()),
            bucket_name: None,
        }],
        vars: HashMap::new(),
        name: None,
        compatibility_date: None,
        main: None,
    }
}

fn with_env(src: &str) -> String {
    format!(
        r#"
        env {{
            my_d1: d1
            my_kv: kv 
            my_r2: r2
        }}

        {}
    "#,
        src
    )
}

fn assert_errors_eq(got: Vec<CompilerErrorKind>, expected: Vec<CompilerErrorKind>) {
    let mut got_sorted = got.clone();
    got_sorted.sort();
    let mut expected_sorted = expected.clone();
    expected_sorted.sort();
    assert_eq!(got_sorted, expected_sorted);
}

#[test]
fn multiple_wrangler_env_blocks() {
    // Arrange
    let src = r#"
        env {}
        env {}
    "#;
    let parse = lex_and_parse(src);

    // Act
    let res = SemanticAnalysis::analyze(parse, &create_spec());

    // Assert
    assert_eq!(
        res.unwrap_err(),
        vec![CompilerErrorKind::MultipleWranglerEnvBlocks]
    );
}

#[test]
fn missing_wrangler_env_block() {
    // Arrange
    let src = r#"
        model User {}
    "#;
    let parse = lex_and_parse(src);

    // Act
    let res = SemanticAnalysis::analyze(parse, &create_spec());

    // Assert
    assert_eq!(
        res.unwrap_err(),
        vec![CompilerErrorKind::MissingWranglerEnvBlock]
    );
}

#[test]
fn wrangler_binding_inconsistent_with_spec() {
    // Arrange
    let src = r#"
        env {
            my_d1: d1
            my_kv: kv 
            my_r2: r2
            other_d1: d1 // NOT consistent with the spec
        }
    "#;
    let parse = lex_and_parse(src);

    // Act
    let res = SemanticAnalysis::analyze(parse, &create_spec());

    // Assert
    assert_eq!(
        res.unwrap_err(),
        vec![CompilerErrorKind::WranglerBindingInconsistentWithSpec]
    );
}

#[test]
fn wrangler_duplicate_symbol() {
    // Arrange
    let src = r#"
        env {
            my_d1: d1
            my_kv: kv 
            my_r2: r2
            my_d1: d1 // duplicate symbol
        }
    "#;
    let parse = lex_and_parse(src);

    // Act
    let res = SemanticAnalysis::analyze(parse, &create_spec());

    // Assert
    assert_eq!(res.unwrap_err(), vec![CompilerErrorKind::DuplicateSymbol]);
}

#[test]
fn d1_model_basic_errors() {
    // Arrange
    let src = with_env(
        r#"
        @d1(my_d1)
        model User {
            // missing primary key
        }

        @d1(other_d1) // unresolved, not in spec
        model Post {}

        // missing binding
        model Comment {
            [primary id]
            id: Integer
        }
    "#,
    );
    let parse = lex_and_parse(&src);

    // Act
    let res = SemanticAnalysis::analyze(parse, &create_spec());

    // Assert
    assert_errors_eq(
        res.unwrap_err(),
        vec![
            CompilerErrorKind::UnresolvedSymbol,
            CompilerErrorKind::D1ModelMissingPrimaryKey,
            CompilerErrorKind::D1ModelMissingD1Binding,
        ],
    );
}

#[test]
fn d1_model_column_fk_errors() {
    // Arrange
    let src = r#"
        env {
            my_d1: d1
            my_kv: kv
            my_r2: r2
            other_d1: d1
        }

        @d1(my_d1)
        model User {
            [primary id]
            id: Option<int> // primary key cannot be nullable
            id: int // duplicate symbol
            name: Object // invalid column type
            value: int
            str_value: string

            [foreign value -> Post::invalid] // invalid foreign key reference
            [foreign str_value -> Post::id] // foreign key references incompatible column type
            [foreign value -> User::id] // foreign key cannot reference same model
            [foreign value -> NonD1Model::id] // foreign key references non-d1 model
            [foreign value -> OtherD1Model::id] // foreign key references model in different database
        }

        @d1(my_d1)
        model Post {
            [primary id]
            id: int
        }

        @d1(other_d1)
        model OtherD1Model {
            [primary id]
            id: int
        }

        model NonD1Model { }
    "#;
    let mut spec = create_spec();
    spec.d1_databases.push(D1Database {
        binding: Some("other_d1".into()),
        database_name: None,
        database_id: None,
        migrations_dir: None,
    });

    let parse = lex_and_parse(src);

    // Act
    let res = SemanticAnalysis::analyze(parse, &spec);

    // Assert
    assert_errors_eq(
        res.unwrap_err(),
        vec![
            CompilerErrorKind::NullablePrimaryKey,
            CompilerErrorKind::DuplicateSymbol,
            CompilerErrorKind::InvalidColumnType,
            CompilerErrorKind::ForeignKeyReferencesInvalidOrUnknownColumn,
            CompilerErrorKind::ForeignKeyReferencesIncompatibleColumnType,
            CompilerErrorKind::ForeignKeyReferenceSelf,
            CompilerErrorKind::ForeignKeyReferencesNonD1Model,
            CompilerErrorKind::ForeignKeyReferencesDifferentDatabase,
        ],
    );
}

#[test]
fn d1_model_consistent_nullability_error() {
    // Arrange
    let src = r#"
        env {
            my_d1: d1
        }

        @d1(my_d1)
        model User {
            [primary id, name]
            id: int

            postId: Option<int>
            name: string

            [foreign (postId, name) -> (Post::id, Post::title)] // inconsistent nullability
        }

        @d1(my_d1)
        model Post {
            [primary id, title]
            id: int
            title: string
        }
    "#;

    let parse = lex_and_parse(src);

    // Act
    let res = SemanticAnalysis::analyze(parse, &create_spec());

    // Assert
    assert_errors_eq(
        res.unwrap_err(),
        vec![CompilerErrorKind::ForeignKeyInconsistentNullability],
    );
}

#[test]
fn d1_model_fk_column_already_in_foreign_key() {
    // Arrange
    let src = with_env(
        r#"
        @d1(my_d1)
        model User {
            [primary id]
            id: int
            postId: int

            [foreign postId -> Post::id]
            [foreign postId -> Post::id] // same column in a second FK
        }

        @d1(my_d1)
        model Post {
            [primary id]
            id: int
        }
    "#,
    );
    let parse = lex_and_parse(&src);

    // Act
    let res = SemanticAnalysis::analyze(parse, &create_spec());

    // Assert
    assert_errors_eq(
        res.unwrap_err(),
        vec![CompilerErrorKind::ForeignKeyColumnAlreadyInForeignKey],
    );
}
