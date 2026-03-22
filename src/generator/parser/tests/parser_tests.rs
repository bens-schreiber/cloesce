use ast::{CidlType, CloesceAst, CrudKind, D1NavigationPropertyKind, HttpVerb};
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
        CidlType::Json
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
    assert_eq!(fk.adj_model, person.symbol);
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
    assert_eq!(fk.adj_model, parent.symbol);
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
    assert_eq!(nav.adj_model, bar.symbol);
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
    assert_eq!(nav.adj_model, bar.symbol);
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
    assert_eq!(student_nav.adj_model, course.symbol);
    assert!(matches!(
        &student_nav.kind,
        D1NavigationPropertyKind::ManyToMany { .. }
    ));

    let course_nav_props: Vec<_> = course.navigation_properties().collect();
    assert_eq!(course_nav_props.len(), 1);
    let (course_nav, _) = &course_nav_props[0];
    assert_eq!(course_nav.adj_model, student.symbol);
    assert!(matches!(
        &course_nav.kind,
        D1NavigationPropertyKind::ManyToMany { .. }
    ));
}

#[test]
fn api_block() {
    let ast = lex_and_parse(
        r#"
        model User {
            [primary id]
            id: int
            name: string
        }

        @crud(get, save, list)
        api User {
            post someMethod(
                @source(mySource)
                self,
                id: Option<int>
            ) -> Result<double>

            get anotherMethod() -> void
        }

        @crud(get)
        api User {
            get getById(id: int) -> void
        }

        @crud(save, list)
        api User {
            post update(self) -> void
        }
        "#,
    );

    // apis should be keyed by api::<model_name>, not by model symbol
    let (user_symbol, _) = ast
        .models
        .iter()
        .find(|(_, model)| model.name == "User")
        .expect("User model to be present");
    assert!(ast.apis.get(user_symbol).is_none());

    // multiple api blocks for the same model are merged into one
    assert_eq!(ast.apis.len(), 1);

    let api = ast
        .apis
        .values()
        .next()
        .expect("User api to be present");

    assert_eq!(
        api.cruds,
        vec![CrudKind::GET, CrudKind::SAVE, CrudKind::LIST]
    );
    assert_eq!(api.methods.len(), 4);

    let some_method = api
        .methods
        .iter()
        .find(|method| method.name == "someMethod")
        .expect("someMethod to be present");
    assert_eq!(some_method.http_verb, HttpVerb::Post);
    assert!(!some_method.is_static);
    assert!(some_method.data_source.is_some());
    assert_eq!(some_method.parameters.len(), 1);
    assert_eq!(some_method.parameters[0].name, "id");
    assert_eq!(
        some_method.parameters[0].cidl_type,
        CidlType::nullable(CidlType::Integer)
    );
    assert_eq!(some_method.return_type, CidlType::http(CidlType::Double));

    let another_method = api
        .methods
        .iter()
        .find(|method| method.name == "anotherMethod")
        .expect("anotherMethod to be present");
    assert_eq!(another_method.http_verb, HttpVerb::Get);
    assert!(another_method.is_static);
    assert!(another_method.data_source.is_none());
    assert_eq!(another_method.parameters.len(), 0);
    assert_eq!(another_method.return_type, CidlType::Void);

    assert!(api.methods.iter().any(|m| m.name == "getById"));
    assert!(api.methods.iter().any(|m| m.name == "update"));
}

#[test]
fn data_source_block() {
    let ast = lex_and_parse(
        r#"
        source UserSource for User {
            include { id, name, address { street, city } }

            sql get(id: int) {
                "SELECT * FROM users WHERE id = ?"
            }

            sql list(offset: int, limit: int) {
                "SELECT * FROM users LIMIT ? OFFSET ?"
            }
        }

        source MinimalSource for Post {
            include { id, title }
        }
        "#,
    );

    assert_eq!(ast.sources.len(), 2);

    let user_sources = ast
        .sources
        .values()
        .find(|s| s[0].name == "UserSource")
        .expect("UserSource entry");
    assert_eq!(user_sources.len(), 1);

    let source = &user_sources[0];
    assert!(!source.is_private);

    // include tree: { id, name, address { street, city } }
    let tree = &source.tree;
    assert!(tree.0.contains_key("id"));
    assert!(tree.0.contains_key("name"));
    assert!(tree.0.contains_key("address"));

    let address_subtree = &tree.0["address"];
    assert!(address_subtree.0.contains_key("street"));
    assert!(address_subtree.0.contains_key("city"));

    let id_subtree = &tree.0["id"];
    assert!(id_subtree.0.is_empty());

    // get method
    let get = source.get.as_ref().expect("get method to be present");
    assert_eq!(get.raw_sql, "\"SELECT * FROM users WHERE id = ?\"");
    assert_eq!(get.parameters.len(), 1);
    assert_eq!(get.parameters[0].name, "id");
    assert_eq!(get.parameters[0].cidl_type, CidlType::Integer);

    // list method
    let list = source.list.as_ref().expect("list method to be present");
    assert_eq!(list.raw_sql, "\"SELECT * FROM users LIMIT ? OFFSET ?\"");
    assert_eq!(list.parameters.len(), 2);
    assert_eq!(list.parameters[0].name, "offset");
    assert_eq!(list.parameters[0].cidl_type, CidlType::Integer);
    assert_eq!(list.parameters[1].name, "limit");
    assert_eq!(list.parameters[1].cidl_type, CidlType::Integer);

    // MinimalSource — optional sql methods absent
    let minimal_sources = ast
        .sources
        .values()
        .find(|s| s[0].name == "MinimalSource")
        .expect("MinimalSource entry");
    let minimal = &minimal_sources[0];
    assert!(minimal.get.is_none());
    assert!(minimal.list.is_none());

    let minimal_tree = &minimal.tree;
    assert!(minimal_tree.0.contains_key("id"));
    assert!(minimal_tree.0.contains_key("title"));
}

#[test]
fn poo_block() {
    let ast = lex_and_parse(
        r#"
        poo Address {
            street: string
            city: string
            zipcode: Option<string>
        }

        poo User {
            id: int
            name: string
            email: string
            age: Option<int>
            active: bool
            balance: double
            created: date
            address: Address
            tags: Array<string>
            metadata: Option<json>
            optional_items: Option<Array<Item>>
            nullable_arrays: Array<Option<string>>
        }

        poo Container {
            items: Array<Item>
            nested: Array<Array<int>>
        }
        "#,
    );

    assert_eq!(ast.poos.len(), 3);

    // Test Address POO
    let address_poo = ast
        .poos
        .values()
        .find(|p| p.name == "Address")
        .expect("Address poo to be present");
    assert_eq!(address_poo.attributes.len(), 3);
    assert_ne!(address_poo.symbol.0, 0);

    let zipcode = address_poo
        .attributes
        .iter()
        .find(|f| f.name == "zipcode")
        .expect("zipcode field to be present");
    assert_eq!(zipcode.cidl_type, CidlType::nullable(CidlType::String));

    // Test User POO - comprehensive type coverage
    let user_poo = ast
        .poos
        .values()
        .find(|p| p.name == "User")
        .expect("User poo to be present");
    assert_eq!(user_poo.attributes.len(), 12);
    assert_ne!(user_poo.symbol.0, 0);

    let field_names: Vec<&str> = user_poo
        .attributes
        .iter()
        .map(|f| f.name.as_str())
        .collect();
    assert_eq!(
        field_names,
        vec![
            "id",
            "name",
            "email",
            "age",
            "active",
            "balance",
            "created",
            "address",
            "tags",
            "metadata",
            "optional_items",
            "nullable_arrays"
        ]
    );

    // Test primitive types
    assert_eq!(
        user_poo
            .attributes
            .iter()
            .find(|f| f.name == "id")
            .unwrap()
            .cidl_type,
        CidlType::Integer
    );
    assert_eq!(
        user_poo
            .attributes
            .iter()
            .find(|f| f.name == "name")
            .unwrap()
            .cidl_type,
        CidlType::String
    );
    assert_eq!(
        user_poo
            .attributes
            .iter()
            .find(|f| f.name == "active")
            .unwrap()
            .cidl_type,
        CidlType::Boolean
    );
    assert_eq!(
        user_poo
            .attributes
            .iter()
            .find(|f| f.name == "balance")
            .unwrap()
            .cidl_type,
        CidlType::Double
    );
    assert_eq!(
        user_poo
            .attributes
            .iter()
            .find(|f| f.name == "created")
            .unwrap()
            .cidl_type,
        CidlType::DateIso
    );

    // Test nullable types
    assert_eq!(
        user_poo
            .attributes
            .iter()
            .find(|f| f.name == "age")
            .unwrap()
            .cidl_type,
        CidlType::nullable(CidlType::Integer)
    );
    assert_eq!(
        user_poo
            .attributes
            .iter()
            .find(|f| f.name == "metadata")
            .unwrap()
            .cidl_type,
        CidlType::nullable(CidlType::Json)
    );

    // Test object reference
    assert_eq!(
        user_poo
            .attributes
            .iter()
            .find(|f| f.name == "address")
            .unwrap()
            .cidl_type,
        CidlType::Object("Address".into())
    );

    // Test array types
    assert_eq!(
        user_poo
            .attributes
            .iter()
            .find(|f| f.name == "tags")
            .unwrap()
            .cidl_type,
        CidlType::array(CidlType::String)
    );
    assert_eq!(
        user_poo
            .attributes
            .iter()
            .find(|f| f.name == "optional_items")
            .unwrap()
            .cidl_type,
        CidlType::nullable(CidlType::array(CidlType::Object("Item".into())))
    );
    assert_eq!(
        user_poo
            .attributes
            .iter()
            .find(|f| f.name == "nullable_arrays")
            .unwrap()
            .cidl_type,
        CidlType::array(CidlType::nullable(CidlType::String))
    );

    // Test Container POO - nested arrays
    let container_poo = ast
        .poos
        .values()
        .find(|p| p.name == "Container")
        .expect("Container poo to be present");
    assert_eq!(container_poo.attributes.len(), 2);
    assert_ne!(container_poo.symbol.0, 0);

    assert_eq!(
        container_poo
            .attributes
            .iter()
            .find(|f| f.name == "nested")
            .unwrap()
            .cidl_type,
        CidlType::array(CidlType::array(CidlType::Integer))
    );
}

#[test]
fn inject_block() {
    let ast = lex_and_parse(
        r#"
        inject {
            OpenApiService
        }

        inject {
            YouTubeApi
            SlackApi
        }
        "#,
    );

    assert_eq!(ast.injectables.len(), 3);
    assert!(ast.injectables.iter().all(|s| s.0 != 0));
}

#[test]
fn service_block() {
    let ast = lex_and_parse(
        r#"
        service MyAppService {
            api1: OpenApiService
            api2: YouTubeApi
        }

        service EmptyService {}

        api MyAppService {
            post createItem(
                name: string,
                count: int
            ) -> Result<string>
        }

        api MyAppService {
            get listItems(self) -> Array<string>
        }
        "#,
    );

    // Two services parsed
    assert_eq!(ast.services.len(), 2);

    let service = ast
        .services
        .values()
        .find(|s| s.name == "MyAppService")
        .expect("MyAppService service to be present");

    // Symbol is non-zero
    assert_ne!(service.symbol.0, 0);

    // Two injected attributes
    assert_eq!(service.attributes.len(), 2);

    let api1 = service
        .attributes
        .iter()
        .find(|a| a.var_name == "api1")
        .expect("api1 attribute");
    assert_eq!(api1.inject_reference, "OpenApiService");
    assert_ne!(api1.symbol.0, 0);

    let api2 = service
        .attributes
        .iter()
        .find(|a| a.var_name == "api2")
        .expect("api2 attribute");
    assert_eq!(api2.inject_reference, "YouTubeApi");
    assert_ne!(api2.symbol.0, 0);

    // Attribute symbols are distinct
    assert_ne!(api1.symbol, api2.symbol);

    // Empty service
    let empty = ast
        .services
        .values()
        .find(|s| s.name == "EmptyService")
        .expect("EmptyService to be present");
    assert_eq!(empty.attributes.len(), 0);
    assert_ne!(empty.symbol.0, 0);

    // Service symbols are distinct
    assert_ne!(service.symbol, empty.symbol);

    // Two api blocks for MyAppService are merged into one Api entry
    assert_eq!(ast.apis.len(), 1);
    let api = ast
        .apis
        .values()
        .find(|a| a.model_symbol == service.symbol)
        .expect("Api for MyAppService to be present");

    // Both api blocks merged: createItem + listItems
    assert_eq!(api.methods.len(), 2);
    assert!(api.methods.iter().any(|m| m.name == "createItem"));
    assert!(api.methods.iter().any(|m| m.name == "listItems"));

    let create = api
        .methods
        .iter()
        .find(|m| m.name == "createItem")
        .unwrap();
    assert_eq!(create.http_verb, HttpVerb::Post);
    assert!(create.is_static);
    assert_eq!(create.parameters.len(), 2);
    assert_eq!(
        create.return_type,
        CidlType::http(CidlType::String)
    );

    let list = api.methods.iter().find(|m| m.name == "listItems").unwrap();
    assert_eq!(list.http_verb, HttpVerb::Get);
    assert!(!list.is_static);
    assert_eq!(list.return_type, CidlType::array(CidlType::String));
}
