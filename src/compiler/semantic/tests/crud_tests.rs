use ast::{ApiMethod, CidlType, HttpVerb, Model, Number, Validator};
use compiler_test::src_to_ast;

fn find_method<'src>(model: &'src Model, name: &str) -> Option<&'src ApiMethod<'src>> {
    model
        .apis
        .iter()
        .find(|m| m.name.eq_ignore_ascii_case(name))
}

#[test]
fn adds_crud_methods_to_models() {
    // Act
    let ast = src_to_ast(
        r#"
        env {
            d1 { db }
        }

        [use db, get, save, list]
        model OrderItem {
            primary {
                orderId: int
                productId: int
            }
        }
    "#,
    );

    // Assert
    let order_item = ast.models.get("OrderItem").unwrap();
    assert!(find_method(order_item, "$get").is_some());
    assert!(find_method(order_item, "$list").is_some());
    assert!(find_method(order_item, "$save").is_some());

    let get_method = find_method(order_item, "$get").unwrap();

    assert!(
        get_method
            .parameters
            .iter()
            .any(|p| matches!(p.cidl_type, CidlType::DataSource { .. })),
        "GET method should have __datasource parameter"
    );

    assert_eq!(
        get_method
            .parameters
            .iter()
            .map(|p| p.name.to_string())
            .collect::<Vec<_>>(),
        vec!["Default_orderId", "Default_productId", "__datasource"]
    );

    assert_eq!(get_method.http_verb, HttpVerb::Get);
    assert!(get_method.is_static);
}

#[test]
fn crud_key_params() {
    // Act
    let ast = src_to_ast(
        r#"
        env {
            d1 { db }
            kv { my_kv }
        }

        [use db, get]
        model Product {
            primary {
                id: int
            }

            keyfield {
                category
                subcategory
            }

            kv(my_kv, "{category}/{subcategory}") {
                cached: json
            }
        }
    "#,
    );

    // Assert
    let product = ast.models.get("Product").unwrap();
    let get_method = find_method(product, "$get").unwrap();

    assert!(
        get_method
            .parameters
            .iter()
            .any(|p| matches!(p.cidl_type, CidlType::DataSource { .. })),
        "GET method should have __datasource parameter"
    );

    let category_param = get_method.parameters.iter().find(|p| p.name == "category");
    assert!(category_param.is_some(), "Should have category key param");

    let subcategory_param = get_method
        .parameters
        .iter()
        .find(|p| p.name == "subcategory");
    assert!(
        subcategory_param.is_some(),
        "Should have subcategory key param"
    );
}

#[test]
fn crud_methods_namespace_sources_inherit_validators() {
    // Act
    let ast = src_to_ast(
        r#"
        env {
            d1 { db }
        }

        [use db, get, list]
        model Product {
            primary {
                [gt 0]
                id: int
            }
        }

        source CustomDs for Product {
            include {}

            sql get(
                [lt 100]
                id: int
            ) {
                "SELECT * FROM Product WHERE id = ?"
            }

            sql list(
                [step 10]
                lastSeen_id: int,

                [gt 0]
                limit: int
            ) {
                "SELECT * FROM Product WHERE id > ? LIMIT ?"
            }
        }
    "#,
    );

    // Assert
    let product = ast.models.get("Product").unwrap();

    // GET
    {
        let method = find_method(product, "$get").unwrap();

        let default_id_param = method
            .parameters
            .iter()
            .find(|p| p.name == "Default_id")
            .unwrap();
        assert!(
            default_id_param
                .validators
                .first()
                .map(|v| matches!(v, Validator::GreaterThan(Number::Int(0))))
                .unwrap_or(false),
        );

        let custom_id_param = method
            .parameters
            .iter()
            .find(|p| p.name == "CustomDs_id")
            .unwrap();
        assert!(
            custom_id_param
                .validators
                .first()
                .map(|v| matches!(v, Validator::LessThan(Number::Int(100))))
                .unwrap_or(false),
        );
    }

    // LIST
    {
        let method = find_method(product, "$list").unwrap();

        let default_last_id_param = method
            .parameters
            .iter()
            .find(|p| p.name == "Default_lastSeen_id")
            .unwrap();
        assert!(
            default_last_id_param
                .validators
                .first()
                .map(|v| matches!(v, Validator::GreaterThan(Number::Int(0))))
                .unwrap_or(false),
        );

        let custom_last_id_param = method
            .parameters
            .iter()
            .find(|p| p.name == "CustomDs_lastSeen_id")
            .unwrap();
        assert!(
            custom_last_id_param
                .validators
                .first()
                .map(|v| matches!(v, Validator::Step(10)))
                .unwrap_or(false),
        );

        let default_limit_param = method
            .parameters
            .iter()
            .find(|p| p.name == "Default_limit")
            .unwrap();
        assert!(default_limit_param.validators.is_empty());

        let custom_limit_param = method
            .parameters
            .iter()
            .find(|p| p.name == "CustomDs_limit")
            .unwrap();
        assert!(
            custom_limit_param
                .validators
                .first()
                .map(|v| matches!(v, Validator::GreaterThan(Number::Int(0))))
                .unwrap_or(false)
        );
    }
}
