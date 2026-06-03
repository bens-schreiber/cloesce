use compiler_test::src_to_idl;
use idl::{ApiMethod, HttpVerb, Model, Number, Validator};

fn find_method<'src>(model: &'src Model, name: &str) -> Option<&'src ApiMethod<'src>> {
    model
        .apis
        .iter()
        .find(|m| m.name.eq_ignore_ascii_case(name))
}

#[test]
fn adds_crud_methods_to_models() {
    // Act
    let idl = src_to_idl(
        r#"
        d1 { db }

        [crud get, save, list]
        model OrderItem for db {
            primary {
                orderId: int
                productId: int
            }
        }
    "#,
    );

    // Assert
    let order_item = idl.models.get("OrderItem").unwrap();
    assert!(find_method(order_item, "$get").is_some());
    assert!(find_method(order_item, "$list").is_some());
    assert!(find_method(order_item, "$save").is_some());

    let get_method = find_method(order_item, "$get").unwrap();

    assert_eq!(
        get_method
            .parameters
            .iter()
            .map(|p| p.name.to_string())
            .collect::<Vec<_>>(),
        vec!["orderId", "productId"]
    );

    assert!(matches!(get_method.http_verb, HttpVerb::Get));
    assert!(get_method.is_static);
}

#[test]
fn crud_methods_namespace_sources_inherit_validators() {
    // Act
    let idl = src_to_idl(
        r#"
        d1 { db }

        [crud get, list]
        model Product for db {
            primary {
                [gt 0]
                id: int
            }
        }

        source CustomDs for Product {
            include {}

            get(
                [lt 100]
                id: int
            )

            list(
                [step 10]
                lastSeen_id: int,

                [gt 0]
                limit: int
            )
        }
    "#,
    );

    // Assert
    let product = idl.models.get("Product").unwrap();

    // $get
    {
        let method = find_method(product, "$get").unwrap();

        let id = method.parameters.iter().find(|p| p.name == "id").unwrap();
        assert!(
            id.validators
                .first()
                .map(|v| matches!(v, Validator::GreaterThan(Number::Int(0))))
                .unwrap_or(false),
        );
    }

    // $get_CustomDs
    {
        let method = find_method(product, "$get_CustomDs").unwrap();

        let id = method.parameters.iter().find(|p| p.name == "id").unwrap();
        assert!(
            id.validators
                .first()
                .map(|v| matches!(v, Validator::LessThan(Number::Int(100))))
                .unwrap_or(false),
        );
    }

    // $list
    {
        let method = find_method(product, "$list").unwrap();

        let id = method
            .parameters
            .iter()
            .find(|p| p.name == "lastSeen_id")
            .unwrap();
        assert!(
            id.validators
                .first()
                .map(|v| matches!(v, Validator::GreaterThan(Number::Int(0))))
                .unwrap_or(false),
        );
        method
            .parameters
            .iter()
            .find(|p| p.name == "limit")
            .unwrap();
    }

    // $list_CustomDs
    {
        let method = find_method(product, "$list_CustomDs").unwrap();

        let last_id = method
            .parameters
            .iter()
            .find(|p| p.name == "lastSeen_id")
            .unwrap();
        assert!(
            last_id
                .validators
                .first()
                .map(|v| matches!(v, Validator::Step(10)))
                .unwrap_or(false),
        );

        let limit = method
            .parameters
            .iter()
            .find(|p| p.name == "limit")
            .unwrap();
        assert!(
            limit
                .validators
                .first()
                .map(|v| matches!(v, Validator::GreaterThan(Number::Int(0))))
                .unwrap_or(false)
        );
    }
}
