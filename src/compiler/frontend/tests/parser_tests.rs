use ast::{CidlType, CrudKind, HttpVerb};
use compiler_test::lex_and_parse;

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
        .wrangler_envs
        .first()
        .expect("wrangler_env to be present");

    assert_eq!(
        env.d1_bindings
            .iter()
            .map(|b| b.name.as_str())
            .collect::<Vec<_>>(),
        vec!["db", "db2"]
    );

    assert_eq!(
        env.r2_bindings
            .iter()
            .map(|b| b.name.as_str())
            .collect::<Vec<_>>(),
        vec!["assets"]
    );

    assert_eq!(
        env.kv_bindings
            .iter()
            .map(|b| b.name.as_str())
            .collect::<Vec<_>>(),
        vec!["cache"]
    );

    assert_eq!(
        env.vars
            .iter()
            .map(|v| (v.name.as_str(), &v.cidl_type))
            .collect::<Vec<_>>(),
        vec![
            ("api_url", &CidlType::String),
            ("max_retries", &CidlType::Integer),
            ("threshold", &CidlType::Double),
            ("created_at", &CidlType::DateIso),
            ("payload", &CidlType::Json),
            ("enabled", &CidlType::Boolean)
        ]
    )
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

    let model = ast.models.first().expect("model to be present");
    assert_eq!(model.symbol.name, "Person");
    assert_eq!(
        model.d1_binding.as_ref().map(|b| b.env_binding.as_str()),
        Some("d1_a")
    );

    assert_eq!(
        model
            .primary_keys
            .iter()
            .map(|p| p.field.as_str())
            .collect::<Vec<_>>(),
        vec!["id", "age"]
    );

    assert_eq!(
        model
            .fields
            .iter()
            .map(|c| (c.name.as_str(), c.cidl_type.clone()))
            .collect::<Vec<_>>(),
        vec![
            ("age", CidlType::Integer),
            ("id", CidlType::Integer),
            ("name", CidlType::String),
            ("active", CidlType::Boolean),
            ("birthday", CidlType::DateIso),
            ("score", CidlType::Double)
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
        .find(|m| m.symbol.name == "Foo")
        .expect("Foo model to be present");

    assert_eq!(foo.kvs.len(), 1);
    assert_eq!(foo.r2s.len(), 1);
    assert_eq!(foo.key_fields.len(), 1);

    let key_param_names: Vec<&str> = foo.key_fields.iter().map(|k| k.field.as_str()).collect();
    assert_eq!(key_param_names, vec!["category"]);

    let kv = &foo.kvs[0];
    assert_eq!(kv.field, "kv_value");
    assert_eq!(kv.format, "my-interpolated-format{field}-{category}");

    let r2 = &foo.r2s[0];
    assert_eq!(r2.field, "obj");
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
        .find(|m| m.symbol.name == "Foo")
        .expect("Foo model to be present");

    let unique_constraints: Vec<Vec<&str>> = foo
        .unique_constraints
        .iter()
        .map(|constraint| constraint.fields.iter().map(|n| n.as_str()).collect())
        .collect();

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

    let dog = ast
        .models
        .iter()
        .find(|m| m.symbol.name == "Dog")
        .expect("Dog model to be present");

    assert_eq!(
        dog.d1_binding.as_ref().map(|b| b.env_binding.as_str()),
        Some("d1_a")
    );

    assert_eq!(dog.foreign_keys.len(), 1);
    let fk = &dog.foreign_keys[0];
    assert_eq!(fk.adj_model, "Person");
    assert_eq!(
        fk.references
            .iter()
            .map(|(src, _)| src.as_str())
            .collect::<Vec<_>>(),
        vec!["userId"]
    );
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

    let child = ast
        .models
        .iter()
        .find(|m| m.symbol.name == "Child")
        .expect("Child model to be present");

    assert_eq!(
        child.d1_binding.as_ref().map(|b| b.env_binding.as_str()),
        Some("d1_a")
    );

    assert_eq!(child.foreign_keys.len(), 1);
    let fk = &child.foreign_keys[0];
    assert_eq!(fk.adj_model, "Parent");
    assert_eq!(
        fk.references
            .iter()
            .map(|(src, _)| src.as_str())
            .collect::<Vec<_>>(),
        vec!["orgId", "userId"]
    );
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

    let foo = ast
        .models
        .iter()
        .find(|m| m.symbol.name == "Foo")
        .expect("Foo model to be present");

    assert_eq!(foo.navigation_properties.len(), 1);
    let nav = &foo.navigation_properties[0];
    assert_eq!(nav.field, "bar");
    assert!(!nav.is_many_to_many);
    assert_eq!(
        nav.fields
            .iter()
            .map(|(m, f)| (m.as_str(), f.as_str()))
            .collect::<Vec<_>>(),
        vec![("Bar", "id")]
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

    let foo = ast
        .models
        .iter()
        .find(|m| m.symbol.name == "Foo")
        .expect("Foo model to be present");

    assert_eq!(foo.navigation_properties.len(), 1);
    let nav = &foo.navigation_properties[0];
    assert_eq!(nav.field, "bars");
    assert!(!nav.is_many_to_many);
    assert_eq!(
        nav.fields
            .iter()
            .map(|(m, f)| (m.as_str(), f.as_str()))
            .collect::<Vec<_>>(),
        vec![("Bar", "fooId")]
    );
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
        .find(|m| m.symbol.name == "Student")
        .expect("Student model to be present");

    let course = ast
        .models
        .iter()
        .find(|m| m.symbol.name == "Course")
        .expect("Course model to be present");

    assert_eq!(student.navigation_properties.len(), 1);
    let student_nav = &student.navigation_properties[0];
    assert_eq!(student_nav.field, "courses");
    assert!(student_nav.is_many_to_many);
    assert_eq!(
        student_nav
            .fields
            .iter()
            .map(|(m, f)| (m.as_str(), f.as_str()))
            .collect::<Vec<_>>(),
        vec![("Course", "students")]
    );

    assert_eq!(course.navigation_properties.len(), 1);
    let course_nav = &course.navigation_properties[0];
    assert_eq!(course_nav.field, "students");
    assert!(course_nav.is_many_to_many);
    assert_eq!(
        course_nav
            .fields
            .iter()
            .map(|(m, f)| (m.as_str(), f.as_str()))
            .collect::<Vec<_>>(),
        vec![("Student", "courses")]
    );
}

#[test]
fn model_block_nav_implicit_model() {
    let ast = lex_and_parse(
        r#"
        @d1(d1_a)
        model Bar {
            [primary id]
            id: int
            col: int

            [nav foo -> col]
            foo: Bar
        }
        "#,
    );

    let bar = ast
        .models
        .iter()
        .find(|m| m.symbol.name == "Bar")
        .expect("Bar model to be present");

    assert_eq!(bar.navigation_properties.len(), 1);
    let nav = &bar.navigation_properties[0];
    assert_eq!(nav.field, "foo");
    assert!(!nav.is_many_to_many);
    assert_eq!(
        nav.fields
            .iter()
            .map(|(m, f)| (m.as_str(), f.as_str()))
            .collect::<Vec<_>>(),
        vec![("Bar", "col")]
    );
}

#[test]
fn model_block_nav_mixed_refs() {
    let ast = lex_and_parse(
        r#"
        @d1(d1_a)
        model Foo {
            [primary id]
            id: int
            col: int
        }

        @d1(d1_a)
        model Bar {
            [primary id]
            id: int
            localCol: int

            [nav baz -> (Foo::col, localCol)]
            baz: Foo
        }
        "#,
    );

    let bar = ast
        .models
        .iter()
        .find(|m| m.symbol.name == "Bar")
        .expect("Bar model to be present");

    assert_eq!(bar.navigation_properties.len(), 1);
    let nav = &bar.navigation_properties[0];
    assert_eq!(nav.field, "baz");
    assert!(!nav.is_many_to_many);
    assert_eq!(
        nav.fields
            .iter()
            .map(|(m, f)| (m.as_str(), f.as_str()))
            .collect::<Vec<_>>(),
        vec![("Foo", "col"), ("Bar", "localCol")]
    );
}

#[test]
fn api_block() {
    let ast = lex_and_parse(
        r#"
        @crud(get, save, list)
        model User {
            [primary id]
            id: int
            name: string
        }

        api User {
            post someMethod(
                @source(mySource)
                self,
                id: Option<int>
            ) -> double

            get anotherMethod() -> void
        }

        api User {
            get getById(id: int, e: env) -> void
        }

        api User {
            post update(self) -> void
        }
        "#,
    );

    // Verify cruds are on the model
    let user_model = ast
        .models
        .iter()
        .find(|m| m.symbol.name == "User")
        .expect("User model to be present");
    assert_eq!(
        user_model.cruds,
        vec![CrudKind::Get, CrudKind::Save, CrudKind::List]
    );

    // ParseAst keeps api blocks separate
    let user_api_blocks: Vec<_> = ast.apis.iter().filter(|a| a.namespace == "User").collect();
    assert_eq!(user_api_blocks.len(), 3);

    // First block: someMethod + anotherMethod
    let first = user_api_blocks
        .iter()
        .find(|a| a.methods.iter().any(|m| m.symbol.name == "someMethod"))
        .expect("block with someMethod");

    let some_method = first
        .methods
        .iter()
        .find(|m| m.symbol.name == "someMethod")
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
    assert_eq!(some_method.return_type, CidlType::Double);

    let another_method = first
        .methods
        .iter()
        .find(|m| m.symbol.name == "anotherMethod")
        .expect("anotherMethod to be present");
    assert_eq!(another_method.http_verb, HttpVerb::Get);
    assert!(another_method.is_static);
    assert!(another_method.data_source.is_none());
    assert_eq!(another_method.parameters.len(), 0);
    assert_eq!(another_method.return_type, CidlType::Void);

    // Second block: getById
    let second = user_api_blocks
        .iter()
        .find(|a| a.methods.iter().any(|m| m.symbol.name == "getById"))
        .expect("block with getById");
    assert_eq!(second.methods.len(), 1);
    let get_by_id = &second.methods[0];
    assert_eq!(get_by_id.parameters.len(), 2);
    assert_eq!(get_by_id.parameters[0].cidl_type, CidlType::Integer);
    assert_eq!(get_by_id.parameters[1].name, "e");
    assert_eq!(get_by_id.parameters[1].cidl_type, CidlType::Env);

    // Third block: update
    let third = user_api_blocks
        .iter()
        .find(|a| a.methods.iter().any(|m| m.symbol.name == "update"))
        .expect("block with update");
    assert_eq!(third.methods.len(), 1);
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

    let user_source = ast
        .sources
        .iter()
        .find(|s| s.symbol.name == "UserSource")
        .expect("UserSource to be present");
    assert_eq!(user_source.model, "User");

    let tree = &user_source.tree;
    assert!(tree.0.contains_key("id"));
    assert!(tree.0.contains_key("name"));
    assert!(tree.0.contains_key("address"));

    let address_subtree = &tree.0["address"];
    assert!(address_subtree.0.contains_key("street"));
    assert!(address_subtree.0.contains_key("city"));

    let id_subtree = &tree.0["id"];
    assert!(id_subtree.0.is_empty());

    let get = user_source.get.as_ref().expect("get method to be present");
    assert_eq!(get.raw_sql, "SELECT * FROM users WHERE id = ?");
    assert_eq!(get.parameters.len(), 1);
    assert_eq!(get.parameters[0].name, "id");
    assert_eq!(get.parameters[0].cidl_type, CidlType::Integer);

    let list = user_source
        .list
        .as_ref()
        .expect("list method to be present");
    assert_eq!(list.raw_sql, "SELECT * FROM users LIMIT ? OFFSET ?");
    assert_eq!(list.parameters.len(), 2);
    assert_eq!(list.parameters[0].name, "offset");
    assert_eq!(list.parameters[0].cidl_type, CidlType::Integer);
    assert_eq!(list.parameters[1].name, "limit");
    assert_eq!(list.parameters[1].cidl_type, CidlType::Integer);

    let minimal = ast
        .sources
        .iter()
        .find(|s| s.symbol.name == "MinimalSource")
        .expect("MinimalSource to be present");
    assert_eq!(minimal.model, "Post");
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

    let address_poo = ast
        .poos
        .iter()
        .find(|p| p.symbol.name == "Address")
        .expect("Address poo to be present");
    assert_eq!(address_poo.fields.len(), 3);

    let zipcode = address_poo
        .fields
        .iter()
        .find(|f| f.name == "zipcode")
        .expect("zipcode field to be present");
    assert_eq!(zipcode.cidl_type, CidlType::nullable(CidlType::String));

    let user_poo = ast
        .poos
        .iter()
        .find(|p| p.symbol.name == "User")
        .expect("User poo to be present");
    assert_eq!(user_poo.fields.len(), 12);

    assert_eq!(
        user_poo
            .fields
            .iter()
            .map(|f| (f.name.as_str(), f.cidl_type.clone()))
            .collect::<Vec<_>>(),
        vec![
            ("id", CidlType::Integer),
            ("name", CidlType::String),
            ("email", CidlType::String),
            ("age", CidlType::nullable(CidlType::Integer)),
            ("active", CidlType::Boolean),
            ("balance", CidlType::Double),
            ("created", CidlType::DateIso),
            (
                "address",
                CidlType::UnresolvedReference {
                    name: "Address".to_string(),
                }
            ),
            ("tags", CidlType::array(CidlType::String)),
            ("metadata", CidlType::nullable(CidlType::Json)),
            (
                "optional_items",
                CidlType::nullable(CidlType::array(CidlType::UnresolvedReference {
                    name: "Item".to_string(),
                }))
            ),
            (
                "nullable_arrays",
                CidlType::array(CidlType::nullable(CidlType::String))
            )
        ]
    );

    let container_poo = ast
        .poos
        .iter()
        .find(|p| p.symbol.name == "Container")
        .expect("Container poo to be present");
    assert_eq!(container_poo.fields.len(), 2);
    assert_eq!(
        container_poo
            .fields
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

    assert_eq!(
        ast.injects
            .iter()
            .flat_map(|s| s.fields.iter())
            .map(|r| r.name.as_str())
            .collect::<Vec<_>>(),
        vec!["OpenApiService", "YouTubeApi", "SlackApi"]
    );
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
            ) -> string
        }

        api MyAppService {
            get listItems(self) -> Array<string>
        }
        "#,
    );

    assert_eq!(ast.services.len(), 2);

    let service = ast
        .services
        .iter()
        .find(|s| s.symbol.name == "MyAppService")
        .expect("MyAppService service to be present");

    assert_eq!(service.fields.len(), 2);

    let api1 = service
        .fields
        .iter()
        .find(|f| f.name == "api1")
        .expect("api1 field");
    assert_eq!(
        api1.cidl_type,
        CidlType::UnresolvedReference {
            name: "OpenApiService".to_string(),
        }
    );

    let api2 = service
        .fields
        .iter()
        .find(|f| f.name == "api2")
        .expect("api2 field");
    assert_eq!(
        api2.cidl_type,
        CidlType::UnresolvedReference {
            name: "YouTubeApi".to_string(),
        }
    );

    // Fields have distinct spans
    assert_ne!(api1.span, api2.span);

    let empty = ast
        .services
        .iter()
        .find(|s| s.symbol.name == "EmptyService")
        .expect("EmptyService to be present");
    assert_eq!(empty.fields.len(), 0);

    // Services have distinct name spans
    assert_ne!(service.symbol.span, empty.symbol.span);

    // Two api blocks for MyAppService kept separate
    let app_api_blocks: Vec<_> = ast
        .apis
        .iter()
        .filter(|a| a.namespace == "MyAppService")
        .collect();
    assert_eq!(app_api_blocks.len(), 2);

    let create_block = app_api_blocks
        .iter()
        .find(|a| a.methods.iter().any(|m| m.symbol.name == "createItem"))
        .expect("block with createItem");
    let create = create_block
        .methods
        .iter()
        .find(|m| m.symbol.name == "createItem")
        .unwrap();
    assert_eq!(create.http_verb, HttpVerb::Post);
    assert!(create.is_static);
    assert_eq!(create.parameters.len(), 2);
    assert_eq!(create.return_type, CidlType::String);

    let list_block = app_api_blocks
        .iter()
        .find(|a| a.methods.iter().any(|m| m.symbol.name == "listItems"))
        .expect("block with listItems");
    let list = list_block
        .methods
        .iter()
        .find(|m| m.symbol.name == "listItems")
        .unwrap();
    assert_eq!(list.http_verb, HttpVerb::Get);
    assert!(!list.is_static);
    assert_eq!(list.return_type, CidlType::array(CidlType::String));
}
