#![allow(unused_variables)]

use ast::{CidlType, NavigationFieldKind};
use compiler_test::lex_and_parse;
use semantic::{SemanticAnalysis, SymbolKind, WranglerEnvBindingKind, err::CompilerErrorKind};

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
fn missing_wrangler_env_block() {
    // Arrange
    let src = r#"
        model User {}
    "#;
    let parse = lex_and_parse(src);

    // Act
    let (_, errors) = SemanticAnalysis::analyze(parse);

    // Assert
    expect_err!(errors, CompilerErrorKind::MissingWranglerEnvBlock);
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
    let (result, errors) = SemanticAnalysis::analyze(parse);

    // Assert
    assert_eq!(errors.len(), 1);
    let second = expect_err!(errors,
        CompilerErrorKind::DuplicateSymbol { second, .. } => second
    );
    assert_eq!(second.name, "my_d1");
    assert!(matches!(
        second.kind,
        SymbolKind::WranglerEnvBinding {
            kind: WranglerEnvBindingKind::D1
        }
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
    let (result, errors) = SemanticAnalysis::analyze(parse);

    // Assert
    assert_eq!(errors.len(), 3);

    // User has @d1 but no primary key
    let model = expect_err!(errors,
        CompilerErrorKind::D1ModelMissingPrimaryKey { model } => model
    );
    assert_eq!(model.name, "User");
    assert!(matches!(model.kind, SymbolKind::ModelDecl));

    // Post references @d1(other_d1) which is not in the env block
    expect_err!(errors, CompilerErrorKind::D1ModelInvalidD1Binding { .. });

    // Comment has fields but no @d1 binding
    let model = expect_err!(errors,
        CompilerErrorKind::D1ModelMissingD1Binding { model } => model
    );
    assert_eq!(model.name, "Comment");
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
    let parse = lex_and_parse(src);

    // Act
    let (result, errors) = SemanticAnalysis::analyze(parse);

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
        CompilerErrorKind::NullablePrimaryKey { column } => column
    );
    assert_eq!(column.name, "id");
    assert!(matches!(column.kind, SymbolKind::ModelField { .. }));

    // Duplicate symbol: id declared twice
    let second = expect_err!(errors,
        CompilerErrorKind::DuplicateSymbol { second, .. } => second
    );
    assert_eq!(second.name, "id");

    // FK incompatible type: str_value (string) -> Post::id (int)
    let (column, adj_column) = expect_err!(errors,
        CompilerErrorKind::ForeignKeyReferencesIncompatibleColumnType { column, adj_column, .. } => (column, adj_column)
    );
    assert_eq!(column.name, "str_value");
    assert_eq!(adj_column.name, "id");

    // FK references self
    let model = expect_err!(errors,
        CompilerErrorKind::ForeignKeyReferencesSelf { model, .. } => model
    );
    assert_eq!(model.name, "User");

    // FK references different database
    let binding = expect_err!(errors,
        CompilerErrorKind::ForeignKeyReferencesDifferentDatabase { binding, .. } => binding.clone()
    );
    assert_eq!(binding, "other_d1");
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
    let (result, errors) = SemanticAnalysis::analyze(parse);

    // Assert
    assert_eq!(errors.len(), 1);
    let (first_column, second_column) = expect_err!(errors,
        CompilerErrorKind::ForeignKeyInconsistentNullability { first_column, second_column, .. } => (first_column, second_column)
    );
    assert_eq!(first_column.name, "postId");
    assert_eq!(second_column.name, "name");
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
    let (result, errors) = SemanticAnalysis::analyze(parse);

    // Assert
    assert_eq!(errors.len(), 1);
    let column = expect_err!(errors,
        CompilerErrorKind::ForeignKeyColumnAlreadyInForeignKey { column, .. } => column
    );
    assert_eq!(column.name, "postId");
    assert!(matches!(column.kind, SymbolKind::ModelField { .. }));
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
    let parse = lex_and_parse(&src);

    // Act
    let (result, errors) = SemanticAnalysis::analyze(parse);

    // Assert
    assert_eq!(errors.len(), 2);
    expect_err!(errors, CompilerErrorKind::UnresolvedSymbol { .. });
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
    let (result, errors) = SemanticAnalysis::analyze(parse);

    // Assert
    assert_eq!(errors.len(), 1, "unexpected errors: {:#?}", errors);
    let field = expect_err!(errors,
        CompilerErrorKind::NavigationPropertyFieldAlreadyInNavigationProperty { field, .. } => field
    );
    assert_eq!(field.name, "horse");
    assert!(matches!(field.kind, SymbolKind::ModelField { .. }));
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
            [nav horse -> Person::horseId]
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
    let (result, errors) = SemanticAnalysis::analyze(parse);

    // Assert
    assert_eq!(errors.len(), 0, "unexpected errors: {:#?}", errors);

    let person = result.ast.models.get("Person").unwrap();

    let person_horse_nav = person.navigation_properties.first().unwrap();
    assert_eq!(person_horse_nav.field.name, "horse");
    assert_eq!(person_horse_nav.model_reference, "Horse");

    assert_eq!(
        person_horse_nav.field.cidl_type,
        CidlType::Object {
            name: "Horse".to_string(),
        }
    );

    let NavigationFieldKind::OneToOne {
        columns: person_horse_nav_columns,
    } = &person_horse_nav.kind
    else {
        unreachable!()
    };

    assert_eq!(person_horse_nav_columns.len(), 1);
    assert_eq!(person_horse_nav_columns[0], "horseId");
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
    let (result, errors) = SemanticAnalysis::analyze(parse);

    // Assert
    assert_eq!(errors.len(), 0);

    let author = result.ast.models.get("Author").unwrap();

    let author_posts_nav = author.navigation_properties.first().unwrap();
    assert_eq!(author_posts_nav.field.name, "posts");
    assert_eq!(author_posts_nav.model_reference, "Post");

    let NavigationFieldKind::OneToMany {
        columns: author_posts_nav_columns,
    } = &author_posts_nav.kind
    else {
        unreachable!()
    };
    assert_eq!(author_posts_nav_columns.len(), 1);
    assert_eq!(author_posts_nav_columns[0], "authorId");
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
    let (result, errors) = SemanticAnalysis::analyze(parse);

    // Assert
    assert_eq!(errors.len(), 0);

    let student = result.ast.models.get("Student").unwrap();

    let student_courses_nav = student.navigation_properties.first().unwrap();
    assert_eq!(student_courses_nav.field.name, "courses");
    assert_eq!(student_courses_nav.model_reference, "Course");

    let NavigationFieldKind::ManyToMany = &student_courses_nav.kind else {
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
    let (result, errors) = SemanticAnalysis::analyze(parse);

    // Assert
    assert_eq!(errors.len(), 1);
    let cycle = expect_err!(errors,
        CompilerErrorKind::CyclicalRelationship { cycle } => cycle.clone()
    );
    // Cycle contains model names now
    assert_eq!(cycle.len(), 3);
    assert!(cycle.contains(&"A".to_string()));
    assert!(cycle.contains(&"B".to_string()));
    assert!(cycle.contains(&"C".to_string()));
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
    let (_, errors) = SemanticAnalysis::analyze(parse);

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
    let (result, errors) = SemanticAnalysis::analyze(parse);

    // Assert
    assert_eq!(errors.len(), 5);

    let field = expect_err!(errors,
        CompilerErrorKind::KvR2InvalidKeyParam { field, .. } => field
    );
    assert_eq!(field.name, "keyParam");

    let binding = expect_err!(errors,
        CompilerErrorKind::KvInvalidBinding { binding, ..} => binding.clone()
    );
    assert_eq!(binding, "my_d1");

    let binding = expect_err!(errors,
        CompilerErrorKind::R2InvalidBinding { binding, .. } => binding.clone()
    );
    assert_eq!(binding, "my_kv");

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
    let (result, errors) = SemanticAnalysis::analyze(parse);

    // Assert
    assert_eq!(errors.len(), 0);
    let user = result.ast.models.get("User").unwrap();
    assert!(user.d1_binding.is_some());
    assert_eq!(user.kv_fields.len(), 1);
    // id is primary, name + cached are regular columns
    assert_eq!(user.columns.len(), 2);
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
    let (_, errors) = SemanticAnalysis::analyze(parse);

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
fn data_source_errors() {
    // Arrange
    let src = &with_env(
        r#"
        @d1(my_d1)
        model User {
            [primary id]
            id: int
            name: string

            @kv(my_kv, "users/{id}")
            cached: string

            @r2(my_r2, "avatars/{id}")
            avatar: R2Object

            [nav posts -> Post::authorId]
            posts: Array<Post>
        }

        @d1(my_d1)
        model Post {
            [primary id]
            id: int
            title: string

            [foreign authorId -> User::id]
            authorId: int
        }

        // Unknown model reference
        source BadModelSource for NonExistent {
            include { nonexistent }
        }

        // Invalid include tree reference
        source BadTreeSource for User {
            include { nonexistent }
        }

        // Invalid nested include tree reference
        source BadNestedTreeSource for User {
            include { posts { bogus } }
        }

        // Invalid method param type (Object, not a sqlite type)
        source BadParamSource for User {
            include { posts }
            sql get(u: User) {
                "SELECT * FROM users WHERE id = ?"
            }
        }

    "#,
    );
    let parse = lex_and_parse(src);

    // Act
    let (_, errors) = SemanticAnalysis::analyze(parse);

    // Assert
    // BadModelSource: unknown model
    expect_err!(
        errors,
        CompilerErrorKind::DataSourceUnknownModelReference { .. }
    );

    // BadTreeSource: "nonexistent" is not a field on User
    assert!(errors.iter().any(|e| matches!(
        e,
        CompilerErrorKind::DataSourceInvalidIncludeTreeReference { name, .. }
            if name == "nonexistent"
    )));

    // BadNestedTreeSource: "bogus" is not a field on Post
    assert!(errors.iter().any(|e| matches!(
        e,
        CompilerErrorKind::DataSourceInvalidIncludeTreeReference { name, .. }
            if name == "bogus"
    )));

    // BadParamSource: User is not a valid sql type
    expect_err!(
        errors,
        CompilerErrorKind::DataSourceInvalidMethodParam { .. }
    );
}

#[test]
fn data_source_include_tree_kv_r2() {
    // Arrange
    let src = &with_env(
        r#"
        @d1(my_d1)
        model User {
            [primary id]
            id: int
            name: string

            @kv(my_kv, "users/{id}")
            cached: string

            @r2(my_r2, "avatars/{id}")
            avatar: R2Object
        }

        source WithKvR2 for User {
            include { cached, avatar }
        }
    "#,
    );
    let parse = lex_and_parse(src);

    // Act
    let (result, errors) = SemanticAnalysis::analyze(parse);

    // Assert
    assert_eq!(errors.len(), 0);

    let user = result.ast.models.get("User").unwrap();
    assert_eq!(user.data_sources.len(), 1);
    assert_eq!(user.data_sources[0].name, "WithKvR2");
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
    let (result, errors) = SemanticAnalysis::analyze(parse);

    // Assert
    assert_eq!(errors.len(), 3);

    let cycle = expect_err!(errors,
        CompilerErrorKind::CyclicalRelationship { cycle } => cycle.clone()
    );
    assert_eq!(cycle, vec!["MyPoo"]);

    assert!(errors.iter().any(|e| matches!(
        e,
        CompilerErrorKind::PlainOldObjectInvalidFieldType { field } if field.name == "streamField"
    )));

    assert!(errors.iter().any(|e| matches!(
        e,
        CompilerErrorKind::PlainOldObjectInvalidFieldType { field } if field.name == "voidField"
    )));
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
    let (result, errors) = SemanticAnalysis::analyze(parse);

    assert!(errors.iter().any(|e| matches!(
        e,
        CompilerErrorKind::ServiceInvalidFieldType { field }
            if field.name == "name"
    )));

    assert!(errors.iter().any(|e| matches!(
        e,
        CompilerErrorKind::ServiceInvalidFieldType { field }
            if field.name == "user"
    )));

    assert_eq!(
        count_errs!(errors, CompilerErrorKind::ServiceInvalidFieldType { .. }),
        2
    );
}
