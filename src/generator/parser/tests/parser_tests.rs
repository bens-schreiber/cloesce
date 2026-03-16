use ast::{CidlType, CloesceAst, D1NavigationPropertyKind};
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
        @d1(d1_a)
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
fn model_block_kv_r2_anchored_tags() {
    let ast = lex_and_parse(
        r#"
        env {
            cache_ns: kv
            assets_bucket: r2
        }

        model Foo {
            field: string

            @keyparam
            category: string

            @kv(cache_ns, "my-interpolated-format{field}-{category}")
            kv_value: anyTypeATAll

            @r2(assets_bucket, "my_interpolated_format{field}")
            obj: R2Object
        }
        "#,
    );

    let foo = ast
        .models
        .iter()
        .find(|(_, m)| m.name == "Foo")
        .expect("Foo model to be present")
        .1;

    assert_eq!(foo.kv_objects().count(), 1);
    assert_eq!(foo.r2_objects().count(), 1);
    assert_eq!(foo.key_params().count(), 1);

    let key_param_names = foo
        .key_params()
        .map(|field| field.name.as_str())
        .collect::<Vec<_>>();

    assert_eq!(key_param_names, vec!["category"]);

    let kv = &foo.kv_navigation_properties[0];
    assert_eq!(kv.field.name, "kv_value");
    assert_eq!(kv.format, "my-interpolated-format{field}-{category}");

    let r2 = &foo.r2_navigation_properties[0];
    assert_eq!(r2.name, "obj");
    assert_eq!(r2.format, "my_interpolated_format{field}");
}

#[test]
fn model_block_unique_constraints() {
    let ast = lex_and_parse(
        r#"
        @d1(d1_a)
        model Foo {
            [primary id]
            [unique a, b, c]
            [unique email]

            id: int
            a: int
            b: int
            c: int
            email: string
        }
        "#,
    );

    let foo = ast
        .models
        .iter()
        .find(|(_, m)| m.name == "Foo")
        .expect("Foo model to be present")
        .1;

    let unique_constraints = foo
        .unique_constraints
        .iter()
        .map(|constraint| {
            constraint
                .iter()
                .map(|symbol| {
                    foo.columns
                        .iter()
                        .find(|field| field.symbol == *symbol)
                        .expect("unique constraint symbol to resolve to a model column")
                        .name
                        .clone()
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();

    assert_eq!(unique_constraints, vec![vec!["a", "b", "c"], vec!["email"]]);
}

#[test]
fn model_block_single_foreign_key() {
    let ast = lex_and_parse(
        r#"
        @d1(d1_a)
        model Person {
            [primary id]
            id: int
        }

        @d1(d1_a)
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
    let ast = lex_and_parse(
        r#"
        @d1(d1_a)
        model Parent {
            [primary orgId, userId]
            orgId: int
            userId: int
        }

        @d1(d1_a)
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

    let parent = ast
        .models
        .iter()
        .find(|(_, m)| m.name == "Parent")
        .expect("Parent model to be present")
        .1;

    let child = ast
        .models
        .iter()
        .find(|(_, m)| m.name == "Child")
        .expect("Child model to be present")
        .1;

    assert_eq!(
        child.d1_binding.as_ref().map(|b| b.name.as_str()),
        Some("d1_a")
    );

    let org_id_symbol = child
        .columns
        .iter()
        .find(|c| c.name == "orgId")
        .expect("orgId column to be present")
        .symbol
        .clone();

    let user_id_symbol = child
        .columns
        .iter()
        .find(|c| c.name == "userId")
        .expect("userId column to be present")
        .symbol
        .clone();

    assert_eq!(child.foreign_keys.len(), 1);
    let fk = &child.foreign_keys[0];
    assert_eq!(fk.to_model, parent.symbol);
    assert_eq!(fk.columns, vec![org_id_symbol, user_id_symbol]);
}

#[test]
fn model_block_nav_one_to_one() {
    let ast = lex_and_parse(
        r#"
        @d1(d1_a)
        model Bar {
            [primary id]
            id: int
        }

        @d1(d1_a)
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
    assert!(matches!(
        &nav.kind,
        D1NavigationPropertyKind::OneToOne { .. }
    ));
    assert_eq!(
        key_fields
            .iter()
            .map(|f| f.name.as_str())
            .collect::<Vec<_>>(),
        vec!["id"]
    );
}

#[test]
fn model_block_nav_one_to_many() {
    let ast = lex_and_parse(
        r#"
        @d1(d1_a)
        model Foo {
            [primary id]
            id: int

            [nav bars -> Bar::fooId]
            bars: Array<Bar>
        }

        @d1(d1_a)
        model Bar {
            [primary id]
            id: int

            [foreign fooId -> Foo::id]
            fooId: int
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

    let (nav, _) = &nav_props[0];
    assert_eq!(nav.to_model, bar.symbol);
    assert!(matches!(
        &nav.kind,
        D1NavigationPropertyKind::OneToMany { .. }
    ));
}

#[test]
fn model_block_nav_many_to_many() {
    let ast = lex_and_parse(
        r#"
        @d1(d1_a)
        model Student {
            [primary id]
            id: int

            [nav courses <> Course::students]
            courses: Array<Course>
        }

        @d1(d1_a)
        model Course {
            [primary id]
            id: int

            [nav students <> Student::courses]
            students: Array<Student>
        }
        "#,
    );

    let student = ast
        .models
        .iter()
        .find(|(_, m)| m.name == "Student")
        .expect("Student model to be present")
        .1;

    let course = ast
        .models
        .iter()
        .find(|(_, m)| m.name == "Course")
        .expect("Course model to be present")
        .1;

    let student_nav_props: Vec<_> = student.navigation_properties().collect();
    assert_eq!(student_nav_props.len(), 1);
    let (student_nav, _) = &student_nav_props[0];
    assert_eq!(student_nav.to_model, course.symbol);
    assert!(matches!(
        &student_nav.kind,
        D1NavigationPropertyKind::ManyToMany { .. }
    ));

    let course_nav_props: Vec<_> = course.navigation_properties().collect();
    assert_eq!(course_nav_props.len(), 1);
    let (course_nav, _) = &course_nav_props[0];
    assert_eq!(course_nav.to_model, student.symbol);
    assert!(matches!(
        &course_nav.kind,
        D1NavigationPropertyKind::ManyToMany { .. }
    ));
}
