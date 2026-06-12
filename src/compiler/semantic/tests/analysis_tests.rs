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
        SemanticError::ModelMissingPrimaryKey { model } => model
    );
    assert_eq!(model.name, "User");

    expect_err!(errors, SemanticError::ModelInvalidBinding { .. });

    let model = expect_err!(errors,
        SemanticError::ModelMissingDatabaseBinding { model } => model
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
    assert_eq!(errors.len(), 10);

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
                tenant: int
            }

            column {
                horseId: int
            }

            nav (Post::id, User::id) {
                inconsistentModelAdjacency
            }

            nav (DifferentDatabaseModel::id) {
                invalidAdjModel
            }

            // 1:1 nav whose local key is not a foreign key to Post
            nav Post::id(horseId) {
                missingOneToOneFk
            }

            // 1:M nav with no foreign key on Post referencing User
            nav Post::id {
                missingOneToManyFk
            }

            // Mixes a 1:1 entry (with local key) and a 1:M entry (without)
            nav (Post::id(horseId), Post::tenant) {
                mixedAdjacency
            }
        }

        model Post for my_d1 {
            primary {
                id: int
                tenant: int
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
        SemanticError::NavigationReferencesDifferentBacking { field, .. } => field.name
    );
    assert_eq!(nav_name, "invalidAdjModel");

    let missing: Vec<&str> = errors
        .iter()
        .filter_map(|e| match e {
            SemanticError::NavigationMissingForeignKey { field, .. } => Some(field.name),
            _ => None,
        })
        .collect();
    assert!(
        missing.contains(&"missingOneToOneFk"),
        "expected 1:1 nav error: {errors:#?}"
    );
    assert!(
        missing.contains(&"missingOneToManyFk"),
        "expected 1:M nav error: {errors:#?}"
    );

    let mixed = expect_err!(errors,
        SemanticError::NavigationMixedAdjacency { field } => field.name
    );
    assert_eq!(mixed, "mixedAdjacency");
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
            }

            nav Horse::id(horseId) {
                horse
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
            && matches!(&nav.kind, NavigationFieldKind::OneToOne { fields } if fields.len() == 1 && fields[0] == "horseId")
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

            nav Post::authorId {
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
fn route_model_valid() {
    // Arrange: route fields, a KV field keyed on a route field, a composite 1:1 nav
    // declared out of the target's route order, and a `get` keyed on route fields.
    let src = &with_env(
        r#"
        [crud get, save]
        model Person {
            route {
                id: int
                org: string
            }

            kv my_kv::cached(id) { cached }

            nav (Dog::tenant(org), Dog::ownerId(id)) { dog }
        }

        model Dog {
            route {
                ownerId: int
                tenant: string
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
    assert!(person.backing.is_none());
    let route_fields: Vec<&str> = person
        .route_fields
        .iter()
        .map(|f| f.name.as_ref())
        .collect();
    assert_eq!(route_fields, vec!["id", "org"]);

    // 1:1 nav columns are ordered to match Dog's route fields (ownerId, tenant).
    let dog_nav = person.navigation_fields.first().unwrap();
    assert_eq!(dog_nav.field.name, "dog");
    assert_eq!(dog_nav.model_reference, "Dog");
    assert_eq!(dog_nav.field.cidl_type, CidlType::Object { name: "Dog" });
    let NavigationFieldKind::OneToOne { fields } = &dog_nav.kind else {
        unreachable!()
    };
    assert_eq!(fields, &vec!["id", "org"]);

    // The default data source has no SQL and is keyed on the route fields.
    let ds = person.default_data_source().unwrap();
    assert!(ds.include_query.is_empty());
    let params: Vec<&str> = ds
        .get
        .parameters
        .iter()
        .map(|p| p.parameter.name.as_ref())
        .collect();
    assert_eq!(params, vec!["id", "org"]);
    assert!(ds.get.parameters.iter().all(|p| p.instance_field));
}

#[test]
fn d1_model_navigates_to_worker_model() {
    // Arrange
    let src = &with_env(
        r#"
        model User for my_d1 {
            primary {
                id: int
            }

            column {
                org: string
            }

            nav (Person::id(id), Person::tenant(org)) { person }
        }

        model Person {
            route {
                tenant: string
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

    let user = result.models.get("User").unwrap();
    assert!(user.is_d1_backed());
    assert_eq!(user.backing.as_ref().unwrap().binding, "my_d1");

    let person_nav = user.navigation_fields.first().unwrap();
    assert_eq!(person_nav.field.name, "person");
    assert_eq!(person_nav.model_reference, "Person");
    assert_eq!(
        person_nav.field.cidl_type,
        CidlType::Object { name: "Person" }
    );

    let NavigationFieldKind::OneToOne { fields } = &person_nav.kind else {
        unreachable!()
    };
    assert_eq!(fields, &vec!["org", "id"]);
}

#[test]
fn d1_model_route_navigation_errors() {
    // Arrange
    let src = &with_env(
        r#"
        model User for my_d1 {
            primary {
                id: int
            }

            nav Person::id { people }      // 1:M not allowed to a route model
            nav Partial::a(id) { partial } // does not supply all of Partial's route fields
        }

        model Person {
            route { id: int }
        }

        model Partial {
            route {
                a: int
                b: int
            }
        }
        "#,
    );
    let parse = lex_and_ast(src);

    // Act
    let (_result, errors) = SemanticAnalysis::analyze(&parse);

    // Assert
    assert_eq!(
        count_errs!(errors, SemanticError::RouteNavigationInvalid { .. }),
        2
    );
}

#[test]
fn worker_model_errors() {
    // Arrange
    let src = &with_env(
        r#"
        [crud get, list]
        model Worker {
            route { id: int }

            nav Stored::id(id) { stored }  // references a D1 model
            nav Other::id { others }       // 1:M not allowed
            nav Partial::a(id) { partial } // does not supply all of Partial's route fields
        }

        model HasBinding for my_d1 {
            route { id: int }              // route block with a `for` binding
        }

        model HasColumn {
            route { id: int }
            column { name: string }        // route block with a SQL block
        }

        model Stored for my_d1 {
            primary { id: int }
        }

        model Other {
            route { id: int }
        }

        model Partial {
            route {
                a: int
                b: int
            }
        }
        "#,
    );
    let parse = lex_and_ast(src);

    // Act
    let (result, errors) = SemanticAnalysis::analyze(&parse);

    // Assert
    expect_err!(
        errors,
        SemanticError::NavigationReferencesDifferentBacking { .. }
    );
    assert_eq!(
        count_errs!(errors, SemanticError::RouteNavigationInvalid { .. }),
        2 // 1:M nav, and the nav missing a target route field
    );
    assert_eq!(
        count_errs!(errors, SemanticError::ModelMixesRoutesAndSql { .. }),
        1
    );
    let crud = expect_err!(errors,
        SemanticError::UnsupportedCrudOperation { crud, .. } => crud
    );
    assert!(matches!(crud.inner, idl::CrudKind::List)); // list needs SQL
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
            }

            nav B::id(bId2) { toB }
        }

        model B for my_d1 {
            primary { id: int }

            foreign (C::id) {
                cId
            }

            nav C::id(cId) { toC }
        }

        model C for my_d1 {
            primary { id: int }

            foreign (A::id) {
                aId
            }

            nav A::id(aId) { toA }
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
            }

            nav B::id(bId) { toB }
        }

        model B for my_d1 {
            primary { id: int }

            foreign (C::id) optional {
                cId
            }

            nav C::id(cId) { toC }
        }

        model C for my_d1 {
            primary { id: int }

            foreign (A::id) optional {
                aId
            }

            nav A::id(aId) { toA }
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
    assert_eq!(
        count_errs!(errors, SemanticError::UnresolvedSymbol { .. }),
        2,
        "expected two unresolved symbol errors, got: {:#?}",
        errors
    );
}

#[test]
fn binding_key_format_unknown_param() {
    // Arrange
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

    // Act
    let parse = lex_and_ast(src);
    let (_, errors) = SemanticAnalysis::analyze(&parse);

    // Assert
    let unknowns = errors
        .iter()
        .filter_map(|e| match e {
            SemanticError::TemplateUnknownVariable { variable, .. } => Some(*variable),
            _ => None,
        })
        .collect::<Vec<_>>();
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
    // Arrange
    let src = r#"
        kv NsA {
            entry(id: int) -> json {
                "entry/{id" // missing closing brace
            }
        }

        r2 NsB {
            obj(id: int) {
                "obj/{{id}" // nested braces not allowed
            }
        }
    "#;

    // Act
    let parse = lex_and_ast(src);
    let (_, errors) = SemanticAnalysis::analyze(&parse);

    // Assert
    assert_eq!(
        count_errs!(errors, SemanticError::TemplateInvalidFormat { .. }),
        2,
        "expected two invalid-key-format errors, got: {:#?}",
        errors
    );
}

#[test]
fn kv_r2_templates_inherit_validators_update_key_format() {
    // Arrange
    let src = r#"
        d1 {
            my_d1
        }

        kv MyKv {
            [gt 10]
            value([step 5] foo: int) -> int {
                "users/{foo}"
            }
        }

        r2 MyR2 {
            obj([len 5] bar: string, [gt 0] baz: int) {
                "objects/{bar}/{baz}"
            }
        }

        model User for my_d1 {
            primary {
                userId: int
            }
            column {
                name: string
            }

            kv MyKv::value(userId) {
                cached
            }

            r2 MyR2::obj(name, userId) {
                avatar
            }
        }
        "#;

    let parse = lex_and_ast(src);

    // Act
    let (result, errors) = SemanticAnalysis::analyze(&parse);

    // Assert
    assert_eq!(errors.len(), 0, "unexpected errors: {:#?}", errors);
    let user = result.models.get("User").unwrap();

    assert_eq!(user.kv_fields.len(), 1);
    assert_eq!(user.kv_fields[0].binding, "MyKv");
    let kv_binding = &user.kv_fields[0];
    assert_eq!(kv_binding.field.name, "cached");
    assert_eq!(
        kv_binding.field.cidl_type,
        CidlType::KvObject(Box::new(CidlType::Int))
    );
    assert_eq!(kv_binding.field.validators.len(), 1);
    assert!(matches!(
        kv_binding.field.validators[0],
        Validator::GreaterThan(Number::Int(10))
    ));
    assert_eq!(kv_binding.key_format, "users/{userId}");

    assert_eq!(user.r2_fields.len(), 1);
    assert_eq!(user.r2_fields[0].binding, "MyR2");
    let r2_binding = &user.r2_fields[0];
    assert_eq!(r2_binding.field.name, "avatar");
    assert_eq!(r2_binding.field.cidl_type, CidlType::R2Object);
    assert_eq!(r2_binding.key_format, "objects/{name}/{userId}");

    let id_col = user
        .primary_columns
        .iter()
        .find(|c| c.field.name == "userId")
        .unwrap();
    assert_eq!(id_col.field.validators.len(), 2);
    assert!(matches!(id_col.field.validators[0], Validator::Step(5)));
    assert!(matches!(
        id_col.field.validators[1],
        Validator::GreaterThan(Number::Int(0))
    ));

    let name_col = user
        .columns
        .iter()
        .find(|c| c.field.name == "name")
        .unwrap();
    assert_eq!(name_col.field.validators.len(), 1);
    assert!(matches!(name_col.field.validators[0], Validator::Length(5)));
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
            get(u: User)
        }
    "#,
    );
    let parse = lex_and_ast(src);

    // Act
    let (_, errors) = SemanticAnalysis::analyze(&parse);

    // Assert
    expect_err!(
        errors,
        SemanticError::DataSourceUnknownModelReference { .. }
    );

    assert!(errors.iter().any(|e| matches!(
        e,
        SemanticError::DataSourceInvalidIncludeTreeReference { field, .. }
            if field.name == "nonexistent"
    )));

    assert!(errors.iter().any(|e| matches!(
        e,
        SemanticError::DataSourceInvalidIncludeTreeReference { field, .. }
            if field.name == "bogus"
    )));

    expect_err!(errors, SemanticError::DataSourceInvalidMethodParam { .. });
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
fn inject_tag_dedupes() {
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
fn context_tag_valid() {
    // Arrange
    let src = r#"
        durable LeaderboardDo {
            shard {
                [gt 0]
                tenantId: int
            }
        }

        durable GlobalDo {}

        model Leaderboard {}

        api Leaderboard {
            [inject LeaderboardDo(tenantId)]
            get topScores(tenantId: int) -> json

            [inject GlobalDo()]
            get config() -> json
        }
    "#;
    let parse = lex_and_ast(src);

    // Act
    let (result, errors) = SemanticAnalysis::analyze(&parse);

    // Assert
    assert_eq!(errors.len(), 0, "unexpected errors: {:#?}", errors);

    let apis = &result.models.get("Leaderboard").unwrap().apis;

    let top = apis.iter().find(|a| a.name == "topScores").unwrap();
    let target = top.durable_target.as_ref().expect("durable target");
    assert_eq!(target.binding, "LeaderboardDo");
    assert_eq!(target.shard_args, vec!["tenantId"]);
    assert!(top.injected.contains(&idl::CONTEXT_INJECT_KEY));

    // The shard field's `[gt 0]` validator is inherited onto the matching param.
    let tenant = top
        .parameters
        .iter()
        .find(|p| p.name == "tenantId")
        .unwrap();
    assert_eq!(tenant.validators.len(), 1);
    assert!(matches!(
        tenant.validators[0],
        Validator::GreaterThan(Number::Int(0))
    ));

    let config = apis.iter().find(|a| a.name == "config").unwrap();
    let target = config.durable_target.as_ref().expect("durable target");
    assert_eq!(target.binding, "GlobalDo");
    assert!(target.shard_args.is_empty());
}

#[test]
fn inject_binding_namespace_and_context() {
    // Arrange
    let src = r#"
        durable GlobalDo {}

        model Global {}

        api Global {
            [inject GlobalDo, GlobalDo()]
            get config() -> json
        }
    "#;
    let parse = lex_and_ast(src);

    // Act
    let (result, errors) = SemanticAnalysis::analyze(&parse);

    // Assert
    assert_eq!(errors.len(), 0, "unexpected errors: {:#?}", errors);

    let api = &result.models.get("Global").unwrap().apis[0];

    // `GlobalDo` injects the binding namespace; `GlobalDo()` the DO context.
    assert!(api.injected.contains(&"GlobalDo"));
    assert!(api.injected.contains(&idl::CONTEXT_INJECT_KEY));
    let target = api.durable_target.as_ref().expect("durable target");
    assert_eq!(target.binding, "GlobalDo");
}

#[test]
fn context_tag_errors() {
    // Arrange
    let src = r#"
        durable LeaderboardDo {
            shard {
                tenantId: int
            }
        }

        durable GlobalDo {}

        model Leaderboard {}

        api Leaderboard {
            [inject NotADo(tenantId)]           // unknown binding
            get unknownBinding(tenantId: int) -> json

            [inject LeaderboardDo()]            // missing shard arg
            get argCount(tenantId: int) -> json

            [inject LeaderboardDo(nope)]        // arg is not a param
            get unknownParam(tenantId: int) -> json

            [inject LeaderboardDo(tenantId)]    // param type mismatch (string vs int)
            get typeMismatch(tenantId: string) -> json

            [inject LeaderboardDo(tenantId)]    // multiple context entries
            [inject GlobalDo()]
            get multiple(tenantId: int) -> json
        }
    "#;
    let parse = lex_and_ast(src);

    // Act
    let (_, errors) = SemanticAnalysis::analyze(&parse);

    // Assert
    let unresolved: Vec<&str> = errors
        .iter()
        .filter_map(|e| match e {
            SemanticError::UnresolvedSymbol { symbol } => Some(symbol.name),
            _ => None,
        })
        .collect();
    assert!(unresolved.contains(&"NotADo"), "got: {unresolved:?}");
    assert!(unresolved.contains(&"nope"), "got: {unresolved:?}");

    let (field, expected, got) = expect_err!(errors,
        SemanticError::ArgCountMismatch { field, expected, got } => (*field, *expected, *got)
    );
    assert_eq!(field.name, "LeaderboardDo");
    assert_eq!(expected, 1);
    assert_eq!(got, 0);

    let arg = expect_err!(errors,
        SemanticError::ArgTypeMismatch { arg, .. } => arg
    );
    assert_eq!(arg.name, "tenantId");

    expect_err!(errors, SemanticError::TagInvalidInContext { .. });
}

#[test]
fn durable_backing_valid() {
    // Arrange
    let src = r#"
        durable LeaderboardDo {
            shard {
                [gt 0]
                tenantId: int
            }

            topEntryCache() -> json {
                "top"
            }

            tenantScoped(tenantId: int) -> json {
                "scoped/{tenantId}"
            }
        }

        durable GlobalDo {
            config() -> json {
                "config"
            }
        }

        model Leaderboard for LeaderboardDo(org) {
            kv LeaderboardDo::topEntryCache {
                top
            }

            kv LeaderboardDo::tenantScoped(org) {
                scoped
            }
        }

        model Settings for GlobalDo {
            kv GlobalDo::config {
                config
            }
        }
    "#;
    let parse = lex_and_ast(src);

    // Act
    let (result, errors) = SemanticAnalysis::analyze(&parse);

    // Assert
    assert_eq!(errors.len(), 0, "unexpected errors: {:#?}", errors);

    let leaderboard = result.models.get("Leaderboard").unwrap();
    assert!(leaderboard.is_durable_backed());
    let backing = leaderboard.backing.as_ref().expect("durable backing");
    assert_eq!(backing.binding, "LeaderboardDo");
    assert_eq!(backing.fields, vec!["org"]);

    let org = leaderboard
        .route_fields
        .iter()
        .find(|f| f.name == "org")
        .expect("org route field (aliased shard field)");
    assert_eq!(org.cidl_type, CidlType::Int);
    assert_eq!(org.validators.len(), 1);
    assert!(matches!(
        org.validators[0],
        Validator::GreaterThan(Number::Int(0))
    ));

    let top_entry = leaderboard
        .kv_fields
        .iter()
        .find(|kv| kv.field.name == "top")
        .expect("top kv field");
    assert_eq!(top_entry.binding, "LeaderboardDo");

    let scoped = leaderboard
        .kv_fields
        .iter()
        .find(|kv| kv.field.name == "scoped")
        .expect("scoped kv field");
    assert_eq!(scoped.binding, "LeaderboardDo");
    assert_eq!(scoped.key_format, "scoped/{org}");

    let settings = result.models.get("Settings").unwrap();
    assert!(settings.is_durable_backed());
    let backing = settings.backing.as_ref().expect("durable backing");
    assert_eq!(backing.binding, "GlobalDo");
    assert!(backing.fields.is_empty());
    assert!(settings.route_fields.is_empty());
}

#[test]
fn durable_backing_errors() {
    // Arrange
    let src = r#"
        d1 {
            my_d1
        }

        durable LeaderboardDo {
            shard {
                tenantId: int
            }

            topEntryCache() -> json {
                "top"
            }
        }

        durable OtherDo {
            other() -> json {
                "other"
            }
        }

        // Shard arg count mismatch: DO has one shard field, model supplies none.
        model MissingShard for LeaderboardDo {}

        // Shard args supplied for a non-DO (D1) binding.
        model ShardOnD1 for my_d1(tenantId) {
            primary {
                id: int
            }
        }

        // Uses a storage template of a DO that is not this model's backing.
        model ForeignTemplate for LeaderboardDo(tenantId) {
            kv OtherDo::other {
                other
            }
        }
    "#;
    let parse = lex_and_ast(src);

    // Act
    let (_, errors) = SemanticAnalysis::analyze(&parse);

    // Assert
    let (field, expected, got) = expect_err!(errors,
        SemanticError::ArgCountMismatch { field, expected, got } => (*field, *expected, *got)
    );
    assert_eq!(field.name, "LeaderboardDo");
    assert_eq!(expected, 1);
    assert_eq!(got, 0);

    let unresolved = errors
        .iter()
        .filter_map(|e| match e {
            SemanticError::UnresolvedSymbol { symbol } => Some(symbol.name),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(unresolved.contains(&"OtherDo"), "got: {unresolved:?}");

    let invalid_d1 = errors
        .iter()
        .filter_map(|e| match e {
            SemanticError::ModelInvalidBinding { model, .. } => Some(model.name),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(invalid_d1.contains(&"ShardOnD1"), "got: {invalid_d1:?}");
}

#[test]
fn route_model_durable_backing() {
    let src = r#"
        durable GlobalDo {
            config() -> json {
                "config"
            }
        }

        durable LeaderboardDo {
            shard {
                tenantId: int
            }
        }

        model RouteGlobal for GlobalDo {
            route {
                id: int
            }

            kv GlobalDo::config {
                config
            }
        }

        // A sharded DO backing: `tenantId` is inherited from the shard, `rank` is explicit.
        model RouteSharded for LeaderboardDo(tenantId) {
            route {
                rank: int
            }
        }
    "#;
    let parse = lex_and_ast(src);

    // Act
    let (result, errors) = SemanticAnalysis::analyze(&parse);

    // Assert
    assert_eq!(errors.len(), 0, "unexpected errors: {:#?}", errors);

    let route_global = result.models.get("RouteGlobal").unwrap();
    assert!(route_global.is_durable_backed());
    assert_eq!(route_global.backing.as_ref().unwrap().binding, "GlobalDo");

    let route_fields: Vec<&str> = route_global
        .route_fields
        .iter()
        .map(|f| f.name.as_ref())
        .collect();
    assert_eq!(route_fields, vec!["id"]);
    assert!(
        route_global
            .kv_fields
            .iter()
            .any(|kv| kv.field.name == "config")
    );

    let route_sharded = result.models.get("RouteSharded").unwrap();
    assert!(route_sharded.is_durable_backed());
    assert_eq!(
        route_sharded.backing.as_ref().unwrap().fields,
        vec!["tenantId"]
    );
    let mut sharded_fields: Vec<&str> = route_sharded
        .route_fields
        .iter()
        .map(|f| f.name.as_ref())
        .collect();
    sharded_fields.sort();
    assert_eq!(sharded_fields, vec!["rank", "tenantId"]);
}

#[test]
fn route_model_durable_backing_errors() {
    let src = r#"
        d1 {
            my_d1
        }

        durable LeaderboardDo {
            shard {
                tenantId: int
            }
        }

        // A route model cannot be backed by a D1 database.
        model RouteD1 for my_d1 {
            route {
                id: int
            }
        }

        // A route field cannot redeclare an inherited shard field (caught as a duplicate).
        model RouteShardCollision for LeaderboardDo(tenantId) {
            route {
                tenantId: int
            }
        }
    "#;
    let parse = lex_and_ast(src);

    // Act
    let (_, errors) = SemanticAnalysis::analyze(&parse);

    // Assert
    expect_err!(errors, SemanticError::ModelMixesRoutesAndSql { .. });
    let dup = expect_err!(errors,
        SemanticError::DuplicateSymbol { second, .. } => second
    );
    assert_eq!(dup.name, "tenantId");
}
