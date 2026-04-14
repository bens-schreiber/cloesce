#![allow(unused_variables)]

use ast::{CidlType, Field, MediaType, NavigationFieldKind};
use compiler_test::lex_and_parse;
use frontend::{EnvBindingKind, SymbolKind};
use semantic::{SemanticAnalysis, err::SemanticError};

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
            d1 {{ my_d1 }}
            kv {{ my_kv }}
            r2 {{ my_r2 }}
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
    let (_, errors) = SemanticAnalysis::analyze(&parse);

    // Assert
    expect_err!(errors, SemanticError::MissingWranglerEnvBlock);
}

#[test]
fn wrangler_duplicate_symbol() {
    // Arrange
    let src = r#"
        env {
            d1 {
                my_d1
            }
            d1 {
                my_d1 // duplicate symbol
            }
        }
    "#;
    let parse = lex_and_parse(src);

    // Act
    let (result, errors) = SemanticAnalysis::analyze(&parse);

    // Assert
    assert_eq!(errors.len(), 1);
    let second = expect_err!(errors,
        SemanticError::DuplicateSymbol { second, .. } => second
    );
    assert_eq!(second.name, "my_d1");
    assert!(matches!(
        second.kind,
        SymbolKind::EnvBinding {
            kind: EnvBindingKind::D1
        }
    ));
}

#[test]
fn d1_model_basic_errors() {
    // Arrange
    let src = with_env(
        r#"
        [use my_d1]
        model User {
            // missing primary key
        }

        [use other_d1] // unresolved, not in spec
        model Post {}

        // missing binding
        model Comment {
            primary { id: int }
            
        }
    "#,
    );
    let parse = lex_and_parse(&src);

    // Act
    let (result, errors) = SemanticAnalysis::analyze(&parse);

    // Assert
    assert_eq!(errors.len(), 3);

    // User has @d1 but no primary key
    let model = expect_err!(errors,
        SemanticError::D1ModelMissingPrimaryKey { model } => model
    );
    assert_eq!(model.name, "User");
    assert!(matches!(model.kind, SymbolKind::ModelDecl));

    // Post references @d1(other_d1) which is not in the env block
    expect_err!(errors, SemanticError::D1ModelInvalidD1Binding { .. });

    // Comment has fields but no @d1 binding
    let model = expect_err!(errors,
        SemanticError::D1ModelMissingD1Binding { model } => model
    );
    assert_eq!(model.name, "Comment");
}

#[test]
fn d1_model_column_fk_errors() {
    // Arrange
    let src = r#"
        env {
            d1 { 
                my_d1 
                other_d1
            }
        }

        [use my_d1]
        model User {
            primary {
                id: Option<int> // primary key cannot be nullable
            }

            id: int // duplicate symbol

            foreign (Post::invalid) {
                doesntExist
            }

            foreign (User::id) {
                shouldError
            }

            foreign (OtherD1Model::id) {
                shouldAlsoError
            }

            foreign (Post::id) {
                validForeignKey
            }

            foreign (DoesNotExist::id) {
                adjacentModelDoesNotExist
            }

            foreign (Post::nonexistent) {
                adjacentFieldDoesNotExist
            }

            foreign (Post::id, User::id) {
                referencesMultipleAdjacentModels
            }

            foreign (Post::id, Post::id) {
                inconsistentFieldAdjacency
            } 
        }

        [use my_d1]
        model Post {
            primary {
                id: int
            }
        }

        [use other_d1]
        model OtherD1Model {
            primary {
                id: int
            }
        }
    "#;
    let parse = lex_and_parse(src);

    // Act
    let (result, errors) = SemanticAnalysis::analyze(&parse);

    // Assert
    assert_eq!(errors.len(), 9);

    let column = expect_err!(errors,
        SemanticError::NullablePrimaryKey { column } => column
    );
    assert_eq!(column.name, "id");
    assert!(matches!(column.kind, SymbolKind::ModelField));

    let second = expect_err!(errors,
        SemanticError::DuplicateSymbol { second, .. } => second
    );
    assert_eq!(second.name, "id");

    let model = expect_err!(errors,
        SemanticError::ForeignKeyReferencesSelf { model, .. } => model
    );
    assert_eq!(model.name, "User");

    let binding = expect_err!(errors,
        SemanticError::ForeignKeyReferencesDifferentDatabase { binding, .. } => *binding
    );
    assert_eq!(binding, "other_d1");

    let inconsistent_model_adj = expect_err!(
        errors,
        SemanticError::InconsistentModelAdjacency {
            first_model,
            second_model,
            ..
        } => (first_model, second_model)
    );
    assert_eq!(*inconsistent_model_adj.0, "Post");
    assert_eq!(*inconsistent_model_adj.1, "User");

    let inconsistent_field_adj = expect_err!(
        errors,
        SemanticError::ForeignKeyInconsistentFieldAdj {
            adj_count,
            field_count,
            ..
        } => (adj_count, field_count)
    );
    assert_eq!(*inconsistent_field_adj.0, 2);
    assert_eq!(*inconsistent_field_adj.1, 1);

    let does_not_exist = errors
        .iter()
        .filter_map(|e| match e {
            SemanticError::UnresolvedSymbol { name, .. }
                if *name == "DoesNotExist" || *name == "nonexistent" =>
            {
                Some(name)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(does_not_exist.len(), 2);
}

#[test]
fn d1_model_nav_errors() {
    // Arrange
    let src = r#"
        env {
            d1 {
                my_d1
                other_d1
            }
        }

        [use my_d1]
        model User {
            primary {
                id: int
            }

            nav (Post::id, User::id) {
                inconsistentModelAdjacency
            }

            nav (DifferentDatabaseModel::id) {
                invalidAdjModel
            }

            nav (Post::id) {
                posts
            }
        }

        [use my_d1]
        model Post {
            primary {
                id: int
            }

            nav (User::id) {
                users1
            }

            nav (User::id) {
                users2
            }
        }

        [use other_d1]
        model DifferentDatabaseModel {
            primary {
                id: int
            }
        }
    "#;
    let parse = lex_and_parse(src);

    // Act
    let (result, errors) = SemanticAnalysis::analyze(&parse);

    // Assert
    assert_eq!(errors.len(), 5);

    let inconsistent_model_adj = expect_err!(
        errors,
        SemanticError::InconsistentModelAdjacency {
            first_model,
            second_model,
            ..
        } => (first_model, second_model)
    );
    assert_eq!(*inconsistent_model_adj.0, "Post");
    assert_eq!(*inconsistent_model_adj.1, "User");

    let binding = expect_err!(errors,
        SemanticError::NavigationReferencesDifferentDatabase { binding, .. } => *binding
    );
    assert_eq!(binding, "other_d1");

    let ambiguous_m2ms = count_errs!(errors, SemanticError::NavigationAmbiguousM2M { .. });
    assert_eq!(ambiguous_m2ms, 3);
}

#[test]
fn d1_model_nav_one_to_one() {
    // Arrange
    let src = &with_env(
        r#"
        [use my_d1]
        model Person {
            primary {
                id: int
            }

            foreign (Horse::id) {
                horseId
                nav { horse }
            }
        }

        [use my_d1]
        model Horse {
            primary {
                id: int
            }
        }
        "#,
    );
    let parse = lex_and_parse(src);

    // Act
    let (result, errors) = SemanticAnalysis::analyze(&parse);

    // Assert
    assert_eq!(errors.len(), 0, "unexpected errors: {:#?}", errors);

    let person = result.models.get("Person").unwrap();
    assert!(person.columns.len() == 1);
    assert!(person.primary_columns.len() == 1);
    assert!(
        person
            .columns
            .iter()
            .any(|c| c.field.name == "horseId" && matches!(c.field.cidl_type, CidlType::Integer))
    );
    assert!(person.navigation_fields.iter().any(|nav| {
        nav.field.name == "horse"
            && nav.model_reference == "Horse"
            && matches!(&nav.kind, NavigationFieldKind::OneToOne { columns } if columns.len() == 1 && columns[0] == "horseId")
            && nav.field.cidl_type == CidlType::Object {
                name: "Horse"
            }
    }));
}

#[test]
fn d1_model_nav_one_to_many() {
    // Arrange
    let src = &with_env(
        r#"
        [use my_d1]
        model Author {
            primary { id: int }

            nav (Post::authorId) {
                posts
            }
        }

        [use my_d1]
        model Post {
            primary { id: int }

            foreign (Author::id) {
                authorId
            }
        }
        "#,
    );

    // Act
    let parse = lex_and_parse(src);
    let (result, errors) = SemanticAnalysis::analyze(&parse);

    // Assert
    assert_eq!(errors.len(), 0, "unexpected errors: {:#?}", errors);

    let author = result.models.get("Author").unwrap();

    let author_posts_nav = author.navigation_fields.first().unwrap();
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
        [use my_d1]
        model Student {
            primary { id: int }

            nav (Course::id) {
                courses
            }
        }

        [use my_d1]
        model Course {
            primary { id: int }

            nav (Student::id) {
                students
            }
        }
        "#,
    );

    // Act
    let parse = lex_and_parse(src);
    let (result, errors) = SemanticAnalysis::analyze(&parse);

    // Assert
    assert_eq!(errors.len(), 0);

    let student = result.models.get("Student").unwrap();

    let student_courses_nav = student.navigation_fields.first().unwrap();
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
        [use my_d1]
        model A {
            primary { id: int }

            foreign (B::id) {
                bId2
                nav { toB }
            }
        }

        [use my_d1]
        model B {
            primary { id: int }

            foreign (C::id) {
                cId
                nav { toC }
            }
        }

        [use my_d1]
        model C {
            primary { id: int }

            foreign (A::id) {
                aId
                nav { toA }
            }
        }
        "#,
    );

    // Act
    let parse = lex_and_parse(src);
    let (result, errors) = SemanticAnalysis::analyze(&parse);

    // Assert
    assert_eq!(errors.len(), 1);
    let cycle = expect_err!(errors,
        SemanticError::CyclicalRelationship { cycle } => cycle.clone()
    );
    // Cycle contains model names now
    assert_eq!(cycle.len(), 3);
    assert!(cycle.contains(&"A"));
    assert!(cycle.contains(&"B"));
    assert!(cycle.contains(&"C"));
}

#[test]
fn d1_model_nullability_prevents_cycle() {
    // Arrange
    let src = &with_env(
        r#"
        [use my_d1]
        model A {
            primary { id: int }

            foreign (B::id) optional {
                bId
                nav { toB }
            }
        }

        [use my_d1]
        model B {
            primary { id: int }

            foreign (C::id) optional {
                cId
                nav { toC }
            }
        }

        [use my_d1]
        model C {
            primary { id: int }

            foreign (A::id) optional {
                aId
                nav { toA }
            }
        }
        "#,
    );

    // Act
    let parse = lex_and_parse(src);
    let (_, errors) = SemanticAnalysis::analyze(&parse);

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

            kv(my_d1, "items/{field}") { // invalid binding type (my_d1 is a D1, not KV)
                foo: json
            }

            r2(my_kv, "assets/{field}") { // invalid binding type (my_kv is a KV, not R2)
                obj
            }

            kv(my_kv, "items/{field}/{nonexistent}") { // unknown variable in format
                cached: json
            }

            r2(my_r2, "assets/{field") { // invalid format, unclosed brace
                obj2
            }
        }
        "#,
    );
    let parse = lex_and_parse(src);

    // Act
    let (result, errors) = SemanticAnalysis::analyze(&parse);

    // Assert
    assert_eq!(errors.len(), 4);

    let binding = expect_err!(errors,
        SemanticError::KvInvalidBinding { binding, ..} => *binding
    );
    assert_eq!(binding, "my_d1");

    let binding = expect_err!(errors,
        SemanticError::R2InvalidBinding { binding, .. } => *binding
    );
    assert_eq!(binding, "my_kv");

    let variable = expect_err!(errors,
        SemanticError::KvR2UnknownKeyVariable { variable, .. } => *variable
    );
    assert_eq!(variable, "nonexistent");

    expect_err!(errors, SemanticError::KvR2InvalidKeyFormat { .. });
}

#[test]
fn kv_and_d1_coexist() {
    // A model can have both D1 and KV/R2 properties
    let src = &with_env(
        r#"
        [use my_d1]
        model User {
            primary {
                id: int
            }
            name: string

            kv(my_kv, "users/{id}") {
                cached: json
            }
        }
        "#,
    );
    let parse = lex_and_parse(src);

    // Act
    let (result, errors) = SemanticAnalysis::analyze(&parse);

    // Assert
    assert_eq!(errors.len(), 0, "unexpected errors: {:#?}", errors);
    let user = result.models.get("User").unwrap();
    assert!(user.d1_binding.is_some());
    assert_eq!(user.kv_fields.len(), 1);
    assert_eq!(user.columns.len(), 1,);
    assert_eq!(
        user.kv_fields[0].format_parameters[0],
        Field {
            name: "id".into(),
            cidl_type: CidlType::Integer,
        }
    );
}

#[test]
fn api_errors() {
    // Arrange
    let src = &with_env(
        r#"
        [use my_d1]
        model User {
            primary {
                id: int
            }
            name: string
        }

        // Unknown model reference
        api NonExistentModel {}

        // Invalid return type
        api User {
            get badReturn() -> Option<stream>
        }

        // Void parameter
        api User {
            post badVoidParam(v: void) -> string
        }

        // Object parameter on GET
        api User {
            get badGetObj(u: User) -> string
        }

        // R2Object parameter on GET
        api User {
            get badGetR2(r: R2Object) -> string
        }

        // Stream param with extra non-inject params (invalid)
        api User {
            post badStream(s: stream, extra: string) -> stream
        }
    "#,
    );
    let parse = lex_and_parse(src);

    // Act
    let (_, errors) = SemanticAnalysis::analyze(&parse);

    // Assert
    assert_eq!(errors.len(), 6);

    expect_err!(errors, SemanticError::ApiUnknownNamespaceReference { .. });

    expect_err!(errors, SemanticError::ApiInvalidReturn { .. });

    assert_eq!(
        count_errs!(errors, SemanticError::ApiInvalidParam { .. }),
        4
    );
}

#[test]
fn api_sets_media_types() {
    // Arrange
    let src = &with_env(
        r#"
        [use my_d1]
        model User {
            primary {
                id: int
            }
            name: string
        }

        api User {
            post streamInputOutput(self, e: env, s: stream) -> stream
            get jsonInputOutput(j: json) -> json
        }
    "#,
    );
    let parse = lex_and_parse(src);

    // Act
    let (result, errors) = SemanticAnalysis::analyze(&parse);

    // Assert
    assert_eq!(errors.len(), 0, "unexpected errors: {:#?}", errors);
    let user_apis = &result.models.get("User").unwrap().apis;
    let stream_method = user_apis
        .iter()
        .find(|m| m.name == "streamInputOutput")
        .unwrap();
    assert!(matches!(stream_method.return_media, MediaType::Octet));
    assert!(matches!(stream_method.parameters_media, MediaType::Octet));

    let json_method = user_apis
        .iter()
        .find(|m| m.name == "jsonInputOutput")
        .unwrap();
    assert!(matches!(json_method.return_media, MediaType::Json));
    assert!(matches!(json_method.parameters_media, MediaType::Json));
}

#[test]
fn data_source_errors() {
    // Arrange
    let src = &with_env(
        r#"
        [use my_d1]
        model User {
            primary {
                id: int
            }
            name: string

            kv(my_kv, "users/{id}") {
                cached: json
            }

            r2(my_r2, "avatars/{id}") {
                avatar
            }

            nav(Post::authorId) {
                posts
            }
        }

        [use my_d1]
        model Post {
            primary {
                id: int
            }
            title: string

            foreign(User::id) {
                authorId
            }
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

        // SQL references $ghost which is not a declared param
        source UnknownSqlParam for User {
            include {}
            sql get(id: int) { "($include) WHERE id = $ghost" }
        }

    "#,
    );
    let parse = lex_and_parse(src);

    // Act
    let (_, errors) = SemanticAnalysis::analyze(&parse);

    // Assert
    // BadModelSource: unknown model
    expect_err!(
        errors,
        SemanticError::DataSourceUnknownModelReference { .. }
    );

    // BadTreeSource: "nonexistent" is not a field on User
    assert!(errors.iter().any(|e| matches!(
        e,
        SemanticError::DataSourceInvalidIncludeTreeReference { name, .. }
            if name == "nonexistent"
    )));

    // BadNestedTreeSource: "bogus" is not a field on Post
    assert!(errors.iter().any(|e| matches!(
        e,
        SemanticError::DataSourceInvalidIncludeTreeReference { name, .. }
            if name == "bogus"
    )));

    // BadParamSource: User is not a valid sql type
    expect_err!(errors, SemanticError::DataSourceInvalidMethodParam { .. });

    // UnknownSqlParam: $ghost is not a declared param
    assert!(errors.iter().any(|e| matches!(
        e,
        SemanticError::DataSourceUnknownSqlParam { name, .. } if name == "ghost"
    )));
}

#[test]
fn data_source_include_tree_kv_r2() {
    // Arrange
    let src = &with_env(
        r#"
        [use my_d1]
        model User {
            primary {
                id: int
            }
            name: string

            kv(my_kv, "users/{id}") {
                cached: json
            }

            r2(my_r2, "avatars/{id}") {
                avatar
            }
        }

        source WithKvR2 for User {
            include { cached, avatar }
        }
    "#,
    );
    let parse = lex_and_parse(src);

    // Act
    let (result, errors) = SemanticAnalysis::analyze(&parse);

    // Assert
    assert_eq!(errors.len(), 0, "unexpected errors: {:#?}", errors);

    let user = result.models.get("User").unwrap();
    assert_eq!(user.data_sources.len(), 2); // including the implicit default source
    assert!(user.data_sources.contains_key("WithKvR2"));
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
    let (result, errors) = SemanticAnalysis::analyze(&parse);

    // Assert
    assert_eq!(errors.len(), 3);

    let cycle = expect_err!(errors,
        SemanticError::CyclicalRelationship { cycle } => cycle.clone()
    );
    assert_eq!(cycle, vec!["MyPoo"]);

    assert!(errors.iter().any(|e| matches!(
        e,
        SemanticError::PlainOldObjectInvalidFieldType { field } if field.name == "streamField"
    )));

    assert!(errors.iter().any(|e| matches!(
        e,
        SemanticError::PlainOldObjectInvalidFieldType { field } if field.name == "voidField"
    )));
}

#[test]
fn service_collects_api_blocks() {
    // Arrange
    let src = r#"
        inject { YouTubeApi }
        service MyService {
            tube: YouTubeApi
            foo: string
        }
        api MyService {
            post firstMethod(e: env) -> string
        }

        api MyService {
            get secondMethod() -> string
        }
    "#;

    // Act
    let parse = lex_and_parse(src);
    let (result, errors) = SemanticAnalysis::analyze(&parse);

    // Assert
    assert_eq!(errors.len(), 0, "unexpected errors: {:#?}", errors);
    let service = result.services.get("MyService").unwrap();
    assert_eq!(service.apis.len(), 2);
}

#[test]
fn poo_with_model_reference() {
    let src = r#"
        env {
            d1 { db }
        }

        [use db]
        model BasicModel {
            primary {
                id: int
            }
        }

        poo PooWithComposition {
            field1: string
            field2: BasicModel
        }
    "#;

    let parse = lex_and_parse(src);
    let (result, errors) = SemanticAnalysis::analyze(&parse);

    assert_eq!(errors.len(), 0, "unexpected errors: {:#?}", errors);
    let poo = result.poos.get("PooWithComposition").unwrap();
    assert_eq!(poo.fields.len(), 2);
}

#[test]
fn cidl_types_resolve() {
    // Arrange
    let src = r#"
        env {
            d1 { my_d1 }
        }

        [use my_d1]
        model User {
            primary {
                id: int
            }
        }

        poo MyPoo {
            field: string
        }

        service MyService {}

        api User {
            post resolveAll(e: env, p: Array<MyPoo>, u: User, s: MyService) -> string
        }
    "#;

    // Act
    let parse = lex_and_parse(src);
    let (result, errors) = SemanticAnalysis::analyze(&parse);

    // Assert
    assert_eq!(errors.len(), 0, "unexpected errors: {:#?}", errors);

    let api = result.models.get("User").unwrap().apis.first().unwrap();
    let param_types: Vec<_> = api.parameters.iter().map(|p| p.cidl_type.clone()).collect();
    assert_eq!(
        param_types,
        vec![
            CidlType::Env,
            CidlType::Array(Box::new(CidlType::Object { name: "MyPoo" })),
            CidlType::Object { name: "User" },
            CidlType::Inject { name: "MyService" },
        ]
    );
}
