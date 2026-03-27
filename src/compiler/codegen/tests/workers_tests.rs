use ast::{ApiMethod, CidlType, HttpVerb, MediaType, Model};
use codegen::workers::WorkersGenerator;
use compiler_test::{SemanticResult, src_to_ast};
use sqlx::{Row, SqlitePool};

fn find_method<'a>(model: &'a Model, name: &str) -> Option<&'a ApiMethod> {
    model
        .apis
        .iter()
        .find(|m| m.name.eq_ignore_ascii_case(name))
}

async fn create_tables(db: &SqlitePool, ddl: &str) {
    for stmt in ddl.split(';').map(str::trim).filter(|s| !s.is_empty()) {
        sqlx::query(stmt).execute(db).await.unwrap();
    }
}

#[test]
fn finalize_adds_crud_methods_to_model() {
    let SemanticResult { mut ast, .. } = src_to_ast(
        r#"
        env { db: d1 }

        @d1(db)
        @crud(get, save, list)
        model User {
            [primary id]
            id: int
        }
    "#,
    );

    WorkersGenerator::finalize_api_methods(&mut ast);

    let user = ast.models.get("User").unwrap();
    assert!(find_method(user, "$get").is_some());
    assert!(find_method(user, "$list").is_some());
    assert!(find_method(user, "$save").is_some());
}

#[test]
fn finalize_does_not_overwrite_existing_method() {
    let SemanticResult { mut ast, .. } = src_to_ast(
        r#"
        env { db: d1 }

        @d1(db)
        @crud(get)
        model User {
            [primary id]
            id: int
        }

        api User {
            post GET(id: int) -> void
        }
    "#,
    );

    WorkersGenerator::finalize_api_methods(&mut ast);

    let user = ast.models.get("User").unwrap();
    let method = find_method(user, "$get").unwrap();

    assert_eq!(method.http_verb, HttpVerb::Post);
    assert_eq!(method.parameters.len(), 1);
}

#[test]
fn finalize_sets_json_media_type() {
    let SemanticResult { mut ast, .. } = src_to_ast(
        r#"
        env { db: d1 }

        @d1(db)
        @crud(get)
        model User {
            [primary id]
            id: int
        }
    "#,
    );

    WorkersGenerator::finalize_api_methods(&mut ast);

    let user = ast.models.get("User").unwrap();
    let method = find_method(user, "$get").unwrap();
    assert!(matches!(method.return_media, MediaType::Json));
    assert!(matches!(method.parameters_media, MediaType::Json));
}

#[test]
fn finalize_sets_octet_media_type() {
    let SemanticResult { mut ast, .. } = src_to_ast(
        r#"
        env { db: d1 }

        @d1(db)
        model User {
            [primary id]
            id: int
        }

        api User {
            post acceptReturnOctet(input: stream) -> stream
        }
    "#,
    );

    WorkersGenerator::finalize_api_methods(&mut ast);

    let user = ast.models.get("User").unwrap();
    let method = find_method(user, "acceptReturnOctet").unwrap();
    assert!(matches!(method.return_media, MediaType::Octet));
    assert!(matches!(method.parameters_media, MediaType::Octet));
}

#[test]
fn finalize_get_crud_adds_primary_key_for_d1_model() {
    let SemanticResult { mut ast, .. } = src_to_ast(
        r#"
        env { db: d1 }

        @d1(db)
        @crud(get)
        model User {
            [primary id]
            id: int
        }
    "#,
    );

    WorkersGenerator::finalize_api_methods(&mut ast);

    let user = ast.models.get("User").unwrap();
    let get_method = find_method(user, "$get").unwrap();

    assert!(
        get_method
            .parameters
            .iter()
            .any(|p| matches!(p.cidl_type, CidlType::DataSource { .. })),
        "GET method should have __datasource parameter"
    );

    assert!(
        get_method.parameters.iter().any(|p| p.name == "id"),
        "GET method should have primary key parameter for D1 model"
    );

    assert_eq!(get_method.http_verb, HttpVerb::Get);
    assert!(get_method.is_static);
}

#[test]
fn finalize_get_and_list_crud_adds_composite_primary_keys() {
    let SemanticResult { mut ast, .. } = src_to_ast(
        r#"
        env { db: d1 }

        @d1(db)
        @crud(get, list)
        model OrderItem {
            [primary orderId, productId]
            orderId: int
            productId: int
        }
    "#,
    );

    WorkersGenerator::finalize_api_methods(&mut ast);

    let order_item = ast.models.get("OrderItem").unwrap();

    let get_method = find_method(order_item, "$get").unwrap();
    assert!(get_method.parameters.iter().any(|p| p.name == "orderId"));
    assert!(get_method.parameters.iter().any(|p| p.name == "productId"));
    assert!(
        get_method
            .parameters
            .iter()
            .any(|p| matches!(p.cidl_type, CidlType::DataSource { .. }))
    );

    let list_method = find_method(order_item, "$list").unwrap();
    let last_seen_order = list_method
        .parameters
        .iter()
        .find(|p| p.name == "lastSeen_orderId")
        .unwrap();
    let last_seen_product = list_method
        .parameters
        .iter()
        .find(|p| p.name == "lastSeen_productId")
        .unwrap();
    assert!(last_seen_order.cidl_type.is_nullable());
    assert!(last_seen_product.cidl_type.is_nullable());
}

#[test]
fn finalize_get_crud_adds_key_params() {
    let SemanticResult { mut ast, .. } = src_to_ast(
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

    WorkersGenerator::finalize_api_methods(&mut ast);

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
        matches!(category_param.unwrap().cidl_type, CidlType::String),
        "Key params should be String type"
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
        matches!(subcategory_param.unwrap().cidl_type, CidlType::String),
        "Key params should be String type"
    );
}

#[sqlx::test]
async fn generate_default_data_sources(db: SqlitePool) {
    let SemanticResult { mut ast, .. } = src_to_ast(
        r#"
        env {
            db: d1
            kv_namespace: kv
            r2_namespace: r2
        }

        @d1(db)
        model Profile {
            [primary id]
            id: int
        }

        @d1(db)
        model Role {
            [primary id]
            id: int

            [nav users <> User::roles]
            users: Array<User>
        }

        @d1(db)
        model Order {
            [primary id]
            id: int

            [foreign userId -> User::id]
            userId: int
        }

        @d1(db)
        model User {
            [primary id]
            id: int

            [foreign profileId -> Profile::id]
            [nav profile -> profileId]
            profileId: int
            profile: Profile

            [nav orders -> Order::userId]
            orders: Array<Order>

            [nav roles <> Role::users]
            roles: Array<Role>

            @kv(kv_namespace, "{id}")
            userCache: json

            @r2(r2_namespace, "{id}")
            userDocuments: R2Object
        }
    "#,
    );

    WorkersGenerator::generate_default_data_sources(&mut ast);

    // Assert include tree
    let user = ast.models.get("User").unwrap();
    let default_ds = user
        .default_data_source()
        .expect("User should have default data source");
    let tree = &default_ds.tree;

    assert!(
        tree.0.contains_key("profile"),
        "Default data source should include 1:1 relationship 'profile'"
    );
    assert!(
        tree.0.contains_key("orders"),
        "Default data source should include 1:M relationship 'orders'"
    );
    assert!(
        tree.0.contains_key("roles"),
        "Default data source should include M:M relationship 'roles'"
    );
    assert!(
        tree.0.contains_key("userCache"),
        "Default data source should include KV object 'userCache'"
    );
    assert!(
        tree.0.contains_key("userDocuments"),
        "Default data source should include R2 object 'userDocuments'"
    );
    assert!(
        !default_ds.is_private,
        "Default data source should be public"
    );
    assert_eq!(
        default_ds.name, "default",
        "Data source should be named 'default'"
    );

    // Assert get/list SQL executes correctly
    create_tables(
        &db,
        r#"CREATE TABLE Profile (id INTEGER PRIMARY KEY);
           CREATE TABLE Role (id INTEGER PRIMARY KEY);
           CREATE TABLE "Order" (id INTEGER PRIMARY KEY, userId INTEGER NOT NULL);
           CREATE TABLE User (id INTEGER PRIMARY KEY, profileId INTEGER NOT NULL);
           CREATE TABLE RoleUser ("left" INTEGER NOT NULL, "right" INTEGER NOT NULL)"#,
    )
    .await;

    sqlx::query("INSERT INTO Profile (id) VALUES (1)")
        .execute(&db)
        .await
        .unwrap();
    sqlx::query("INSERT INTO Role (id) VALUES (10)")
        .execute(&db)
        .await
        .unwrap();
    sqlx::query("INSERT INTO User (id, profileId) VALUES (1, 1), (2, 1)")
        .execute(&db)
        .await
        .unwrap();
    sqlx::query(r#"INSERT INTO "Order" (id, userId) VALUES (100, 1), (200, 1)"#)
        .execute(&db)
        .await
        .unwrap();
    sqlx::query(r#"INSERT INTO RoleUser ("left", "right") VALUES (10, 1)"#)
        .execute(&db)
        .await
        .unwrap();

    let get_sql = &default_ds.get.as_ref().unwrap().raw_sql;
    let rows = sqlx::query(get_sql).bind(1).fetch_all(&db).await.unwrap();
    assert!(!rows.is_empty(), "GET query should return rows");
    assert_eq!(rows[0].get::<u32, _>("id"), 1);
    assert_eq!(rows[0].get::<u32, _>("profile.id"), 1);

    let list_sql = &default_ds.list.as_ref().unwrap().raw_sql;
    let rows = sqlx::query(list_sql)
        .bind(0) // lastSeen_id
        .bind(10) // limit
        .fetch_all(&db)
        .await
        .unwrap();
    assert!(rows.len() >= 2, "LIST query should return multiple rows");
}

#[sqlx::test]
async fn generate_default_data_sources_does_not_include_manys(db: SqlitePool) {
    let SemanticResult { mut ast, .. } = src_to_ast(
        r#"
        env { db: d1 }

        @d1(db)
        model Grade {
            [primary id]
            id: int

            [foreign studentId -> Student::id]
            studentId: int
        }

        @d1(db)
        model Teacher {
            [primary id]
            id: int

            [nav students -> Student::teacherId]
            students: Array<Student>
        }

        @d1(db)
        model Student {
            [primary id]
            id: int

            [foreign teacherId -> Teacher::id]
            teacherId: int

            [nav grades -> Grade::studentId]
            grades: Array<Grade>
        }
    "#,
    );

    WorkersGenerator::generate_default_data_sources(&mut ast);

    // Assert include tree
    let teacher = ast.models.get("Teacher").unwrap();
    let default_ds = teacher
        .default_data_source()
        .expect("Teacher should have default data source");
    let tree = &default_ds.tree;

    assert!(
        tree.0.contains_key("students"),
        "Default data source for Teacher should include 'students' relationship"
    );
    let students_node = tree.0.get("students").unwrap();
    assert!(
        !students_node.0.contains_key("grades"),
        "Default data source for Teacher should NOT include nested 'grades' under 'students'"
    );

    // Assert get/list SQL executes correctly
    create_tables(
        &db,
        "CREATE TABLE Grade (id INTEGER PRIMARY KEY, studentId INTEGER NOT NULL);
         CREATE TABLE Teacher (id INTEGER PRIMARY KEY);
         CREATE TABLE Student (id INTEGER PRIMARY KEY, teacherId INTEGER NOT NULL)",
    )
    .await;

    sqlx::query("INSERT INTO Teacher (id) VALUES (1)")
        .execute(&db)
        .await
        .unwrap();
    sqlx::query("INSERT INTO Student (id, teacherId) VALUES (10, 1), (20, 1)")
        .execute(&db)
        .await
        .unwrap();
    sqlx::query("INSERT INTO Grade (id, studentId) VALUES (100, 10)")
        .execute(&db)
        .await
        .unwrap();

    let get_sql = &default_ds.get.as_ref().unwrap().raw_sql;
    let rows = sqlx::query(get_sql).bind(1).fetch_all(&db).await.unwrap();
    assert_eq!(rows.len(), 2, "GET should return 2 rows (1 per student)");
    assert_eq!(rows[0].get::<u32, _>("id"), 1);
    assert_eq!(rows[0].get::<u32, _>("students.id"), 10);
    assert_eq!(rows[1].get::<u32, _>("students.id"), 20);

    let list_sql = &default_ds.list.as_ref().unwrap().raw_sql;
    let rows = sqlx::query(list_sql)
        .bind(0)
        .bind(10)
        .fetch_all(&db)
        .await
        .unwrap();
    assert!(!rows.is_empty(), "LIST query should return rows");
}

#[sqlx::test]
async fn generate_default_data_sources_includes_multiple_one_to_ones(db: SqlitePool) {
    let SemanticResult { mut ast, .. } = src_to_ast(
        r#"
        env { db: d1 }

        @d1(db)
        model Toy {
            [primary id]
            id: int
            color: string
        }

        @d1(db)
        model Dog {
            [primary id]
            id: int
            breed: string

            [foreign toyId -> Toy::id]
            [nav toy -> toyId]
            toyId: int
            toy: Toy
        }

        @d1(db)
        model Owner {
            [primary id]
            id: int
            name: string

            [foreign dogId -> Dog::id]
            [nav dog -> dogId]
            dogId: int
            dog: Dog
        }
    "#,
    );

    WorkersGenerator::generate_default_data_sources(&mut ast);

    // Assert include tree
    let owner = ast.models.get("Owner").unwrap();
    let default_ds = owner
        .default_data_source()
        .expect("Owner should have default data source");
    let tree = &default_ds.tree;
    assert!(
        tree.0.contains_key("dog"),
        "Default data source for Owner should include 'dog' relationship"
    );

    let dog_node = tree.0.get("dog").unwrap();
    assert!(
        dog_node.0.contains_key("toy"),
        "Default data source for Owner should include 'toy' relationship under 'dog'"
    );

    let toy_node = dog_node.0.get("toy").unwrap();
    assert!(
        toy_node.0.is_empty(),
        "Default data source for Owner should NOT include any nested relationships under 'toy'"
    );

    // Assert get/list SQL executes correctly with nested joins
    create_tables(
        &db,
        "CREATE TABLE Toy (id INTEGER PRIMARY KEY, color TEXT NOT NULL);
         CREATE TABLE Dog (id INTEGER PRIMARY KEY, breed TEXT NOT NULL, toyId INTEGER NOT NULL REFERENCES Toy(id));
         CREATE TABLE Owner (id INTEGER PRIMARY KEY, name TEXT NOT NULL, dogId INTEGER NOT NULL REFERENCES Dog(id))",
    )
    .await;

    sqlx::query("INSERT INTO Toy (id, color) VALUES (1, 'red')")
        .execute(&db)
        .await
        .unwrap();
    sqlx::query("INSERT INTO Dog (id, breed, toyId) VALUES (1, 'poodle', 1)")
        .execute(&db)
        .await
        .unwrap();
    sqlx::query("INSERT INTO Owner (id, name, dogId) VALUES (1, 'Alice', 1)")
        .execute(&db)
        .await
        .unwrap();

    let get_sql = &default_ds.get.as_ref().unwrap().raw_sql;
    let row = sqlx::query(get_sql).bind(1).fetch_one(&db).await.unwrap();
    assert_eq!(row.get::<String, _>("name"), "Alice");
    assert_eq!(row.get::<String, _>("dog.breed"), "poodle");
    assert_eq!(row.get::<String, _>("dog.toy.color"), "red");

    let list_sql = &default_ds.list.as_ref().unwrap().raw_sql;
    let rows = sqlx::query(list_sql)
        .bind(0)
        .bind(10)
        .fetch_all(&db)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get::<String, _>("name"), "Alice");
}

#[sqlx::test]
async fn generate_default_data_sources_diamond_does_not_duplicate_traversal(db: SqlitePool) {
    let SemanticResult { mut ast, .. } = src_to_ast(
        r#"
        env { db: d1 }

        @d1(db)
        model Team {
            [primary id]
            id: int
            name: string
        }

        @d1(db)
        model Department {
            [primary id]
            id: int

            [foreign teamId -> Team::id]
            [nav team -> teamId]
            teamId: int
            team: Team
        }

        @d1(db)
        model Company {
            [primary id]
            id: int

            [foreign departmentId -> Department::id]
            [nav department -> departmentId]
            departmentId: int
            department: Department

            [foreign teamId -> Team::id]
            [nav team -> teamId]
            teamId: int
            team: Team
        }
    "#,
    );

    WorkersGenerator::generate_default_data_sources(&mut ast);

    // Assert include tree
    let company = ast.models.get("Company").unwrap();
    let default_ds = company
        .default_data_source()
        .expect("Company should have default data source");
    let tree = &default_ds.tree;

    assert!(
        tree.0.contains_key("department"),
        "Default data source for Company should include 'department' relationship"
    );
    assert!(
        tree.0.contains_key("team"),
        "Default data source for Company should include 'team' relationship"
    );

    let department_node = tree.0.get("department").unwrap();
    assert!(
        department_node.0.contains_key("team"),
        "Default data source for Company should include 'team' under 'department'"
    );

    // Assert get/list SQL executes correctly
    create_tables(
        &db,
        "CREATE TABLE Team (id INTEGER PRIMARY KEY, name TEXT NOT NULL);
         CREATE TABLE Department (id INTEGER PRIMARY KEY, teamId INTEGER NOT NULL REFERENCES Team(id));
         CREATE TABLE Company (id INTEGER PRIMARY KEY, departmentId INTEGER NOT NULL REFERENCES Department(id), teamId INTEGER NOT NULL REFERENCES Team(id))",
    )
    .await;

    sqlx::query("INSERT INTO Team (id, name) VALUES (1, 'Alpha'), (2, 'Beta')")
        .execute(&db)
        .await
        .unwrap();
    sqlx::query("INSERT INTO Department (id, teamId) VALUES (1, 1)")
        .execute(&db)
        .await
        .unwrap();
    sqlx::query("INSERT INTO Company (id, departmentId, teamId) VALUES (1, 1, 2)")
        .execute(&db)
        .await
        .unwrap();

    let get_sql = &default_ds.get.as_ref().unwrap().raw_sql;
    let row = sqlx::query(get_sql).bind(1).fetch_one(&db).await.unwrap();
    assert_eq!(row.get::<u32, _>("id"), 1);
    assert_eq!(row.get::<String, _>("team.name"), "Beta");
    assert_eq!(row.get::<String, _>("department.team.name"), "Alpha");

    let list_sql = &default_ds.list.as_ref().unwrap().raw_sql;
    let rows = sqlx::query(list_sql)
        .bind(0)
        .bind(10)
        .fetch_all(&db)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
}

#[sqlx::test]
async fn generate_default_data_sources_composite_pk(db: SqlitePool) {
    let SemanticResult { mut ast, .. } = src_to_ast(
        r#"
        env { db: d1 }

        @d1(db)
        model OrderItem {
            [primary orderId, productId]
            orderId: int
            productId: int
            qty: int
        }
    "#,
    );

    WorkersGenerator::generate_default_data_sources(&mut ast);

    create_tables(
        &db,
        "CREATE TABLE OrderItem (orderId INTEGER NOT NULL, productId INTEGER NOT NULL, qty INTEGER NOT NULL, PRIMARY KEY (orderId, productId))",
    )
    .await;

    sqlx::query(
        "INSERT INTO OrderItem (orderId, productId, qty) VALUES (1, 1, 5), (1, 2, 3), (2, 1, 7)",
    )
    .execute(&db)
    .await
    .unwrap();

    let ds = ast
        .models
        .get("OrderItem")
        .unwrap()
        .default_data_source()
        .unwrap();

    // GET by composite PK
    let get_sql = &ds.get.as_ref().unwrap().raw_sql;
    let row = sqlx::query(get_sql)
        .bind(1)
        .bind(2)
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(row.get::<u32, _>("orderId"), 1);
    assert_eq!(row.get::<u32, _>("productId"), 2);
    assert_eq!(row.get::<u32, _>("qty"), 3);

    // LIST with seek pagination
    let list_sql = &ds.list.as_ref().unwrap().raw_sql;
    let rows = sqlx::query(list_sql)
        .bind(0) // lastSeen_orderId
        .bind(0) // lastSeen_productId
        .bind(10) // limit
        .fetch_all(&db)
        .await
        .unwrap();
    assert_eq!(rows.len(), 3);
}
