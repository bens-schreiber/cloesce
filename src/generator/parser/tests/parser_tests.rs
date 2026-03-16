use ast::{CidlType, CloesceAst, NavigationPropertyKind};
use lexer::Lexer;
use parser::CloesceParser;

fn lex_and_parse(src: &str) -> CloesceAst {
    let tokens = Lexer::default().lex(src).expect("lex to succeed");
    CloesceParser::default()
        .parse(tokens)
        .expect("parse to succeed")
}

#[test]
fn env_block() {
    let ast = lex_and_parse(
        r#"
        env {
            // bindings
            db: d1
            db2: d1
            assets: r2
            cache: kv

            // variables
            api_url: string
            max_retries: int
            threshold: double
            created_at: date
            payload: json
            enabled: bool
        }
        "#,
    );

    let env = ast
        .wrangler_env
        .first()
        .expect("wrangler_env to be present");

    assert_eq!(
        env.d1_bindings.iter().map(|b| &b.name).collect::<Vec<_>>(),
        vec!["db", "db2"]
    );

    assert_eq!(
        env.r2_bindings.iter().map(|b| &b.name).collect::<Vec<_>>(),
        vec!["assets"]
    );

    assert_eq!(
        env.kv_bindings.iter().map(|b| &b.name).collect::<Vec<_>>(),
        vec!["cache"]
    );

    assert_eq!(
        env.vars
            .values()
            .find(|v| v.name == "api_url")
            .expect("api_url var to be present")
            .cidl_type,
        CidlType::String
    );
    assert_eq!(
        env.vars
            .values()
            .find(|v| v.name == "max_retries")
            .expect("max_retries var to be present")
            .cidl_type,
        CidlType::Integer
    );
    assert_eq!(
        env.vars
            .values()
            .find(|v| v.name == "threshold")
            .expect("threshold var to be present")
            .cidl_type,
        CidlType::Double
    );
    assert_eq!(
        env.vars
            .values()
            .find(|v| v.name == "created_at")
            .expect("created_at var to be present")
            .cidl_type,
        CidlType::DateIso
    );
    assert_eq!(
        env.vars
            .values()
            .find(|v| v.name == "payload")
            .expect("payload var to be present")
            .cidl_type,
        CidlType::JsonValue
    );
    assert_eq!(
        env.vars
            .values()
            .find(|v| v.name == "enabled")
            .expect("enabled var to be present")
            .cidl_type,
        CidlType::Boolean
    );
}

#[test]
fn model_block_scalar() {
    let ast = lex_and_parse(
        r#"
        [d1_a]
        model Person {
            // Composite PK id, age
            [primary id, age]
            age: int
            id: int

            name: string
            
            active: bool
            birthday: date
            score: double
        }
        "#,
    );

    let (_, model) = ast.models.first().expect("model to be present");
    assert_eq!(model.name, "Person");
    assert_eq!(
        model.d1_binding.as_ref().map(|b| b.name.as_str()),
        Some("d1_a")
    );

    let primary_key_cols: Vec<&str> = model.primary_keys().map(|f| f.name.as_str()).collect();
    assert_eq!(primary_key_cols, vec!["age", "id"]);

    let col_names: Vec<&str> = model.columns.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(
        col_names,
        vec!["age", "id", "name", "active", "birthday", "score"]
    );

    let col_types: Vec<CidlType> = model.columns.iter().map(|c| c.cidl_type.clone()).collect();
    assert_eq!(
        col_types,
        vec![
            CidlType::Integer,
            CidlType::Integer,
            CidlType::String,
            CidlType::Boolean,
            CidlType::DateIso,
            CidlType::Double
        ]
    );
}

#[test]
fn model_block_single_foreign_key() {
    let ast = lex_and_parse(
        r#"
        [d1_a]
        model Person {
            [primary id]
            id: int
        }

        [d1_a]
        model Dog {
            [primary id]
            [foreign userId -> Person::id]
            id: int
            userId: int
        }
        "#,
    );

    let person = ast
        .models
        .iter()
        .find(|(_, m)| m.name == "Person")
        .expect("Person model to be present")
        .1;

    let dog = ast
        .models
        .iter()
        .find(|(_, m)| m.name == "Dog")
        .expect("Dog model to be present")
        .1;

    assert_eq!(
        dog.d1_binding.as_ref().map(|b| b.name.as_str()),
        Some("d1_a")
    );

    let user_id_symbol = dog
        .columns
        .iter()
        .find(|c| c.name == "userId")
        .expect("userId column to be present")
        .symbol
        .clone();

    assert_eq!(dog.foreign_keys.len(), 1);
    let fk = &dog.foreign_keys[0];
    assert_eq!(fk.to_model, person.symbol);
    assert_eq!(fk.columns, vec![user_id_symbol]);
}

#[test]
fn model_block_composite_foreign_key() {
    let _ast = lex_and_parse(
        r#"
        [d1_a]
        model Parent {
            [primary orgId, userId]
            orgId: int
            userId: int
        }

        [d1_a]
        model Child {
            [primary id]
            id: int

            [foreign (orgId, userId) 
                -> (Parent::orgId, Parent::userId)]
            orgId: int
            userId: int

        }
        "#,
    );

    // let parent = get_model_by_name(&ast, "Parent");
    // let child = get_model_by_name(&ast, "Child");

    // let org_id_symbol = child
    //     .columns
    //     .iter()
    //     .find(|c| c.name == "orgId")
    //     .expect("orgId column to be present")
    //     .symbol
    //     .clone();
    // let user_id_symbol = child
    //     .columns
    //     .iter()
    //     .find(|c| c.name == "userId")
    //     .expect("userId column to be present")
    //     .symbol
    //     .clone();

    // assert_eq!(child.foreign_keys.len(), 1);
    // let fk = &child.foreign_keys[0];
    // assert_eq!(fk.to_model, parent.symbol);
    // assert_eq!(fk.columns, vec![org_id_symbol, user_id_symbol]);
}

#[test]
fn model_block_nav_one_to_one() {
    let ast = lex_and_parse(
        r#"
        [d1_a]
        model Bar {
            [primary id]
            id: int
        }

        [d1_a]
        model Foo {
            [primary id]
            id: int

            [foreign barId -> Bar::id]
            barId: int

            [nav bar -> Bar::id]
            bar: Bar
        }
        "#,
    );

    let bar = ast
        .models
        .iter()
        .find(|(_, m)| m.name == "Bar")
        .expect("Bar model to be present")
        .1;

    let foo = ast
        .models
        .iter()
        .find(|(_, m)| m.name == "Foo")
        .expect("Foo model to be present")
        .1;

    let nav_props: Vec<_> = foo.navigation_properties().collect();
    assert_eq!(nav_props.len(), 1);
    let (nav, key_fields) = &nav_props[0];
    assert_eq!(nav.to_model, bar.symbol);
    assert!(matches!(&nav.kind, NavigationPropertyKind::OneToOne { .. }));
    assert_eq!(
        key_fields
            .iter()
            .map(|f| f.name.as_str())
            .collect::<Vec<_>>(),
        vec!["id"]
    );
}
