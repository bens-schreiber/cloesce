use ast::{CidlType, CrudKind, HttpVerb};
use compiler_test::lex_and_parse;

#[test]
fn env_block() {
    // Act
    let ast = lex_and_parse(
        r#"

        env {
            d1 { db db2 }
            r2 { assets }
            kv { cache }

            vars {
                api_url: string
                max_retries: int
                threshold: double
                created_at: date
                payload: json
                enabled: bool
            }
        }
        "#,
    );

    // Assert
    let env = ast
        .wrangler_envs
        .first()
        .expect("wrangler_env to be present");

    assert_eq!(
        env.d1_bindings.iter().map(|b| b.name).collect::<Vec<_>>(),
        vec!["db", "db2"]
    );

    assert_eq!(
        env.r2_bindings.iter().map(|b| b.name).collect::<Vec<_>>(),
        vec!["assets"]
    );

    assert_eq!(
        env.kv_bindings.iter().map(|b| b.name).collect::<Vec<_>>(),
        vec!["cache"]
    );

    assert_eq!(
        env.vars
            .iter()
            .map(|v| (v.name, &v.cidl_type))
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
fn poo_block() {
    // Act
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

    // Assert
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
            .map(|f| (f.name, f.cidl_type.clone()))
            .collect::<Vec<_>>(),
        vec![
            ("id", CidlType::Integer),
            ("name", CidlType::String),
            ("email", CidlType::String),
            ("age", CidlType::nullable(CidlType::Integer)),
            ("active", CidlType::Boolean),
            ("balance", CidlType::Double),
            ("created", CidlType::DateIso),
            ("address", CidlType::UnresolvedReference { name: "Address" }),
            ("tags", CidlType::array(CidlType::String)),
            ("metadata", CidlType::nullable(CidlType::Json)),
            (
                "optional_items",
                CidlType::nullable(CidlType::array(CidlType::UnresolvedReference {
                    name: "Item",
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
    // Act
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

    // Assert
    assert_eq!(
        ast.injects
            .iter()
            .flat_map(|s| s.fields.iter())
            .map(|r| r.name)
            .collect::<Vec<_>>(),
        vec!["OpenApiService", "YouTubeApi", "SlackApi"]
    );
}

#[test]
fn service_block() {
    // Act
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

    // Assert
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
            name: "OpenApiService",
        }
    );

    let api2 = service
        .fields
        .iter()
        .find(|f| f.name == "api2")
        .expect("api2 field");
    assert_eq!(
        api2.cidl_type,
        CidlType::UnresolvedReference { name: "YouTubeApi" }
    );
    assert_ne!(api1.span, api2.span, "fields should have distinct spans");

    let empty = ast
        .services
        .iter()
        .find(|s| s.symbol.name == "EmptyService")
        .expect("EmptyService to be present");
    assert_eq!(empty.fields.len(), 0);
    assert_ne!(
        service.symbol.span, empty.symbol.span,
        "services should have distinct spans"
    );

    let app_api_blocks: Vec<_> = ast
        .apis
        .iter()
        .filter(|a| a.namespace == "MyAppService")
        .collect();
    assert_eq!(
        app_api_blocks.len(),
        2,
        "should have two separate api blocks for MyAppService"
    );

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

#[test]
fn model_block_scalar() {
    // Act
    let ast = lex_and_parse(
        r#"
        [use d1_a]
        model Person {
            // Composite PK: id and age
            primary {
                id: int
                age: int
            }

            name: string
            active: bool
            birthday: date
            score: double
        }
        "#,
    );

    // Assert
    let model = ast.models.first().expect("model to be present");
    assert_eq!(model.symbol.name, "Person");
    assert_eq!(model.use_tag.as_ref().unwrap().env_bindings, vec!["d1_a"]);
    assert_eq!(model.primary_fields, vec!["id", "age"]);

    let all_names: Vec<&str> = model.typed_idents.iter().map(|f| f.name).collect();
    assert!(all_names.contains(&"id")); // should include primary fields
    assert!(all_names.contains(&"age"));
    assert!(all_names.contains(&"name"));
    assert!(all_names.contains(&"active"));
    assert!(all_names.contains(&"birthday"));
    assert!(all_names.contains(&"score"));
}

#[test]
fn model_block_kv_r2() {
    // Act
    let ast = lex_and_parse(
        r#"
        model Foo {
            field: string

            kv(cache_ns, "my-interpolated-format{field}-{category}") {
                kv_value: json
            }

            r2(assets_bucket, "my_interpolated_format{field}") {
                obj
            }
        }
        "#,
    );

    // Assert
    let foo = ast
        .models
        .iter()
        .find(|m| m.symbol.name == "Foo")
        .expect("Foo model to be present");

    assert_eq!(foo.kvs.len(), 1);
    assert_eq!(foo.r2s.len(), 1);

    let kv = &foo.kvs[0];
    assert_eq!(kv.env_binding, "cache_ns");
    assert_eq!(kv.key_format, "my-interpolated-format{field}-{category}");
    assert_eq!(kv.field.name, "kv_value");

    let r2 = &foo.r2s[0];
    assert_eq!(r2.env_binding, "assets_bucket");
    assert_eq!(r2.key_format, "my_interpolated_format{field}");
    assert_eq!(r2.field.name, "obj");
}

#[test]
fn model_block_unique_constraints() {
    // Act
    let ast = lex_and_parse(
        r#"
        [use d1_a]
        model Foo {
            primary {
                id: int
            }

            unique {
                a: int
                b: int
                c: int
            }

            unique {
                email: string
            }
        }
        "#,
    );

    // Assert
    let foo = ast
        .models
        .iter()
        .find(|m| m.symbol.name == "Foo")
        .expect("Foo model to be present");

    let unique_constraints: Vec<Vec<&str>> = foo
        .unique_constraints
        .iter()
        .map(|constraint| constraint.fields.to_vec())
        .collect();

    assert_eq!(unique_constraints, vec![vec!["a", "b", "c"], vec!["email"]]);
}

#[test]
fn model_block_single_foreign_key() {
    // Act
    let ast = lex_and_parse(
        r#"
        [use d1_a]
        model Person {
            primary {
                id: int
            }
        }

        [use d1_a]
        model Dog {
            primary {
                id: int
            }

            foreign(Person::id) {
                userId
            }

            userId: int
        }
        "#,
    );

    // Assert
    let dog = ast
        .models
        .iter()
        .find(|m| m.symbol.name == "Dog")
        .expect("Dog model to be present");

    assert_eq!(dog.use_tag.as_ref().unwrap().env_bindings, vec!["d1_a"]);

    assert_eq!(dog.foreign_blocks.len(), 1);
    let fb = &dog.foreign_blocks[0];
    assert_eq!(fb.adj, vec![("Person", "id")]);
    assert_eq!(
        fb.fields.iter().map(|s| s.name).collect::<Vec<_>>(),
        vec!["userId"]
    );
}

#[test]
fn model_block_composite_foreign_key() {
    // Act
    let ast = lex_and_parse(
        r#"
        [use d1_a]
        model Parent {
            primary {
                orgId: int
                userId: int
            }
        }

        [use d1_a]
        model Child {
            primary {
                id: int
            }

            foreign(Parent::orgId, Parent::userId) {
                orgId
                userId
            }

            orgId: int
            userId: int
        }
        "#,
    );

    // Assert
    let child = ast
        .models
        .iter()
        .find(|m| m.symbol.name == "Child")
        .expect("Child model to be present");

    assert_eq!(child.use_tag.as_ref().unwrap().env_bindings, vec!["d1_a"]);

    assert_eq!(child.foreign_blocks.len(), 1);
    let fb = &child.foreign_blocks[0];
    assert_eq!(fb.adj, vec![("Parent", "orgId"), ("Parent", "userId")]);
    assert_eq!(
        fb.fields.iter().map(|s| s.name).collect::<Vec<_>>(),
        vec!["orgId", "userId"]
    );
}

#[test]
fn model_block_foreign_with_nav() {
    // Act
    let ast = lex_and_parse(
        r#"
        [use d1_a]
        model Bar {
            primary {
                id: int
            }
        }

        [use d1_a]
        model Foo {
            primary {
                id: int
            }

            foreign(Bar::id) {
                barId
                nav { bar }
            }

            barId: int
        }
        "#,
    );

    // Assert
    let foo = ast
        .models
        .iter()
        .find(|m| m.symbol.name == "Foo")
        .expect("Foo model to be present");

    assert_eq!(foo.foreign_blocks.len(), 1);
    let fb = &foo.foreign_blocks[0];
    assert_eq!(fb.adj, vec![("Bar", "id")]);
    assert_eq!(
        fb.fields.iter().map(|s| s.name).collect::<Vec<_>>(),
        vec!["barId"]
    );

    assert_eq!(foo.navigation_blocks.len(), 1);
    let nav = &foo.navigation_blocks[0];
    assert_eq!(nav.field.name, "bar");
    assert_eq!(nav.adj, vec![("Bar", "id")]);
}

#[test]
fn model_block_primary_foreign() {
    // Act
    let ast = lex_and_parse(
        r#"
        [use d1_a]
        model Person {
            primary {
                id: int
            }
        }

        [use d1_a]
        model PassportEntry {
            primary foreign(Person::id) {
                personId
            }
        }
        "#,
    );

    // Assert
    let passport = ast
        .models
        .iter()
        .find(|m| m.symbol.name == "PassportEntry")
        .expect("PassportEntry model to be present");

    assert_eq!(passport.primary_fields, vec!["personId"]);
    assert_eq!(passport.foreign_blocks.len(), 1);
    let fb = &passport.foreign_blocks[0];
    assert_eq!(fb.adj, vec![("Person", "id")]);
    assert_eq!(
        fb.fields.iter().map(|s| s.name).collect::<Vec<_>>(),
        vec!["personId"]
    );
}

#[test]
fn model_block_unique_foreign() {
    // Act
    let ast = lex_and_parse(
        r#"
        [use d1_a]
        model Company {
            primary {
                id: int
            }
        }

        [use d1_a]
        model Employee {
            primary {
                id: int
            }

            unique foreign(Company::id) {
                companyId
            }

            companyId: int
        }
        "#,
    );

    // Assert
    let employee = ast
        .models
        .iter()
        .find(|m| m.symbol.name == "Employee")
        .expect("Employee model to be present");

    assert_eq!(employee.unique_constraints.len(), 1);
    assert_eq!(employee.unique_constraints[0].fields, vec!["companyId"]);
    assert_eq!(employee.foreign_blocks.len(), 1);
    let fb = &employee.foreign_blocks[0];
    assert_eq!(fb.adj, vec![("Company", "id")]);
    assert_eq!(
        fb.fields.iter().map(|s| s.name).collect::<Vec<_>>(),
        vec!["companyId"]
    );
}

#[test]
fn model_block_unique_with_nested_foreign() {
    // Act
    let ast = lex_and_parse(
        r#"
        [use d1_a]
        model Org {
            primary {
                id: int
            }
        }

        [use d1_a]
        model Member {
            primary {
                id: int
            }

            unique {
                foreign(Org::id) {
                    orgId
                }
                role: string
            }

            orgId: int
        }
        "#,
    );

    // Assert
    let member = ast
        .models
        .iter()
        .find(|m| m.symbol.name == "Member")
        .expect("Member model to be present");

    assert_eq!(member.unique_constraints.len(), 1);
    assert_eq!(member.unique_constraints[0].fields, vec!["orgId", "role"]);
    assert_eq!(member.foreign_blocks.len(), 1);
}

#[test]
fn model_block_use_tags() {
    // Act
    let ast = lex_and_parse(
        r#"
        [use d1_db, list]
        [use get, save, d2_db]
        model User {
            primary {
                id: int
            }
            name: string
        }
        "#,
    );

    // Assert
    let user = ast
        .models
        .iter()
        .find(|m| m.symbol.name == "User")
        .expect("User model to be present");

    assert_eq!(
        user.use_tag.as_ref().unwrap().env_bindings,
        vec!["d1_db", "d2_db"]
    );
    assert_eq!(
        user.use_tag.as_ref().unwrap().cruds,
        vec![CrudKind::List, CrudKind::Get, CrudKind::Save]
    );
}

#[test]
fn model_block_nav() {
    // Act
    let ast = lex_and_parse(
        r#"
        [use d1_a]
        model WeatherReport {
            primary {
                id: int
            }

            foreign(Location::id) {
                locationId
                nav { location }
            }

            locationId: int

            nav(Weather::weatherReportId) {
                weathers
            }
        }
        "#,
    );

    // Assert
    let model = ast
        .models
        .iter()
        .find(|m| m.symbol.name == "WeatherReport")
        .expect("WeatherReport model to be present");

    assert_eq!(model.navigation_blocks.len(), 2);

    let fk_nav = model
        .navigation_blocks
        .iter()
        .find(|n| n.field.name == "location")
        .expect("location nav to be present");
    assert!(
        fk_nav.is_one_to_one,
        "nav from foreign key should be one-to-one"
    );
    assert_eq!(fk_nav.adj, vec![("Location", "id")]);

    let top_nav = model
        .navigation_blocks
        .iter()
        .find(|n| n.field.name == "weathers")
        .expect("weathers nav to be present");
    assert!(
        !top_nav.is_one_to_one,
        "top level nav should not be one-to-oen"
    );
    assert_eq!(top_nav.adj, vec![("Weather", "weatherReportId")]);
}
