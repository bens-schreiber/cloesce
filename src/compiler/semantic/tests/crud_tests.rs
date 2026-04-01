use ast::{ApiMethod, CidlType, HttpVerb, Model};
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
        env { db: d1 }

        @d1(db)
        @crud(get, save, list)
        model OrderItem {
            [primary orderId, productId]
            orderId: int
            productId: int
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
        vec!["orderId", "productId", "__datasource"]
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
            db: d1
            my_kv: kv
        }

        @d1(db)
        @crud(get)
        model Product {
            [primary id]
            id: int

            @keyparam
            category: string

            @keyparam
            subcategory: string

            @kv(my_kv, "{category}/{subcategory}")
            cached: json
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
    assert!(
        category_param.unwrap().cidl_type.is_nullable(),
        "Key params should be nullable in union"
    );

    let subcategory_param = get_method
        .parameters
        .iter()
        .find(|p| p.name == "subcategory");
    assert!(
        subcategory_param.is_some(),
        "Should have subcategory key param"
    );
    assert!(
        subcategory_param.unwrap().cidl_type.is_nullable(),
        "Key params should be nullable in union"
    );
}

#[test]
fn crud_params_union_data_source_params() {
    // Act
    let ast = src_to_ast(
        r#"
        env { db: d1 }

        @d1(db)
        @crud(get, list, save)
        model Product {
            [primary id]
            id: int
            name: string
            category: string
        }

        source ByName for Product {
            include {}

            sql get(name: string) {
                "SELECT * FROM Product WHERE name = ?"
            }

            sql list(name: string, limit: int) {
                "SELECT * FROM Product WHERE name LIKE ? LIMIT ?"
            }
        }
    "#,
    );

    // Assert
    let product = ast.models.get("Product").unwrap();

    // $get should have union of default (id) and ByName (name), all nullable
    let get_method = find_method(product, "$get").unwrap();
    let get_param_names: Vec<String> = get_method
        .parameters
        .iter()
        .filter(|p| p.name != "__datasource")
        .map(|p| p.name.to_string())
        .collect();
    assert!(
        get_param_names.contains(&"id".into()),
        "GET should have 'id' from default data source"
    );
    assert!(
        get_param_names.contains(&"name".into()),
        "GET should have 'name' from ByName data source"
    );
    // All non-datasource params should be nullable
    for p in &get_method.parameters {
        if p.name != "__datasource" {
            assert!(
                p.cidl_type.is_nullable(),
                "GET param '{}' should be nullable",
                p.name
            );
        }
    }

    // $list should have union of default (lastSeen_id, limit) and ByName (name, limit)
    let list_method = find_method(product, "$list").unwrap();
    let list_param_names: Vec<String> = list_method
        .parameters
        .iter()
        .filter(|p| p.name != "__datasource")
        .map(|p| p.name.to_string())
        .collect();
    assert!(
        list_param_names.contains(&"lastSeen_id".into()),
        "LIST should have 'lastSeen_id' from default data source"
    );
    assert!(
        list_param_names.contains(&"limit".into()),
        "LIST should have 'limit' (shared between both data sources)"
    );
    assert!(
        list_param_names.contains(&"name".into()),
        "LIST should have 'name' from ByName data source"
    );
    // 'limit' should not be duplicated
    assert_eq!(
        list_param_names.iter().filter(|&n| n == "limit").count(),
        1,
        "LIST should not have duplicate 'limit' param"
    );

    // $save should just have model + __datasource
    let save_method = find_method(product, "$save").unwrap();
    assert!(
        save_method.parameters.iter().any(|p| p.name == "model"),
        "SAVE should have 'model' parameter"
    );
    assert!(
        save_method
            .parameters
            .iter()
            .any(|p| matches!(p.cidl_type, CidlType::DataSource { .. })),
        "SAVE should have __datasource parameter"
    );
}
