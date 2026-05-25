#![allow(unused_variables)]

use compiler_test::lex_and_ast;
use idl::{CidlType, MediaType, NavigationFieldKind, Number, Validator};
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
        d1 {{
            my_d1
        }}

        kv my_kv {{
            cached(id: int) -> json {{
                "users/{{id}}"
            }}

            items(field: string) -> json {{
                "items/{{field}}"
            }}
        }}

        r2 my_r2 {{
            avatar(id: int) {{
                "avatars/{{id}}"
            }}

            obj(field: string) {{
                "assets/{{field}}"
            }}
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
        model User for db {
            primary { id: int }
        }
    "#;
    let parse = lex_and_ast(src);

    // Act
    let (_, errors) = SemanticAnalysis::analyze(&parse);

    // Assert
    expect_err!(errors, SemanticError::MissingWranglerEnvBlock);
}

#[test]
fn wrangler_duplicate_symbol() {
    // Arrange
    let src = r#"
        d1 {
            my_d1
        }
        d1 {
            my_d1 // duplicate symbol
        }
    "#;
    let parse = lex_and_ast(src);

    // Act
    let (result, errors) = SemanticAnalysis::analyze(&parse);

    // Assert
    assert_eq!(errors.len(), 1);
    let second = expect_err!(errors,
        SemanticError::DuplicateSymbol { second, .. } => second
    );
    assert_eq!(second.name, "my_d1");
}

#[test]
fn d1_model_basic_errors() {
    // Arrange
    let src = with_env(
        r#"
        model User for my_d1 {
            // missing primary key
        }

        model Post for other_d1 {} // unresolved, not in spec

        // missing binding
        model Comment {
            primary { id: int }
        }
    "#,
    );
    let parse = lex_and_ast(&src);

    // Act
    let (result, errors) = SemanticAnalysis::analyze(&parse);

    // Assert
    let model = expect_err!(errors,
        SemanticError::D1ModelMissingPrimaryKey { model } => model
    );
    assert_eq!(model.name, "User");

    expect_err!(errors, SemanticError::D1ModelInvalidD1Binding { .. });

    let model = expect_err!(errors,
        SemanticError::D1ModelMissingD1Binding { model } => model
    );
    assert_eq!(model.name, "Comment");
}

#[test]
fn d1_model_column_fk_errors() {
    // Arrange
    let src = r#"
        d1 {
            my_d1
            other_d1
        }

        model User for my_d1 {
            primary {
                id: option<int> // primary key cannot be nullable
            }

            column {
                id: int // duplicate symbol
            }

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

        model Post for my_d1 {
            primary {
                id: int
            }
        }

        model OtherD1Model for other_d1 {
            primary {
                id: int
            }
        }
    "#;
    let parse = lex_and_ast(src);

    // Act
    let (result, errors) = SemanticAnalysis::analyze(&parse);

    // Assert
    assert_eq!(errors.len(), 9);

    let column = expect_err!(errors,
        SemanticError::NullablePrimaryKey { column } => column
    );
    assert_eq!(column.name, "id");

    let second = expect_err!(errors,
        SemanticError::DuplicateSymbol { second, .. } => second
    );
    assert_eq!(second.name, "id");

    let model = expect_err!(errors,
        SemanticError::ForeignKeyReferencesSelf { model, .. } => model
    );
    assert_eq!(model.name, "User");

    let fk_model = expect_err!(errors,
        SemanticError::ForeignKeyReferencesDifferentDatabase { fk_model, .. } => fk_model.name
    );
    assert_eq!(fk_model, "OtherD1Model");

    let inconsistent_model_adj = expect_err!(
        errors,
        SemanticError::InconsistentModelAdjacency {
            first_model,
            second_model,
            ..
        } => (first_model.name, second_model.name)
    );
    assert_eq!(inconsistent_model_adj.0, "Post");
    assert_eq!(inconsistent_model_adj.1, "User");

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
            SemanticError::UnresolvedSymbol { symbol, .. }
                if symbol.name == "DoesNotExist" || symbol.name == "nonexistent" =>
            {
                Some(symbol.name)
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
        d1 {
            my_d1
            other_d1
        }

        model User for my_d1 {
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

        model Post for my_d1 {
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

        model DifferentDatabaseModel for other_d1 {
            primary {
                id: int
            }
        }
    "#;
    let parse = lex_and_ast(src);

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
        } => (first_model.name, second_model.name)
    );
    assert_eq!(inconsistent_model_adj.0, "Post");
    assert_eq!(inconsistent_model_adj.1, "User");

    let nav_name = expect_err!(errors,
        SemanticError::NavigationReferencesDifferentDatabase { field, .. } => field.name
    );
    assert_eq!(nav_name, "invalidAdjModel");

    let ambiguous_m2ms = count_errs!(errors, SemanticError::NavigationAmbiguousM2M { .. });
    assert_eq!(ambiguous_m2ms, 3);
}

#[test]
fn d1_model_nav_one_to_one() {
    // Arrange
    let src = &with_env(
        r#"
        model Person for my_d1 {
            primary {
                id: int
            }

            foreign (Horse::id) {
                horseId
                nav { horse }
            }
        }

        model Horse for my_d1 {
            primary {
                id: int
            }
        }
        "#,
    );
    let parse = lex_and_ast(src);

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
            .any(|c| c.field.name == "horseId" && matches!(c.field.cidl_type, CidlType::Int))
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
        model Author for my_d1 {
            primary { id: int }

            nav (Post::authorId) {
                posts
            }
        }

        model Post for my_d1 {
            primary { id: int }

            foreign (Author::id) {
                authorId
            }
        }
        "#,
    );

    // Act
    let parse = lex_and_ast(src);
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
        model Student for my_d1 {
            primary { id: int }

            nav (Course::id) {
                courses
            }
        }

        model Course for my_d1 {
            primary { id: int }

            nav (Student::id) {
                students
            }
        }
        "#,
    );

    // Act
    let parse = lex_and_ast(src);
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
        model A for my_d1 {
            primary { id: int }

            foreign (B::id) {
                bId2
                nav { toB }
            }
        }

        model B for my_d1 {
            primary { id: int }

            foreign (C::id) {
                cId
                nav { toC }
            }
        }

        model C for my_d1 {
            primary { id: int }

            foreign (A::id) {
                aId
                nav { toA }
            }
        }
        "#,
    );

    // Act
    let parse = lex_and_ast(src);
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
        model A for my_d1 {
            primary { id: int }

            foreign (B::id) optional {
                bId
                nav { toB }
            }
        }

        model B for my_d1 {
            primary { id: int }

            foreign (C::id) optional {
                cId
                nav { toC }
            }
        }

        model C for my_d1 {
            primary { id: int }

            foreign (A::id) optional {
                aId
                nav { toA }
            }
        }
        "#,
    );

    // Act
    let parse = lex_and_ast(src);
    let (_, errors) = SemanticAnalysis::analyze(&parse);

    // Assert
    assert_eq!(errors.len(), 0);
}

#[test]
fn kv_r2_errors() {
    // Arrange
    let src = &with_env(
        r#"
        model Foo for my_d1 {
            primary { field: string }

            // invalid binding type (my_d1 is a D1, not KV)
            kv my_d1::items(field) { foo }

            // invalid binding type (my_kv is a KV, not R2)
            r2 my_kv::items(field) { obj }
        }
        "#,
    );
    let parse = lex_and_ast(src);

    // Act
    let (result, errors) = SemanticAnalysis::analyze(&parse);

    // Assert
    let binding = expect_err!(errors,
        SemanticError::KvInvalidBinding { binding, ..} => binding.name
    );
    assert_eq!(binding, "my_d1");

    let binding = expect_err!(errors,
        SemanticError::R2InvalidBinding { binding, .. } => binding.name
    );
    assert_eq!(binding, "my_kv");
}

#[test]
fn binding_key_format_unknown_param() {
    // A `{var}` in a binding field's key format must correspond to a declared
    // param on that field. Otherwise we should get KvR2UnknownKeyVariable.
    let src = r#"
        kv UserMeta {
            meta(id: int) -> json {
                "metadata/{id}/{bogus}"
            }
        }

        r2 UserAvatars {
            avatar(id: int) {
                "avatars/{ghost}.jpg"
            }
        }
    "#;
    let parse = lex_and_ast(src);
    let (_, errors) = SemanticAnalysis::analyze(&parse);

    let unknowns: Vec<&str> = errors
        .iter()
        .filter_map(|e| match e {
            SemanticError::KvR2UnknownKeyVariable { variable, .. } => Some(*variable),
            _ => None,
        })
        .collect();
    assert!(
        unknowns.contains(&"bogus"),
        "expected 'bogus' to be flagged, got: {:?}",
        unknowns
    );
    assert!(
        unknowns.contains(&"ghost"),
        "expected 'ghost' to be flagged, got: {:?}",
        unknowns
    );
}

#[test]
fn binding_key_format_invalid_syntax() {
    // Malformed key format (unclosed/nested brace) should produce KvR2InvalidKeyFormat.
    let src = r#"
        kv NsA {
            entry(id: int) -> json {
                "entry/{id"
            }
        }

        r2 NsB {
            obj(id: int) {
                "obj/{{id}"
            }
        }
    "#;
    let parse = lex_and_ast(src);
    let (_, errors) = SemanticAnalysis::analyze(&parse);

    assert_eq!(
        count_errs!(errors, SemanticError::KvR2InvalidKeyFormat { .. }),
        2,
        "expected two invalid-key-format errors, got: {:#?}",
        errors
    );
}

#[test]
fn kv_and_d1_coexist() {
    // A model can have both D1 and KV/R2 properties
    let src = &with_env(
        r#"
        model User for my_d1 {
            primary {
                id: int
            }
            column {
                name: string
            }

            kv my_kv::cached(id) {
                cached
            }
        }
        "#,
    );
    let parse = lex_and_ast(src);

    // Act
    let (result, errors) = SemanticAnalysis::analyze(&parse);

    // Assert
    assert_eq!(errors.len(), 0, "unexpected errors: {:#?}", errors);
    let user = result.models.get("User").unwrap();
    assert!(user.backing_binding.is_some());
    assert_eq!(user.kv_fields.len(), 1);
    assert_eq!(user.columns.len(), 1);
    assert_eq!(user.kv_fields[0].binding, "my_kv");
    assert_eq!(user.kv_fields[0].binding_field, "cached");
    assert_eq!(user.kv_fields[0].args, vec!["id"]);

    // The binding field's params live in the wrangler env, not on the model.
    let env = result.wrangler_env.as_ref().unwrap();
    let kv = env.kv_bindings.iter().find(|b| b.name == "my_kv").unwrap();
    let cached = kv.fields.iter().find(|f| f.name == "cached").unwrap();
    assert_eq!(cached.params[0].name, "id");
    assert_eq!(cached.params[0].cidl_type, CidlType::Int);
}

#[test]
fn api_errors() {
    // Arrange
    let src = &with_env(
        r#"
        model User for my_d1 {
            primary {
                id: int
            }
            column {
                name: string
            }
        }

        // Unknown model reference
        api NonExistentModel {}

        // Invalid return type
        api User {
            get badReturn() -> option<stream>
        }

        // Object parameter on GET
        api User {
            get badGetObj(u: User) -> string
        }

        // R2Object parameter on GET
        api User {
            get badGetR2(r: r2object) -> string
        }

        // Stream param with extra non-inject params (invalid)
        api User {
            post badStream(s: stream, extra: string) -> stream
        }
    "#,
    );
    let parse = lex_and_ast(src);

    // Act
    let (_, errors) = SemanticAnalysis::analyze(&parse);

    // Assert
    assert_eq!(errors.len(), 5);

    expect_err!(errors, SemanticError::ApiUnknownNamespaceReference { .. });

    expect_err!(errors, SemanticError::ApiInvalidReturn { .. });

    assert_eq!(
        count_errs!(errors, SemanticError::ApiInvalidParam { .. }),
        3
    );
}

#[test]
fn api_sets_media_types() {
    // Arrange
    let src = &with_env(
        r#"
        model User for my_d1 {
            primary {
                id: int
            }
            column {
                name: string
            }
        }

        api User {
            [inject my_d1]
            post streamInputOutput(self, s: stream) -> stream
            get jsonInputOutput(j: json) -> json
        }
    "#,
    );
    let parse = lex_and_ast(src);

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
        model User for my_d1 {
            primary {
                id: int
            }
            column {
                name: string
            }

            kv my_kv::cached(id) {
                cached
            }

            r2 my_r2::avatar(id) {
                avatar
            }

            nav(Post::authorId) {
                posts
            }
        }

        model Post for my_d1 {
            primary {
                id: int
            }
            column {
                title: string
            }

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
    let parse = lex_and_ast(src);

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
        SemanticError::DataSourceInvalidIncludeTreeReference { field, .. }
            if field.name == "nonexistent"
    )));

    // BadNestedTreeSource: "bogus" is not a field on Post
    assert!(errors.iter().any(|e| matches!(
        e,
        SemanticError::DataSourceInvalidIncludeTreeReference { field, .. }
            if field.name == "bogus"
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
        model User for my_d1 {
            primary {
                id: int
            }
            column {
                name: string
            }

            kv my_kv::cached(id) {
                cached
            }

            r2 my_r2::avatar(id) {
                avatar
            }
        }

        source WithKvR2 for User {
            include {
                cached
                avatar
            }
        }
    "#,
    );
    let parse = lex_and_ast(src);

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
        }
    "#;

    // Act
    let parse = lex_and_ast(src);
    let (_result, errors) = SemanticAnalysis::analyze(&parse);

    // Assert
    assert_eq!(errors.len(), 1);
    assert!(errors.iter().any(|e| matches!(
        e,
        SemanticError::PlainOldObjectInvalidFieldType { field } if field.name == "streamField"
    )));
}

#[test]
fn poo_with_model_reference() {
    let src = r#"
        d1 { db }

        model BasicModel for db {
            primary {
                id: int
            }
        }

        poo PooWithComposition {
            field1: string
            field2: BasicModel
        }
    "#;

    let parse = lex_and_ast(src);
    let (result, errors) = SemanticAnalysis::analyze(&parse);

    assert_eq!(errors.len(), 0, "unexpected errors: {:#?}", errors);
    let poo = result.poos.get("PooWithComposition").unwrap();
    assert_eq!(poo.fields.len(), 2);
}

#[test]
fn cidl_types_resolve() {
    // Arrange
    let src = r#"
        d1 { my_d1 }

        model User for my_d1 {
            primary {
                id: int
            }
        }

        poo MyPoo {
            field: string
        }

        api User {
            [inject my_d1]
            post resolveAll(p: array<MyPoo>, u: User) -> string
        }
    "#;

    // Act
    let parse = lex_and_ast(src);
    let (result, errors) = SemanticAnalysis::analyze(&parse);

    // Assert
    assert_eq!(errors.len(), 0, "unexpected errors: {:#?}", errors);

    let api = result.models.get("User").unwrap().apis.first().unwrap();
    let param_types: Vec<_> = api.parameters.iter().map(|p| p.cidl_type.clone()).collect();
    assert_eq!(
        param_types,
        vec![
            CidlType::Array(Box::new(CidlType::Object { name: "MyPoo" })),
            CidlType::Object { name: "User" },
        ]
    );
    assert_eq!(api.injected, vec!["my_d1"]);
}

#[test]
fn fk_inherits_validators() {
    // Arrange
    let src = with_env(
        r#"
        model User for my_d1 {
            primary {
                [gt 0]
                id: int
            }

            column {
                [lt 100]
                age: int
            }
        }

        model Post for my_d1 {
            primary { id: int }

            foreign (User::id) {
                userId
            }

            foreign (User::age) {
                userAge
            }
        }
        "#,
    );
    let parse = lex_and_ast(&src);
    let (result, errors) = SemanticAnalysis::analyze(&parse);

    // Assert
    assert_eq!(errors.len(), 0, "unexpected errors: {:#?}", errors);

    let post = result.models.get("Post").unwrap();

    let user_id_col = post
        .columns
        .iter()
        .find(|c| c.field.name == "userId")
        .unwrap();
    assert_eq!(user_id_col.field.validators.len(), 1);
    assert!(matches!(
        user_id_col.field.validators[0],
        Validator::GreaterThan(Number::Int(0))
    ));

    let user_age_col = post
        .columns
        .iter()
        .find(|c| c.field.name == "userAge")
        .unwrap();
    assert_eq!(user_age_col.field.validators.len(), 1);
    assert!(matches!(
        user_age_col.field.validators[0],
        Validator::LessThan(Number::Int(100))
    ));
}

#[test]
fn validator_errors() {
    // Arrange
    let src = with_env(
        r#"
        model User for my_d1 {
            primary { id: int }

            column {
                [len 3.14]                // ValidatorInvalidArgument (wrong literal kind)
                name: string

                [step 2.5]                // ValidatorInvalidArgument (float to step)
                [len 3]                   // ValidatorInvalidForType (length on non-string)
                [regex "not_a_regex"]     // ValidatorInvalidForType (regex on non-string)
                age: int
            }
        }
        "#,
    );
    let parse = lex_and_ast(&src);
    let (_, errors) = SemanticAnalysis::analyze(&parse);

    // Assert
    assert_eq!(
        count_errs!(errors, SemanticError::ValidatorInvalidArgument { .. }),
        2
    );
    assert_eq!(
        count_errs!(errors, SemanticError::ValidatorInvalidForType { .. }),
        2
    );
}

#[test]
fn validator_valid() {
    // Arrange
    let src = with_env(
        r#"
        model User for my_d1 {
            primary { id: int }

            column {
                [gt 0]
                [gte 0]
                [lt 200]
                [lte 150]
                [step 5]
                age: int

                [minlen 1]
                [maxlen 64]
                [len 10]
                [regex /^[a-z]+$/]
                name: string
            }
        }

        poo MyPoo {
            [gt 0]
            [lte 100]
            score: int
        }
        "#,
    );
    let parse = lex_and_ast(&src);
    let (result, errors) = SemanticAnalysis::analyze(&parse);

    // Assert
    assert_eq!(errors.len(), 0, "unexpected errors: {:#?}", errors);

    let user = result.models.get("User").unwrap();
    let age_col = user.columns.iter().find(|c| c.field.name == "age").unwrap();
    let name_col = user
        .columns
        .iter()
        .find(|c| c.field.name == "name")
        .unwrap();
    assert_eq!(age_col.field.validators.len(), 5);
    assert_eq!(name_col.field.validators.len(), 4);

    let poo = result.poos.get("MyPoo").unwrap();
    let score = poo.fields.iter().find(|f| f.name == "score").unwrap();
    assert_eq!(score.validators.len(), 2);
}

#[test]
fn inject_tag_populates_api_method_injected() {
    let src = r#"
        d1 { db }

        kv cache {
            entry(id: int) -> json {
                "entry/{id}"
            }
        }

        vars { API_KEY: string }

        inject { YouTubeApi }

        model M for db {
            primary { id: int }
        }

        api M {
            [inject db, cache, API_KEY, YouTubeApi]
            get all(self) -> string
        }
    "#;

    let parse = lex_and_ast(src);
    let (result, errors) = SemanticAnalysis::analyze(&parse);
    assert_eq!(errors.len(), 0, "unexpected errors: {:#?}", errors);

    let api = result
        .models
        .get("M")
        .unwrap()
        .apis
        .iter()
        .find(|a| a.name == "all")
        .unwrap();
    assert_eq!(api.injected, vec!["db", "cache", "API_KEY", "YouTubeApi"]);
    // Explicit params remain empty (only `self`).
    assert!(api.parameters.is_empty());
}

#[test]
fn inject_tag_dedupes_duplicates() {
    let src = r#"
        d1 { db }

        model M for db {
            primary { id: int }
        }

        api M {
            [inject db, db]
            get dup(self) -> string
        }
    "#;

    let parse = lex_and_ast(src);
    let (result, errors) = SemanticAnalysis::analyze(&parse);
    assert_eq!(errors.len(), 0, "unexpected errors: {:#?}", errors);

    let api = result
        .models
        .get("M")
        .unwrap()
        .apis
        .iter()
        .find(|a| a.name == "dup")
        .unwrap();
    assert_eq!(api.injected, vec!["db"]);
}

#[test]
fn dataless_model_errors() {
    let src = r#"
        d1 { db }

        model Foo {}
        model Bar {}

        api Foo {
            get instanceLike(self) -> string
        }

        api Bar {
            post takesFoo(foo: Foo) -> string
            get yieldsFoo() -> Foo
            post takesPartialFoo(foo: partial<Foo>) -> string
        }

        poo Container {
            inner: Foo
        }

        source FooSource for Foo {
            include {}
        }
    "#;

    let parse = lex_and_ast(src);
    let (_result, errors) = SemanticAnalysis::analyze(&parse);

    let method = expect_err!(errors,
        SemanticError::ModelInstanceMethodWithNoData { method } => method
    );
    assert_eq!(method.name, "instanceLike");

    let data_source = expect_err!(errors,
        SemanticError::ModelDataSourceWithNoData { data_source, .. } => data_source
    );
    assert_eq!(data_source.name, "FooSource");

    let used_as_type_count = errors
        .iter()
        .filter(|e| {
            matches!(
                e,
                SemanticError::ModelWithNoDataUsedAsType { model_name, .. } if *model_name == "Foo"
            )
        })
        .count();
    assert_eq!(used_as_type_count, 4);
}
