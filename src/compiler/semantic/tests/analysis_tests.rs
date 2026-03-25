#![allow(unused_variables)]

use std::collections::HashMap;

use ast::{
    CidlType, D1Database, KVNamespace, NavigationPropertyKind, R2Bucket, SymbolKind,
    WranglerEnvBindingKind, WranglerSpec,
};
use compiler_test::lex_and_parse;
use semantic::{SemanticAnalysis, err::CompilerErrorKind};

/// Find exactly one error matching the pattern. Panics if not found.
/// Destructure with `=> expr` to extract fields in one step.
macro_rules! expect_err {
    ($errors:expr, $pat:pat) => {
        $errors
            .iter()
            .find(|e| matches!(e, $pat))
            .unwrap_or_else(|| {
                panic!(
                    "expected error matching `{}`, got: {:#?}",
                    stringify!($pat),
                    $errors
                )
            })
    };
    ($errors:expr, $pat:pat => $result:expr) => {{
        let __found = expect_err!($errors, $pat);
        match __found {
            $pat => $result,
            _ => unreachable!(),
        }
    }};
}

macro_rules! count_errs {
    ($errors:expr, $pat:pat) => {
        $errors.iter().filter(|e| matches!(e, $pat)).count()
    };
}

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

#[test]
fn multiple_wrangler_env_blocks() {
    // Arrange
    let src = r#"
        env {}
        env {}
    "#;
    let parse = lex_and_parse(src);

    // Act
    let (ast, errors) = SemanticAnalysis::analyze(parse, &create_spec());

    // Assert
    assert_eq!(errors.len(), 1);
    let (first, second) = expect_err!(errors,
        CompilerErrorKind::MultipleWranglerEnvBlocks { first, second } => (*first, *second)
    );
    assert!(matches!(
        ast.table.kind(first),
        Some(SymbolKind::WranglerEnvDecl)
    ));
    assert!(matches!(
        ast.table.kind(second),
        Some(SymbolKind::WranglerEnvDecl)
    ));
}

#[test]
fn missing_wrangler_env_block() {
    // Arrange
    let src = r#"
        model User {}
    "#;
    let parse = lex_and_parse(src);

    // Act
    let (_table, errors) = SemanticAnalysis::analyze(parse, &create_spec());

    // Assert
    assert_eq!(errors.len(), 1);
    expect_err!(errors, CompilerErrorKind::MissingWranglerEnvBlock);
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
    let (ast, errors) = SemanticAnalysis::analyze(parse, &create_spec());

    // Assert
    assert_eq!(errors.len(), 1);
    let binding = expect_err!(errors,
        CompilerErrorKind::WranglerBindingInconsistentWithSpec { binding } => *binding
    );
    assert_eq!(ast.table.name(binding), "other_d1");
    assert!(matches!(
        ast.table.kind(binding),
        Some(SymbolKind::WranglerEnvBinding {
            kind: WranglerEnvBindingKind::D1
        })
    ));
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
    let (ast, errors) = SemanticAnalysis::analyze(parse, &create_spec());

    // Assert
    assert_eq!(errors.len(), 1);
    let symbol = expect_err!(errors,
        CompilerErrorKind::DuplicateSymbol { symbol, .. } => *symbol
    );
    assert_eq!(ast.table.name(symbol), "my_d1");
    assert!(matches!(
        ast.table.kind(symbol),
        Some(SymbolKind::WranglerEnvBinding {
            kind: WranglerEnvBindingKind::D1
        })
    ));
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
    let (ast, errors) = SemanticAnalysis::analyze(parse, &create_spec());

    // Assert
    assert_eq!(errors.len(), 3);

    // User has @d1 but no primary key
    let model = expect_err!(errors,
        CompilerErrorKind::D1ModelMissingPrimaryKey { model } => *model
    );
    assert_eq!(ast.table.name(model), "User");
    assert!(matches!(ast.table.kind(model), Some(SymbolKind::ModelDecl)));

    // Post references @d1(other_d1) which is not in the env block
    expect_err!(errors, CompilerErrorKind::UnresolvedSymbol { .. });

    // Comment has fields but no @d1 binding
    let model = expect_err!(errors,
        CompilerErrorKind::D1ModelMissingD1Binding { model } => *model
    );
    assert_eq!(ast.table.name(model), "Comment");
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
            value: int
            str_value: string

            [foreign value -> Post::invalid] // invalid foreign key reference
            [foreign str_value -> Post::id] // foreign key references incompatible column type
            [foreign value -> User::id] // foreign key cannot reference same model
            [foreign value -> NonD1Model::id] // foreign key references non-d1 model
            [foreign value -> OtherD1Model::id] // foreign key references model in different database
            [foreign doesNotExist -> Post::id] // foreign key column does not exist
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
    let (ast, errors) = SemanticAnalysis::analyze(parse, &spec);

    // Assert
    assert_eq!(errors.len(), 8);

    // Variant counts for repeated errors
    assert_eq!(
        count_errs!(
            errors,
            CompilerErrorKind::ForeignKeyReferencesInvalidOrUnknownColumn { .. }
        ),
        2
    );

    // Nullable primary key: id is Option<int>
    let column = expect_err!(errors,
        CompilerErrorKind::NullablePrimaryKey { column } => *column
    );
    assert_eq!(ast.table.name(column), "id");
    assert!(matches!(
        ast.table.kind(column),
        Some(SymbolKind::ModelField { .. })
    ));

    // Duplicate symbol: id declared twice
    let symbol = expect_err!(errors,
        CompilerErrorKind::DuplicateSymbol { symbol, .. } => *symbol
    );
    assert_eq!(ast.table.name(symbol), "id");

    // FK incompatible type: str_value (string) -> Post::id (int)
    let (column, adj_column) = expect_err!(errors,
        CompilerErrorKind::ForeignKeyReferencesIncompatibleColumnType { column, adj_column, .. } => (*column, *adj_column)
    );
    assert_eq!(ast.table.name(column), "str_value");
    assert_eq!(ast.table.name(adj_column), "id");

    // FK references self
    let (model, foreign_key) = expect_err!(errors,
        CompilerErrorKind::ForeignKeyReferenceSelf { model, foreign_key } => (*model, *foreign_key)
    );
    assert_eq!(ast.table.name(model), "User");
    assert!(matches!(
        ast.table.kind(foreign_key),
        Some(SymbolKind::ModelForeignKeyTag { .. })
    ));

    // FK references non-D1 model
    let model = expect_err!(errors,
        CompilerErrorKind::ForeignKeyReferencesNonD1Model { model, .. } => *model
    );
    assert_eq!(ast.table.name(model), "NonD1Model");

    // FK references different database
    let binding = expect_err!(errors,
        CompilerErrorKind::ForeignKeyReferencesDifferentDatabase { binding, .. } => *binding
    );
    assert_eq!(ast.table.name(binding), "other_d1");
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
    let (ast, errors) = SemanticAnalysis::analyze(parse, &create_spec());

    // Assert
    assert_eq!(errors.len(), 1);
    let (first_column, second_column) = expect_err!(errors,
        CompilerErrorKind::ForeignKeyInconsistentNullability { first_column, second_column, .. } => (*first_column, *second_column)
    );
    assert_eq!(ast.table.name(first_column), "postId");
    assert_eq!(ast.table.name(second_column), "name");
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
    let (ast, errors) = SemanticAnalysis::analyze(parse, &create_spec());

    // Assert
    assert_eq!(errors.len(), 1);
    let column = expect_err!(errors,
        CompilerErrorKind::ForeignKeyColumnAlreadyInForeignKey { column, .. } => *column
    );
    assert_eq!(ast.table.name(column), "postId");
    assert!(matches!(
        ast.table.kind(column),
        Some(SymbolKind::ModelField { .. })
    ));
}

#[test]
fn d1_model_nav_errors() {
    // Arrange
    let src = r#"
        env {
            my_d1: d1
            other_d1: d1
        }

        @d1(my_d1)
        model User {
            [primary id]
            id: int

            postNav: Post

            [nav unknown -> Post::id] // profile is not a declared field
            [nav postNav -> User::id] // self reference
            [nav postNav -> NonD1Model::id] // references non-D1 model
            [nav postNav -> DifferentDatabaseModel::id] // references model in different database
        }

        @d1(my_d1)
        model Post {
            [primary id]
            id: int
        }

        model NonD1Model {
            id: int
        }

        @d1(other_d1)
        model DifferentDatabaseModel {
            [primary id]
            id: int
        }
    "#;
    let mut spec = create_spec();
    spec.d1_databases.push(D1Database {
        binding: Some("other_d1".into()),
        database_name: None,
        database_id: None,
        migrations_dir: None,
    });
    let parse = lex_and_parse(&src);

    // Act
    let (ast, errors) = SemanticAnalysis::analyze(parse, &spec);

    // Assert
    assert_eq!(errors.len(), 4);
    let unknown_id = expect_err!(errors, CompilerErrorKind::UnresolvedSymbol { symbol: unknown_id } => *unknown_id);
    assert!(matches!(
        ast.table.kind(unknown_id),
        Some(SymbolKind::ModelNavigationTag { .. })
    ));

    let self_ref_model = expect_err!(errors,
        CompilerErrorKind::NavigationPropertyReferencesSelf { model, .. } => *model
    );
    assert_eq!(ast.table.name(self_ref_model), "User");

    let non_d1_model = expect_err!(errors,
        CompilerErrorKind::NavigationPropertyReferencesNonD1Model { model, .. } => *model
    );
    assert_eq!(ast.table.name(non_d1_model), "NonD1Model");
}

#[test]
fn d1_model_nav_one_to_one() {
    // Arrange
    let src = &with_env(
        r#"
        @d1(my_d1)
        model Person {
            [primary id]
            id: int

            [foreign horseId -> Horse::id]
            [nav horse -> Horse::id]
            horseId: int
            horse: Horse
        }

        @d1(my_d1)
        model Horse {
            [primary id]
            id: int
        }
        "#,
    );
    let parse = lex_and_parse(src);

    // Act
    let (ast, errors) = SemanticAnalysis::analyze(parse, &create_spec());

    // Assert
    assert_eq!(errors.len(), 0);

    let person = ast
        .models
        .values()
        .find(|m| ast.table.name(m.symbol) == "Person")
        .unwrap();
    let horse = ast
        .models
        .values()
        .find(|m| ast.table.name(m.symbol) == "Horse")
        .unwrap();

    let person_horse_nav = person.navigation_properties.first().unwrap();
    let person_nav_symbol = ast.table.lookup(person_horse_nav.symbol).unwrap();
    let SymbolKind::ModelNavigationTag {
        parent: person_horse_nav_parent,
    } = person_nav_symbol.kind
    else {
        unreachable!()
    };
    assert_eq!(person.symbol, person_horse_nav_parent);
    assert_eq!(person_horse_nav.adj_model, horse.symbol);

    let person_horse_field = ast.table.lookup(person_horse_nav.field).unwrap();
    let SymbolKind::ModelField {
        parent: person_horse_field_parent,
        cidl_type: person_horse_field_type,
    } = &person_horse_field.kind
    else {
        unreachable!()
    };

    assert_eq!(*person_horse_field_parent, person.symbol);
    assert_eq!(person_horse_field_type, &CidlType::Object(horse.symbol));

    let NavigationPropertyKind::OneToOne {
        columns: person_horse_nav_columns,
    } = &person_horse_nav.kind
    else {
        unreachable!()
    };

    assert_eq!(person_horse_nav_columns.len(), 1);
    let person_horse_nav_column_symbol = ast.table.lookup(person_horse_nav_columns[0]).unwrap();
    let SymbolKind::ModelField {
        parent: person_horse_nav_column_parent,
        cidl_type: person_horse_nav_column_type,
    } = &person_horse_nav_column_symbol.kind
    else {
        unreachable!()
    };
    assert_eq!(*person_horse_nav_column_parent, horse.symbol);
    assert_eq!(person_horse_nav_column_type, &CidlType::Integer);
}

#[test]
fn d1_model_nav_one_to_many() {
    // Arrange
    let src = &with_env(
        r#"
        @d1(my_d1)
        model Author {
            [primary id]
            id: int

            [nav posts -> Post::authorId]
            posts: Array<Post>
        }

        @d1(my_d1)
        model Post {
            [primary id]
            id: int

            [foreign authorId -> Author::id]
            authorId: int
        }
        "#,
    );

    // Act
    let parse = lex_and_parse(src);
    let (ast, errors) = SemanticAnalysis::analyze(parse, &create_spec());

    // Assert
    assert_eq!(errors.len(), 0);

    let author = ast
        .models
        .values()
        .find(|m| ast.table.name(m.symbol) == "Author")
        .unwrap();
    let post = ast
        .models
        .values()
        .find(|m| ast.table.name(m.symbol) == "Post")
        .unwrap();

    let author_posts_nav = author.navigation_properties.first().unwrap();
    let author_nav_symbol = ast.table.lookup(author_posts_nav.symbol).unwrap();
    let SymbolKind::ModelNavigationTag {
        parent: author_posts_nav_parent,
    } = author_nav_symbol.kind
    else {
        unreachable!()
    };
    assert_eq!(author.symbol, author_posts_nav_parent);
    assert_eq!(author_posts_nav.adj_model, post.symbol);

    let author_posts_field = ast.table.lookup(author_posts_nav.field).unwrap();
    let SymbolKind::ModelField {
        parent: author_posts_field_parent,
        cidl_type: author_posts_field_type,
    } = &author_posts_field.kind
    else {
        unreachable!()
    };
    assert_eq!(*author_posts_field_parent, author.symbol);
    assert_eq!(
        author_posts_field_type,
        &CidlType::array(CidlType::Object(post.symbol))
    );

    let NavigationPropertyKind::OneToMany {
        columns: author_posts_nav_columns,
    } = &author_posts_nav.kind
    else {
        unreachable!()
    };
    assert_eq!(author_posts_nav_columns.len(), 1);
    let author_posts_nav_column_symbol = ast.table.lookup(author_posts_nav_columns[0]).unwrap();
    let SymbolKind::ModelField {
        parent: author_posts_nav_column_parent,
        cidl_type: author_posts_nav_column_type,
    } = &author_posts_nav_column_symbol.kind
    else {
        unreachable!()
    };
    assert_eq!(*author_posts_nav_column_parent, post.symbol);
    assert_eq!(author_posts_nav_column_type, &CidlType::Integer);
}

#[test]
fn d1_model_nav_many_to_many() {
    // Arrange
    let src = &with_env(
        r#"
        @d1(my_d1)
        model Student {
            [primary id]
            id: int

            [nav courses <> Course::students]
            courses: Array<Course>
        }

        @d1(my_d1)
        model Course {
            [primary id]
            id: int

            [nav students <> Student::courses]
            students: Array<Student>
        }
        "#,
    );

    // Act
    let parse = lex_and_parse(src);
    let (ast, errors) = SemanticAnalysis::analyze(parse, &create_spec());

    // Assert
    assert_eq!(errors.len(), 0);

    let student = ast
        .models
        .values()
        .find(|m| ast.table.name(m.symbol) == "Student")
        .unwrap();
    let course = ast
        .models
        .values()
        .find(|m| ast.table.name(m.symbol) == "Course")
        .unwrap();

    let student_courses_nav = student.navigation_properties.first().unwrap();
    let student_nav_symbol = ast.table.lookup(student_courses_nav.symbol).unwrap();
    let SymbolKind::ModelNavigationTag {
        parent: student_courses_nav_parent,
    } = student_nav_symbol.kind
    else {
        unreachable!()
    };
    assert_eq!(student.symbol, student_courses_nav_parent);
    assert_eq!(student_courses_nav.adj_model, course.symbol);

    let student_courses_field = ast.table.lookup(student_courses_nav.field).unwrap();
    let SymbolKind::ModelField {
        parent: student_courses_field_parent,
        cidl_type: student_courses_field_type,
    } = &student_courses_field.kind
    else {
        unreachable!()
    };
    assert_eq!(*student_courses_field_parent, student.symbol);
    assert_eq!(
        student_courses_field_type,
        &CidlType::array(CidlType::Object(course.symbol))
    );

    let NavigationPropertyKind::ManyToMany = &student_courses_nav.kind else {
        unreachable!()
    };
}
