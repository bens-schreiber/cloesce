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
    assert_eq!(errors.len(), 7);

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
            [nav postNav -> DifferentDatabaseModel::id] // references model in different database
        }

        @d1(my_d1)
        model Post {
            [primary id]
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
    assert_eq!(errors.len(), 3);
    let unknown_id = expect_err!(errors, CompilerErrorKind::UnresolvedSymbol { symbol: unknown_id } => *unknown_id);
    assert!(matches!(
        ast.table.kind(unknown_id),
        Some(SymbolKind::ModelNavigationTag { .. })
    ));

    let self_ref_model = expect_err!(errors,
        CompilerErrorKind::NavigationPropertyReferencesSelf { model, .. } => *model
    );
    assert_eq!(ast.table.name(self_ref_model), "User");
}

#[test]
fn d1_model_nav_field_already_in_navigation_property() {
    // Arrange
    let src = with_env(
        r#"
        @d1(my_d1)
        model Person {
            [primary id]
            id: int

            [foreign horseId -> Horse::id]
            [nav horse -> Horse::id]
            [nav horse -> Horse::id] // same field used in a second nav
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
    let parse = lex_and_parse(&src);

    // Act
    let (ast, errors) = SemanticAnalysis::analyze(parse, &create_spec());

    // Assert
    assert_eq!(errors.len(), 1);
    let field = expect_err!(errors,
        CompilerErrorKind::NavigationPropertyFieldAlreadyInNavigationProperty { field, .. } => *field
    );
    assert_eq!(ast.table.name(field), "horse");
    assert!(matches!(
        ast.table.kind(field),
        Some(SymbolKind::ModelField { .. })
    ));
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

    assert_eq!(person.symbol, person_nav_symbol.parent);
    assert_eq!(person_horse_nav.adj_model, horse.symbol);

    let person_horse_field = ast.table.lookup(person_horse_nav.field).unwrap();

    assert_eq!(person_horse_field.parent, person.symbol);
    assert_eq!(person_horse_field.cidl_type, CidlType::Object(horse.symbol));

    let NavigationPropertyKind::OneToOne {
        columns: person_horse_nav_columns,
    } = &person_horse_nav.kind
    else {
        unreachable!()
    };

    assert_eq!(person_horse_nav_columns.len(), 1);
    let person_horse_nav_column_symbol = ast.table.lookup(person_horse_nav_columns[0]).unwrap();
    assert_eq!(person_horse_nav_column_symbol.parent, horse.symbol);
    assert_eq!(person_horse_nav_column_symbol.cidl_type, CidlType::Integer);
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

    assert_eq!(author.symbol, author_nav_symbol.parent);
    assert_eq!(author_posts_nav.adj_model, post.symbol);

    let author_posts_field = ast.table.lookup(author_posts_nav.field).unwrap();
    assert_eq!(author_posts_field.parent, author.symbol);
    assert_eq!(
        author_posts_field.cidl_type,
        CidlType::array(CidlType::Object(post.symbol))
    );

    let NavigationPropertyKind::OneToMany {
        columns: author_posts_nav_columns,
    } = &author_posts_nav.kind
    else {
        unreachable!()
    };
    assert_eq!(author_posts_nav_columns.len(), 1);
    let author_posts_nav_column_symbol = ast.table.lookup(author_posts_nav_columns[0]).unwrap();
    assert_eq!(author_posts_nav_column_symbol.parent, post.symbol);
    assert_eq!(author_posts_nav_column_symbol.cidl_type, CidlType::Integer);
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
    assert_eq!(student.symbol, student_nav_symbol.parent);
    assert_eq!(student_courses_nav.adj_model, course.symbol);

    let student_courses_field = ast.table.lookup(student_courses_nav.field).unwrap();
    assert_eq!(student_courses_field.parent, student.symbol);
    assert_eq!(
        student_courses_field.cidl_type,
        CidlType::array(CidlType::Object(course.symbol))
    );

    let NavigationPropertyKind::ManyToMany = &student_courses_nav.kind else {
        unreachable!()
    };
}

#[test]
fn d1_model_cyclical_relationship_error() {
    // Arrange
    let src = &with_env(
        r#"
        @d1(my_d1)
        model A {
            [primary id]
            id: int

            [foreign bId -> B::id]
            [nav toB -> B::id]
            bId: int
            toB: B
        }

        @d1(my_d1)
        model B {
            [primary id]
            id: int

            [foreign cId -> C::id]
            [nav toC -> C::id]
            cId: int
            toC: C
        }

        @d1(my_d1)
        model C {
            [primary id]
            id: int

            [foreign aId -> A::id]
            [nav toA -> A::id]
            aId: int
            toA: A
        }
        "#,
    );

    // Act
    let parse = lex_and_parse(src);
    let (ast, errors) = SemanticAnalysis::analyze(parse, &create_spec());

    // Assert
    assert_eq!(errors.len(), 1);
    let cycle = expect_err!(errors,
        CompilerErrorKind::CyclicalRelationship { cycle } => cycle.clone()
    );
    let cycle_names: Vec<&str> = cycle.iter().map(|&sym| ast.table.name(sym)).collect();
    assert_eq!(cycle_names, vec!["B", "A", "C"]);
}

#[test]
fn d1_model_nullability_prevents_cycle() {
    // Arrange
    let src = &with_env(
        r#"
        @d1(my_d1)
        model A {
            [primary id]
            id: int

            [foreign bId -> B::id]
            [nav toB -> B::id]
            bId: Option<int>
            toB: Option<B>
        }

        @d1(my_d1)
        model B {
            [primary id]
            id: int

            [foreign cId -> C::id]
            [nav toC -> C::id]
            cId: Option<int>
            toC: Option<C>
        }

        @d1(my_d1)
        model C {
            [primary id]
            id: int

            [foreign aId -> A::id]
            [nav toA -> A::id]
            aId: Option<int>
            toA: Option<A>
        }
        "#,
    );

    // Act
    let parse = lex_and_parse(src);
    let (ast, errors) = SemanticAnalysis::analyze(parse, &create_spec());

    // Assert
    assert_eq!(errors.len(), 0);
}

#[test]
fn kv_r2_errors() {
    // Arrange
    let src = &with_env(
        r#"
        model Foo {
            field: string

            @keyparam
            keyParam: int // can't be an int

            @kv(my_d1, "items/{field}") // invalid binding type
            foo: string

            @r2(my_kv, "assets/{field}") // invalid binding type
            obj: R2Object

            @kv(my_kv, "items/{field}/{nonexistent}") // unknown variable in format
            cached: string

            @r2(my_r2, "assets/{field") // invalid format, unclosed brace
            obj2: R2Object
        }
        "#,
    );
    let parse = lex_and_parse(src);

    // Act
    let (ast, errors) = SemanticAnalysis::analyze(parse, &create_spec());

    // Assert
    assert_eq!(errors.len(), 5);

    let key_param = expect_err!(errors,
        CompilerErrorKind::KvR2InvalidKeyParam { field, .. } => *field
    );
    assert_eq!(ast.table.name(key_param), "keyParam");

    let binding = expect_err!(errors,
        CompilerErrorKind::KvInvalidBinding { binding, ..} => *binding
    );
    assert_eq!(ast.table.name(binding), "my_d1");

    let binding = expect_err!(errors,
        CompilerErrorKind::R2InvalidBinding { binding, .. } => *binding
    );
    assert_eq!(ast.table.name(binding), "my_kv");

    let variable = expect_err!(errors,
        CompilerErrorKind::KvR2UnknownKeyVariable { variable, .. } => variable.clone()
    );
    assert_eq!(variable, "nonexistent");

    expect_err!(
        errors,
        CompilerErrorKind::KvR2InvalidKeyFormat { reason, .. }
    );
}

#[test]
fn kv_and_d1_coexist() {
    // A model can have both D1 and KV/R2 properties
    let src = &with_env(
        r#"
        @d1(my_d1)
        model User {
            [primary id]
            id: int
            name: string

            @kv(my_kv, "users/{id}")
            cached: string
        }
        "#,
    );
    let parse = lex_and_parse(src);

    // Act
    let (ast, errors) = SemanticAnalysis::analyze(parse, &create_spec());

    // Assert
    assert_eq!(errors.len(), 0);
    let user = ast
        .models
        .values()
        .find(|m| ast.table.name(m.symbol) == "User")
        .unwrap();
    assert!(user.d1_binding.is_some());
    assert_eq!(user.kv_properties.len(), 1);
    assert_eq!(user.columns.len(), 3); // id, name, cached
}

#[test]
fn api_errors() {
    // Arrange
    let src = &with_env(
        r#"
        @d1(my_d1)
        model User {
            [primary id]
            id: int
            name: string
        }

        // Unknown model reference
        api BadModelApi for NonExistentModel {}

        // Unknown return type (references non-existent object)
        api BadReturnApi for User {
            get badReturn() -> UnknownObj
        }

        // Void parameter
        api BadParamApi for User {
            post badVoidParam(v: void) -> string
        }

        // Object parameter on GET
        api GetObjectApi for User {
            get badGetObj(u: User) -> string
        }

        // R2Object parameter on GET
        api GetR2Api for User {
            get badGetR2(r: R2Object) -> string
        }

        // Stream param with extra non-inject params (invalid)
        api BadStreamApi for User {
            post badStream(s: stream, extra: string) -> stream
        }
    "#,
    );
    let parse = lex_and_parse(src);

    // Act
    let (ast, errors) = SemanticAnalysis::analyze(parse, &create_spec());

    // Assert
    assert_eq!(errors.len(), 6);

    expect_err!(errors, CompilerErrorKind::ApiUnknownModelReference { .. });

    expect_err!(errors, CompilerErrorKind::ApiInvalidReturn { .. });

    assert_eq!(
        count_errs!(errors, CompilerErrorKind::ApiInvalidParam { .. }),
        4
    );
}

#[test]
fn poo_errors() {
    // Arrange
    let src = r#"
        poo MyPoo {
            streamField: stream
            voidField: void
            cyclicalField: MyPoo
        }
    "#;

    // Act
    let parse = lex_and_parse(src);
    let (ast, errors) = SemanticAnalysis::analyze(parse, &create_spec());

    // Assert
    assert_eq!(errors.len(), 3);

    let cycle = expect_err!(errors,
        CompilerErrorKind::CyclicalRelationship { cycle } => cycle.clone()
    );
    let cycle_names: Vec<&str> = cycle.iter().map(|&sym| ast.table.name(sym)).collect();
    assert_eq!(cycle_names, vec!["MyPoo"]);

    assert!(errors.iter().find(|e| matches!(
        e,
        CompilerErrorKind::PlainOldObjectInvalidFieldType { field } if ast.table.name(*field) == "streamField"
    )).is_some());

    assert!(errors.iter().find(|e| matches!(
        e,
        CompilerErrorKind::PlainOldObjectInvalidFieldType { field } if ast.table.name(*field) == "voidField"
    )).is_some());
}

#[test]
fn service_errors() {
    let src = &with_env(
        r#"
        inject {
            OpenApiService
            YouTubeApi
        }

        @d1(my_d1)
        model User {
            [primary id]
            id: int
            name: string
        }

        // Error: primitive field type
        service BadPrimitive {
            name: string
        }

        // Error: model field type
        service BadModel {
            user: User
        }
    "#,
    );

    let parse = lex_and_parse(src);
    let (ast, errors) = SemanticAnalysis::analyze(parse, &create_spec());

    assert!(errors.iter().any(|e| matches!(
        e,
        CompilerErrorKind::ServiceInvalidFieldType { field }
            if ast.table.name(*field) == "name"
    )));

    assert!(errors.iter().any(|e| matches!(
        e,
        CompilerErrorKind::ServiceInvalidFieldType { field }
            if ast.table.name(*field) == "user"
    )));

    assert_eq!(
        count_errs!(errors, CompilerErrorKind::ServiceInvalidFieldType { .. }),
        2
    );
}
