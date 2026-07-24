#![allow(unused_variables)]

use compiler_test::lex_and_ast;
use frontend::Ast;
use idl::{
    BackingKind, CidlType, CloesceIdl, MediaType, NavigationCardinality, Number, ParamSource,
    Validator,
};
use semantic::err::SemanticError;

fn analyze<'src, 'p>(ast: &'p Ast<'src>) -> (CloesceIdl<'src>, Vec<SemanticError<'src, 'p>>) {
    match semantic::analyze(ast) {
        Ok(idl) => (idl, vec![]),
        Err(errors) => (CloesceIdl::default(), errors),
    }
}

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
            cached -> json {{
                id: int
                "users/{{id}}"
            }}

            items -> json {{
                field: string
                "items/{{field}}"
            }}
        }}

        r2 my_r2 {{
            avatar {{
                id: int
                "avatars/{{id}}"
            }}

            obj {{
                field: string
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
    let (result, errors) = analyze(&parse);

    // Assert
    assert_eq!(errors.len(), 1);
    let second = expect_err!(errors,
        SemanticError::DuplicateSymbol { second, .. } => second
    );
    assert_eq!(second.name, "my_d1");
}

#[test]
fn d1_model_errors() {
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
    let (result, errors) = analyze(&parse);

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

            foreign Post::invalid {
                doesntExist
            }

            foreign OtherD1Model::id {
                shouldAlsoError
            }

            foreign Post::id {
                validForeignKey
            }

            foreign DoesNotExist::id {
                adjacentModelDoesNotExist
            }

            foreign Post::nonexistent {
                adjacentFieldDoesNotExist
            }

            foreign Post::{ id, id } {
                inconsistentFieldAdjacency
            }

            // nav target field `bogus` does not exist on Post
            one Post::bogus(id) { byBadTarget }

            // nav local field `ghost` does not exist on User
            one Post::id(ghost) { byBadLocal }

            // nav bare form omits the local field
            one Post::id { byBareKey }
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
    let (result, errors) = analyze(&parse);

    // Assert
    assert_eq!(errors.len(), 11);

    let column = expect_err!(errors,
        SemanticError::NullablePrimaryKey { column } => column
    );
    assert_eq!(column.name, "id");

    let second = expect_err!(errors,
        SemanticError::DuplicateSymbol { second, .. } => second
    );
    assert_eq!(second.name, "id");

    let fk_model = expect_err!(errors,
        SemanticError::ForeignKeyReferencesDifferentDatabase { fk_model, .. } => fk_model.name
    );
    assert_eq!(fk_model, "OtherD1Model");

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

    // A nav key must resolve its target field (on Post) and its local field (on User),
    // and may not omit the local field with the bare `Target::field` form.
    let unresolved_nav_fields = errors
        .iter()
        .filter_map(|e| match e {
            SemanticError::UnresolvedSymbol { symbol }
                if symbol.name == "bogus" || symbol.name == "ghost" =>
            {
                Some(symbol.name)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(unresolved_nav_fields.len(), 2);

    let missing_local = expect_err!(errors,
        SemanticError::RelationMissingLocalKey { target } => target.name
    );
    assert_eq!(missing_local, "id");
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

            foreign Horse::id {
                horseId
            }

            one Horse::id(horseId) {
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
    let (result, errors) = analyze(&parse);

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
            && matches!(nav.cardinality, NavigationCardinality::One)
            && nav.keys.len() == 1
            && nav.keys[0].local == "horseId"
            && nav.keys[0].target == "id"
            && nav.field.cidl_type == CidlType::Object { name: "Horse" }
    }));
}

#[test]
fn d1_model_nav_one_to_many() {
    // Arrange
    let src = &with_env(
        r#"
        model Author for my_d1 {
            primary { id: int }

            many Post::authorId(id) {
                posts
            }
        }

        model Post for my_d1 {
            primary { id: int }

            foreign Author::id {
                authorId
            }
        }
        "#,
    );

    // Act
    let parse = lex_and_ast(src);
    let (result, errors) = analyze(&parse);

    // Assert
    assert_eq!(errors.len(), 0, "unexpected errors: {:#?}", errors);

    let author = result.models.get("Author").unwrap();

    let author_posts_nav = author.navigation_fields.first().unwrap();
    assert_eq!(author_posts_nav.field.name, "posts");
    assert_eq!(author_posts_nav.model_reference, "Post");
    assert!(matches!(
        author_posts_nav.cardinality,
        NavigationCardinality::Many
    ));

    assert_eq!(author_posts_nav.keys.len(), 1);
    assert_eq!(author_posts_nav.keys[0].local, "id");
    assert_eq!(author_posts_nav.keys[0].target, "authorId");
}

#[test]
fn route_model_valid() {
    // Arrange
    let src = &with_env(
        r#"
        [crud get, save]
        model Person {
            route {
                id: int
                org: string
            }

            kv my_kv::cached(id) { cached }

            one Dog::{ tenant(org), ownerId(id) } { dog }
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
    let (result, errors) = analyze(&parse);

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

    // The 1:1 nav resolves each target discriminator to a local field, in source order.
    let dog_nav = person.navigation_fields.first().unwrap();
    assert_eq!(dog_nav.field.name, "dog");
    assert_eq!(dog_nav.model_reference, "Dog");
    assert_eq!(dog_nav.field.cidl_type, CidlType::Object { name: "Dog" });
    assert!(matches!(dog_nav.cardinality, NavigationCardinality::One));
    let keys: Vec<(&str, &str)> = dog_nav.keys.iter().map(|k| (k.local, k.target)).collect();
    assert_eq!(keys, vec![("org", "tenant"), ("id", "ownerId")]);

    // The default data source is keyed on the route fields.
    let ds = person.default_data_source().unwrap();
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

            one Person::{ id(id), tenant(org) } { person }
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
    let (result, errors) = analyze(&parse);

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
    assert!(matches!(person_nav.cardinality, NavigationCardinality::One));

    // Person is worker-backed, so its resolved target backing is None.
    assert!(person_nav.target_backing.is_none());

    let keys: Vec<(&str, &str)> = person_nav
        .keys
        .iter()
        .map(|k| (k.local, k.target))
        .collect();
    assert_eq!(keys, vec![("id", "id"), ("org", "tenant")]);
}

#[test]
fn keyless_singleton_nav() {
    let src = r#"
    kv GlobalKv {
        config -> json {
            "config"
        }
    }

    durable AppDo {
        settings -> json {
            "settings"
        }
    }

    model AppConfig for AppDo {
        kv GlobalKv::config {
            config
        }
    }

    model App for AppDo {
        kv AppDo::settings {
            settings
        }

        one AppConfig { appConfig }
    }
    "#;

    let parse = lex_and_ast(src);
    let (result, errors) = analyze(&parse);

    assert_eq!(errors.len(), 0, "unexpected errors: {:#?}", errors);

    let app = result.models.get("App").unwrap();
    let nav = app.navigation_fields.first().unwrap();
    assert_eq!(nav.field.name, "appConfig");
    assert_eq!(nav.model_reference, "AppConfig");
    assert_eq!(nav.field.cidl_type, CidlType::Object { name: "AppConfig" });
    assert!(matches!(nav.cardinality, NavigationCardinality::One));
    assert!(nav.keys.is_empty(), "expected a discriminator-less nav");
}

#[test]
fn d1_model_cyclical_relationship_error() {
    // Arrange
    let src = &with_env(
        r#"
        model A for my_d1 {
            primary { id: int }

            foreign B::id {
                bId2
            }

            one B::id(bId2) { toB }
        }

        model B for my_d1 {
            primary { id: int }

            foreign C::id {
                cId
            }

            one C::id(cId) { toC }
        }

        model C for my_d1 {
            primary { id: int }

            foreign A::id {
                aId
            }

            one A::id(aId) { toA }
        }
        "#,
    );

    // Act
    let parse = lex_and_ast(src);
    let (result, errors) = analyze(&parse);

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

            foreign B::id option {
                bId
            }

            one B::id(bId) { toB }
        }

        model B for my_d1 {
            primary { id: int }

            foreign C::id option {
                cId
            }

            one C::id(cId) { toC }
        }

        model C for my_d1 {
            primary { id: int }

            foreign A::id option {
                aId
            }

            one A::id(aId) { toA }
        }
        "#,
    );

    // Act
    let parse = lex_and_ast(src);
    let (_, errors) = analyze(&parse);

    // Assert
    assert_eq!(errors.len(), 0);
}

#[test]
fn kv_r2_errors() {
    // Arrange
    let src = &with_env(
        r#"
        durable MyDurable {
            shard {
                doId: string
            }

            value -> string {
                key: string
                "value/{key}"
            }

            other -> string {
                key: string
                "other/{key}"
            }
        }

        model Foo for my_d1 {
            primary { field: string }

            // invalid binding type (my_d1 is a D1, not KV)
            kv my_d1::items(field) { foo }

            // invalid binding type (my_kv is a KV, not R2)
            r2 my_kv::items(field) { obj }

            // `bogus` is not a Workers KV shard (KV has none).
            kv my_kv::{ items(field), bogus(field) } { strayKvArg }
        }

        model Bar {
            route {
                key: string
                doId: string
            }

            // missing the `doId(doId)` shard discriminator.
            kv MyDurable::value(key) { missingShard }

            // no storage template referenced (only the shard discriminator).
            kv MyDurable::doId(doId) { noTemplate }

            // two storage templates referenced.
            kv MyDurable::{ value(key), other(key), doId(doId) } { twoTemplates }

            // `nope` is not a shard field of MyDurable.
            kv MyDurable::{ value(key), doId(doId), nope(key) } { strayDoArg }
        }
        "#,
    );
    let parse = lex_and_ast(src);

    // Act
    let (_, errors) = analyze(&parse);

    // Assert
    let unresolved: Vec<&str> = errors
        .iter()
        .filter_map(|e| match e {
            SemanticError::UnresolvedSymbol { symbol } => Some(symbol.name),
            _ => None,
        })
        .collect();
    for name in ["my_d1", "my_kv", "bogus", "nope"] {
        assert!(
            unresolved.contains(&name),
            "expected '{name}' unresolved, got: {unresolved:?}"
        );
    }

    let missing = expect_err!(errors,
        SemanticError::RelationMissingDiscriminator { field, missing } => (field.name, *missing)
    );
    assert_eq!(missing, ("missingShard", "doId"));

    let counts: Vec<(&str, usize)> = errors
        .iter()
        .filter_map(|e| match e {
            SemanticError::KvTemplateCount { field, count } => Some((field.name, *count)),
            _ => None,
        })
        .collect();
    assert!(counts.contains(&("noTemplate", 0)), "got: {counts:?}");
    assert!(counts.contains(&("twoTemplates", 2)), "got: {counts:?}");
}

#[test]
fn nav_requires_every_target_route_field() {
    // Arrange
    let src = &with_env(
        r#"
        durable SubRedditDo {
            shard {
                id: int
            }
        }

        model Post for SubRedditDo(subId) {
            primary { id: int }
            column { title: string }

            // `subId(subId)` supplies the shard route field: valid.
            many Comment::{ postId(id), subId(subId) } { comments }

            // No shard key at all.
            many Comment::postId(id) { noShard }

            // Both of the target's plain route fields supplied: valid.
            one Ledger::{ region(title), owner(title) } { ledger }

            // `owner` is not supplied.
            one Ledger::region(title) { partialLedger }
        }

        model Comment for SubRedditDo(subId) {
            primary { id: int }
            foreign Post::id { postId }
        }

        model Ledger for my_d1 {
            primary { lid: int }
            route {
                region: string
                owner: string
            }
        }
        "#,
    );

    let src = src.as_str();
    let parse = lex_and_ast(src);

    // Act
    let (result, errors) = analyze(&parse);

    // Assert
    let missing: Vec<(&str, &str)> = errors
        .iter()
        .filter_map(|e| match e {
            SemanticError::RelationMissingDiscriminator { field, missing } => {
                Some((field.name, *missing))
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        missing,
        vec![("noShard", "subId"), ("partialLedger", "owner"),],
        "got errors: {errors:#?}"
    );
    assert_eq!(errors.len(), 2, "unexpected errors: {errors:#?}");
}

#[test]
fn binding_key_format_unknown_param() {
    // Arrange
    let src = r#"
        kv UserMeta {
            meta -> json {
                id: int
                "metadata/{id}/{bogus}"
            }
        }

        r2 UserAvatars {
            avatar {
                id: int
                "avatars/{ghost}.jpg"
            }
        }
    "#;

    // Act
    let parse = lex_and_ast(src);
    let (_, errors) = analyze(&parse);

    // Assert
    let unknowns = errors
        .iter()
        .filter_map(|e| match e {
            SemanticError::TemplateUnknownVariable { variable, .. } => Some(*variable),
            _ => None,
        })
        .collect::<Vec<&str>>();
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
            entry -> json {
                id: int
                "entry/{id" // missing closing brace
            }
        }

        r2 NsB {
            obj {
                id: int
                "obj/{{id}" // nested braces not allowed
            }
        }
    "#;

    // Act
    let parse = lex_and_ast(src);
    let (_, errors) = analyze(&parse);

    // Assert
    assert_eq!(
        count_errs!(errors, SemanticError::TemplateInvalidFormat { .. }),
        2,
        "expected two invalid-key-format errors, got: {:#?}",
        errors
    );
}

#[test]
fn binding_template_prefix_is_computed() {
    // Arrange
    let src = r#"
        kv NS {
            a -> json {
                id: int
                "path/to/data/{id}"
            }
            b -> json {
                id: int
                "data/{id}/value"
            }
            c -> json { "top" }
        }
    "#;

    // Act
    let parse = lex_and_ast(src);
    let (idl, errors) = analyze(&parse);

    // Assert
    assert_eq!(errors.len(), 0, "{errors:#?}");

    let ns = idl
        .wrangler_env
        .kv_bindings
        .iter()
        .find(|b| b.name == "NS")
        .unwrap();
    let prefix = |field: &str| {
        &ns.templates
            .iter()
            .find(|t| t.field.name == field)
            .unwrap()
            .prefix
    };
    assert_eq!(prefix("a"), "path/to/data/");
    assert_eq!(prefix("b"), "data/");
    assert_eq!(prefix("c"), "top");
}

#[test]
fn key_format_overlap_is_detected_and_scoped_per_namespace() {
    let src = r#"
        kv NS {
            a -> json {
                id: int
                "foo/{id}" // overlaps with b
            }
            b -> json { "foo/bar" }
        }
        kv Other {
            c -> json {
                id: int
                "foo/{id}" // overlaps but diff NS
            }
        }
    "#;

    // Act
    let parse = lex_and_ast(src);
    let (_, errors) = analyze(&parse);

    // Assert
    assert_eq!(
        count_errs!(errors, SemanticError::KeyFormatOverlap { .. }),
        1,
        "{errors:#?}"
    );
    let (first, second) = expect_err!(errors,
        SemanticError::KeyFormatOverlap { first, second } => (first.name, second.name)
    );
    assert_eq!(first, "a");
    assert_eq!(second, "b");
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
            value -> int {
                [step 5]
                foo: int
                "users/{foo}"
            }
        }

        r2 MyR2 {
            obj {
                [len 5]
                bar: string
                [gt 0]
                baz: int
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
    let (result, errors) = analyze(&parse);

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
            get badReturn -> option<stream> {}
        }

        // Object parameter on GET
        api User {
            get badGetObj -> string {
                u: User
            }
        }

        // R2Object parameter on GET
        api User {
            get badGetR2 -> string {
                r: r2object
            }
        }

        // Stream param with extra non-inject params (invalid)
        api User {
            post badStream -> stream {
                s: stream
                extra: string
            }
        }
    "#,
    );
    let parse = lex_and_ast(src);

    // Act
    let (_, errors) = analyze(&parse);

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
            self post streamInputOutput -> stream {
                s: stream

                inject { my_d1 }
            }

            get jsonInputOutput -> json {
                j: json
            }
        }
    "#,
    );
    let parse = lex_and_ast(src);

    // Act
    let (result, errors) = analyze(&parse);

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
fn api_header_param_source() {
    // Arrange
    let src = &with_env(
        r#"
        model User for my_d1 {
            primary {
                id: int
            }
        }

        api User {
            post withHeader -> string {
                [header]
                Authorization: string

                body: string
            }
        }
    "#,
    );
    let parse = lex_and_ast(src);

    // Act
    let (result, errors) = analyze(&parse);

    // Assert
    assert_eq!(errors.len(), 0, "unexpected errors: {:#?}", errors);
    let method = result
        .models
        .get("User")
        .unwrap()
        .apis
        .iter()
        .find(|m| m.name == "withHeader")
        .unwrap();

    let auth = method
        .parameters
        .iter()
        .find(|p| p.field.name == "Authorization")
        .unwrap();
    assert_eq!(auth.source, ParamSource::Header);

    let body = method
        .parameters
        .iter()
        .find(|p| p.field.name == "body")
        .unwrap();
    assert_eq!(body.source, ParamSource::Body);
}

#[test]
fn api_bare_self_defaults_to_default() {
    // Arrange
    let src = r#"
        d1 { db }

        model Item for db {
            primary { id: int }
        }

        api Item {
            self post edit -> Item {
                input: string
            }
        }
    "#;
    let parse = lex_and_ast(src);

    // Act
    let (result, errors) = analyze(&parse);

    // Assert
    assert_eq!(errors.len(), 0, "unexpected errors: {:#?}", errors);
    let edit = result
        .models
        .get("Item")
        .unwrap()
        .apis
        .iter()
        .find(|m| m.name == "edit")
        .unwrap();
    assert!(!edit.is_static);
    assert_eq!(edit.data_source, Some("Default"));
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

            many Post::authorId(id) {
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

            foreign User::id {
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
            get {
                u: User
            }
        }
    "#,
    );
    let parse = lex_and_ast(src);

    // Act
    let (_, errors) = analyze(&parse);

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
    let (result, errors) = analyze(&parse);

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
    let (_result, errors) = analyze(&parse);

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
    let (result, errors) = analyze(&parse);

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
            self post resolveAll -> string {
                p: array<MyPoo>
                u: User

                inject { my_d1 }
            }
        }
    "#;

    // Act
    let parse = lex_and_ast(src);
    let (result, errors) = analyze(&parse);

    // Assert
    assert_eq!(errors.len(), 0, "unexpected errors: {:#?}", errors);

    let api = result.models.get("User").unwrap().apis.first().unwrap();
    let param_types: Vec<_> = api
        .parameters
        .iter()
        .map(|p| p.field.cidl_type.clone())
        .collect();
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

            foreign User::id {
                userId
            }

            foreign User::age {
                userAge
            }
        }
        "#,
    );
    let parse = lex_and_ast(&src);
    let (result, errors) = analyze(&parse);

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
    let (_, errors) = analyze(&parse);

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
    let (result, errors) = analyze(&parse);

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
            entry -> json {
                id: int
                "entry/{id}"
            }
        }

        var { API_KEY: string }

        inject { YouTubeApi }

        model M for db {
            primary { id: int }
        }

        api M {
            self get all -> string {
                inject {
                    db
                    cache
                    API_KEY
                    YouTubeApi
                }
            }
        }
    "#;

    let parse = lex_and_ast(src);
    let (result, errors) = analyze(&parse);
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
            self get dup -> string {
                inject {
                    db
                    db
                }
            }
        }
    "#;

    let parse = lex_and_ast(src);
    let (result, errors) = analyze(&parse);
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
            get topScores -> json {
                tenantId: int

                inject {
                    LeaderboardDo::tenantId(tenantId)
                }
            }

            get config -> json {
                inject {
                    GlobalDo::{}
                }
            }
        }
    "#;
    let parse = lex_and_ast(src);

    // Act
    let (result, errors) = analyze(&parse);

    // Assert
    assert_eq!(errors.len(), 0, "unexpected errors: {:#?}", errors);

    let apis = &result.models.get("Leaderboard").unwrap().apis;

    let top = apis.iter().find(|a| a.name == "topScores").unwrap();
    let target = top.durable_target.as_ref().expect("durable target");
    assert_eq!(target.binding, "LeaderboardDo");
    assert_eq!(target.shard_args, vec!["tenantId"]);

    // The shard field's `[gt 0]` validator is inherited onto the matching param.
    let tenant = top
        .parameters
        .iter()
        .find(|p| p.field.name == "tenantId")
        .unwrap();
    assert_eq!(tenant.field.validators.len(), 1);
    assert!(matches!(
        tenant.field.validators[0],
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
            get config -> json {
                inject {
                    GlobalDo
                    GlobalDo::{}
                }
            }
        }
    "#;
    let parse = lex_and_ast(src);

    // Act
    let (result, errors) = analyze(&parse);

    // Assert
    assert_eq!(errors.len(), 0, "unexpected errors: {:#?}", errors);

    let api = &result.models.get("Global").unwrap().apis[0];

    // `GlobalDo` injects the binding namespace; `GlobalDo()` the DO context.
    assert!(api.injected.contains(&"GlobalDo"));
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
            get unknownBinding -> json {
                tenantId: int
                inject { NotADo::tenantId(tenantId) }   // unknown binding
            }

            get missingShard -> json {
                tenantId: int
                inject { LeaderboardDo::{} }             // missing shard field
            }

            get unknownParam -> json {
                tenantId: int
                inject { LeaderboardDo::tenantId(nope) } // arg is not a param
            }

            get typeMismatch -> json {
                tenantId: string
                inject { LeaderboardDo::tenantId(tenantId) } // type mismatch (string vs int)
            }

            get multiple -> json {
                tenantId: int
                inject {
                    LeaderboardDo::tenantId(tenantId)    // multiple context entries
                    GlobalDo::{}
                }
            }
        }
    "#;
    let parse = lex_and_ast(src);

    // Act
    let (_, errors) = analyze(&parse);

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

    let missing = expect_err!(errors,
        SemanticError::DurableMissingShardField { context, missing } => (context.name, *missing)
    );
    assert_eq!(missing, ("LeaderboardDo", "tenantId"));

    let arg = expect_err!(errors,
        SemanticError::ArgTypeMismatch { arg, .. } => arg
    );
    assert_eq!(arg.name, "tenantId");

    expect_err!(errors, SemanticError::ApiMultipleDurableContexts { .. });
}

#[test]
fn instantiated_method_inherits_durable_target() {
    // Arrange
    let src = r#"
        durable SubRedditDo {
            shard {
                id: int
            }
        }

        model SubReddit for SubRedditDo(subId) {
            primary { pid: int }
        }

        api SubReddit {
            self get feed -> json {
                subId: int
            }
        }
    "#;
    let parse = lex_and_ast(src);

    // Act
    let (result, errors) = analyze(&parse);

    // Assert
    assert_eq!(errors.len(), 0, "unexpected errors: {:#?}", errors);

    let feed = result
        .models
        .get("SubReddit")
        .unwrap()
        .apis
        .iter()
        .find(|a| a.name == "feed")
        .unwrap();
    let target = feed
        .durable_target
        .as_ref()
        .expect("inherited durable target");
    assert_eq!(target.binding, "SubRedditDo");
    assert_eq!(target.shard_args, vec!["subId"]);
    assert!(feed.injected.is_empty());
}

#[test]
fn instantiated_method_injecting_durable_conflicts() {
    // Arrange
    let src = r#"
        durable SubRedditDo {
            shard {
                id: int
            }
        }

        model SubReddit for SubRedditDo(subId) {
            primary { pid: int }
        }

        api SubReddit {
            self get feed -> json {
                subId: int

                inject {
                    SubRedditDo::id(subId)
                }
            }
        }
    "#;
    let parse = lex_and_ast(src);

    // Act
    let (_, errors) = analyze(&parse);

    // Assert
    let method = expect_err!(errors,
        SemanticError::ApiInjectsDurableWhenSourceInjectsDurable { method } => method
    );
    assert_eq!(method.name, "feed");
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

            topEntryCache -> json {
                "top"
            }

            tenantScoped -> json {
                tenantId: int
                "scoped/{tenantId}"
            }
        }

        durable GlobalDo {
            config -> json {
                "config"
            }
        }

        model Leaderboard for LeaderboardDo(org) {
            kv LeaderboardDo::{ topEntryCache, tenantId(org) } {
                top
            }

            kv LeaderboardDo::{ tenantScoped(org), tenantId(org) } {
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
    let (result, errors) = analyze(&parse);

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

            topEntryCache -> json {
                "top"
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
    "#;
    let parse = lex_and_ast(src);

    // Act
    let (_, errors) = analyze(&parse);

    // Assert
    let (field, expected, got) = expect_err!(errors,
        SemanticError::ArgCountMismatch { field, expected, got } => (*field, expected, got)
    );
    assert_eq!(field.name, "LeaderboardDo");
    assert_eq!(*expected, 1);
    assert_eq!(*got, 0);

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
            config -> json {
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
    let (result, errors) = analyze(&parse);

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
fn route_shard_field_collision_errors() {
    let src = r#"
        durable LeaderboardDo {
            shard {
                tenantId: int
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
    let (_, errors) = analyze(&parse);

    // Assert
    let dup = expect_err!(errors,
        SemanticError::DuplicateSymbol { second, .. } => second
    );
    assert_eq!(dup.name, "tenantId");
}

// Comprehensive test for cross-database relationships
#[test]
fn proposal_relationship_matrix() {
    let src = r#"
        d1 {
            db_a
            db_b
        }

        durable DoA {
            shard { tenantId: int }

            cache -> json {
                key: int
                "cache/{key}"
            }
        }

        durable DoB {
            shard { tenantId: int }
        }

        model Worker {
            route {
                routeId: int
                tenantId: int
            }

            // Worker -> D1 (1:1)
            one D1Primary::primaryId(routeId) { d1Backed }

            // Worker -> D1 (1:N)
            many D1Primary::primaryId(routeId) { d1BackedMany }

            // DO KV from a worker reaching into a DO: shard supplied explicitly.
            kv DoA::{ cache(routeId), tenantId(tenantId) } { workerCache }
        }

        model D1Primary for db_a {
            primary {
                primaryId: int
                tenantId: int
            }

            column {
                bId: int
            }

            // D1 -> Worker (1:1)
            one Worker::{ routeId(primaryId), tenantId(tenantId) } { worker }

            // D1 -> DO (1:1)
            one DoBacked::{ tenantId(tenantId), routeId(primaryId) } { doBacked }

            // D1 -> DO (1:N)
            many DoBacked::{ tenantId(tenantId), routeId(primaryId) } { doBackedMany }

            // D1 -> D1 across databases (1:1)
            one D1Other::otherId(bId) { d1Other }

            // Unindexed target 1:1
            one Unindexed { unindexed }

            // Discriminator-less 1:N
            many AllPosts { allPosts }
        }

        model D1Other for db_b {
            primary {
                otherId: int
            }
        }

        model DoBacked for DoA(tenantId) {
            route {
                routeId: int
            }

            // DO -> D1 (1:1)
            one D1Other::otherId(routeId) { d1Other }

            // DO A -> DO B (1:1
            one DoBackedB::{ tenantId(tenantId), routeId(routeId) } { doBackedB }

            // DO A -> DO B (1:N)
            many DoBackedB::{ tenantId(tenantId), routeId(routeId) } { doBackedBMany }

            // DO KV from a DO-backed model
            kv DoA::{ cache(routeId), tenantId(tenantId) } { selfCache }
        }

        model DoBackedB for DoB(tenantId) {
            route {
                routeId: int
            }
        }

        model Unindexed {}

        model AllPosts for db_a {
            primary {
                id: int
            }
        }

        model TenantUser for DoA(tenantId) {
            primary {
                id: int
            }

            // Only the DO shard discriminator is supplied, so the target is all posts for that tenant.
            many TenantPost::tenantId(tenantId) { posts }
        }

        model TenantPost for DoA(tenantId) {
            primary {
                id: int
            }
        }
    "#;
    let parse = lex_and_ast(src);

    // Act
    let (result, errors) = analyze(&parse);

    // Assert
    assert_eq!(errors.len(), 0, "unexpected errors: {:#?}", errors);

    // Resolves a navigation by field name on a model.
    let nav = |model: &str, field: &str| {
        result
            .models
            .get(model)
            .unwrap_or_else(|| panic!("model {model}"))
            .navigation_fields
            .iter()
            .find(|n| n.field.name == field)
            .unwrap_or_else(|| panic!("nav {model}.{field}"))
    };

    // Asserts a nav's cardinality, target model, target backing kind, and resolved keys.
    let check = |model: &str,
                 field: &str,
                 cardinality: NavigationCardinality,
                 target: &str,
                 backing: Option<BackingKind>,
                 keys: &[(&str, &str)]| {
        let n = nav(model, field);
        assert_eq!(n.cardinality, cardinality, "{model}.{field} cardinality");
        assert_eq!(n.model_reference, target, "{model}.{field} target");
        assert_eq!(
            n.target_backing.as_ref().map(|b| b.kind.clone()),
            backing,
            "{model}.{field} backing"
        );
        let got: Vec<(&str, &str)> = n.keys.iter().map(|k| (k.local, k.target)).collect();
        assert_eq!(got, keys, "{model}.{field} keys");

        // `one` is a single object, `many` is an array of it.
        match n.cardinality {
            NavigationCardinality::One => {
                assert_eq!(n.field.cidl_type, CidlType::Object { name: target });
            }
            NavigationCardinality::Many => {
                assert_eq!(
                    n.field.cidl_type,
                    CidlType::Array(Box::new(CidlType::Object { name: target }))
                );
            }
        }
    };

    use NavigationCardinality::{Many, One};

    // Worker -> D1
    check(
        "Worker",
        "d1Backed",
        One,
        "D1Primary",
        Some(BackingKind::D1),
        &[("routeId", "primaryId")],
    );
    check(
        "Worker",
        "d1BackedMany",
        Many,
        "D1Primary",
        Some(BackingKind::D1),
        &[("routeId", "primaryId")],
    );

    // D1 -> Worker / DO / D1
    check(
        "D1Primary",
        "worker",
        One,
        "Worker",
        None,
        &[("primaryId", "routeId"), ("tenantId", "tenantId")],
    );
    check(
        "D1Primary",
        "doBacked",
        One,
        "DoBacked",
        Some(BackingKind::DurableObject),
        &[("tenantId", "tenantId"), ("primaryId", "routeId")],
    );
    check(
        "D1Primary",
        "doBackedMany",
        Many,
        "DoBacked",
        Some(BackingKind::DurableObject),
        &[("tenantId", "tenantId"), ("primaryId", "routeId")],
    );
    check(
        "D1Primary",
        "d1Other",
        One,
        "D1Other",
        Some(BackingKind::D1),
        &[("bId", "otherId")],
    );
    check("D1Primary", "unindexed", One, "Unindexed", None, &[]);
    check(
        "D1Primary",
        "allPosts",
        Many,
        "AllPosts",
        Some(BackingKind::D1),
        &[],
    );

    // DO -> D1 / DO
    check(
        "DoBacked",
        "d1Other",
        One,
        "D1Other",
        Some(BackingKind::D1),
        &[("routeId", "otherId")],
    );
    check(
        "DoBacked",
        "doBackedB",
        One,
        "DoBackedB",
        Some(BackingKind::DurableObject),
        &[("tenantId", "tenantId"), ("routeId", "routeId")],
    );
    check(
        "DoBacked",
        "doBackedBMany",
        Many,
        "DoBackedB",
        Some(BackingKind::DurableObject),
        &[("tenantId", "tenantId"), ("routeId", "routeId")],
    );

    // Shard-only `many`: only the shard discriminator is supplied.
    check(
        "TenantUser",
        "posts",
        Many,
        "TenantPost",
        Some(BackingKind::DurableObject),
        &[("tenantId", "tenantId")],
    );

    // DO KV resolves both from a DO-backed model and from a worker reaching into the DO.
    // Either way the shard discriminator is supplied explicitly.
    let kv_field = |model: &str, field: &str| {
        result
            .models
            .get(model)
            .unwrap()
            .kv_fields
            .iter()
            .find(|kv| kv.field.name == field)
            .unwrap_or_else(|| panic!("kv {model}.{field}"))
    };

    let do_kv = kv_field("DoBacked", "selfCache");
    assert_eq!(do_kv.binding, "DoA");
    assert_eq!(do_kv.key_format, "cache/{routeId}");
    assert_eq!(do_kv.shard_fields, vec!["tenantId"]);

    let worker_kv = kv_field("Worker", "workerCache");
    assert_eq!(worker_kv.binding, "DoA");
    assert_eq!(worker_kv.key_format, "cache/{routeId}");
    assert_eq!(worker_kv.shard_fields, vec!["tenantId"]);
}
