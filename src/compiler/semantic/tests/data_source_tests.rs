use compiler_test::src_to_idl;
use sqlx::{Row, SqlitePool};

async fn exec_batch(db: &SqlitePool, sql: &str) {
    sqlx::raw_sql(sql).execute(db).await.unwrap();
}

#[sqlx::test]
async fn default_data_source_tree_includes_all_relationships(db: SqlitePool) {
    let idl = src_to_idl(
        r#"
        d1 { db }

        kv kv_namespace {
            userCache(id: int) -> json {
                "{id}"
            }
        }

        r2 r2_namespace {
            userDocuments(id: int) {
                "{id}"
            }
        }

        model Profile for db {
            primary {
                id: int
            }
        }

        model Order for db {
            primary {
                id: int
            }

            foreign(User::id) {
                userId
            }
        }

        model User for db {
            primary {
                id: int
            }

            foreign(Profile::id) {
                profileId
                nav { profile }
            }

            nav(Order::userId) {
                orders
            }

            kv kv_namespace::userCache(id) {
                userCache
            }

            r2 r2_namespace::userDocuments(id) {
                userDocuments
            }
        }
    "#,
    );

    let user = idl.models.get("User").unwrap();
    let default_ds = user
        .default_data_source()
        .expect("User should have default data source");
    let tree = &default_ds.tree;

    for key in ["profile", "orders", "userCache", "userDocuments"] {
        assert!(
            tree.0.contains_key(key),
            "Default data source should include '{key}'"
        );
    }

    assert!(!default_ds.is_internal);
    assert_eq!(default_ds.name, "Default");

    assert!(!default_ds.get.is_stub);
    assert!(!default_ds.list.is_stub);
    assert!(!default_ds.save.is_stub);
    assert!(default_ds.include_query.to_uppercase().contains("SELECT"));

    exec_batch(
        &db,
        r#"CREATE TABLE Profile (id INTEGER PRIMARY KEY);
           CREATE TABLE Role (id INTEGER PRIMARY KEY);
           CREATE TABLE "Order" (id INTEGER PRIMARY KEY, userId INTEGER NOT NULL);
           CREATE TABLE User (id INTEGER PRIMARY KEY, profileId INTEGER NOT NULL);

           INSERT INTO Profile (id) VALUES (1);
           INSERT INTO Role (id) VALUES (10);
           INSERT INTO User (id, profileId) VALUES (1, 1), (2, 1);
           INSERT INTO "Order" (id, userId) VALUES (100, 1), (200, 1);"#,
    )
    .await;

    let rows = sqlx::query(&default_ds.get_query)
        .bind(1)
        .fetch_all(&db)
        .await
        .unwrap();
    assert!(!rows.is_empty(), "GET query should return rows");
    assert_eq!(rows[0].get::<u32, _>("id"), 1);
    assert_eq!(rows[0].get::<u32, _>("profile.id"), 1);

    let rows = sqlx::query(&default_ds.list_query)
        .bind(0) // lastSeen_id
        .bind(10) // limit
        .fetch_all(&db)
        .await
        .unwrap();
    assert!(rows.len() >= 2, "LIST query should return multiple rows");
}

#[test]
fn default_data_source_present_on_every_d1_model() {
    let idl = src_to_idl(
        r#"
        d1 { db }

        kv my_kv {
            cached(tag: string) -> json {
                "{tag}"
            }
        }

        [crud get, list]
        model Item for db {
            primary {
                id: int
            }

            column {
                tag: string
            }

            kv my_kv::cached(tag) {
                cached
            }
        }

        source WithKv for Item {
            include { cached }
        }
    "#,
    );

    let item = idl.models.get("Item").unwrap();
    let with_kv = item
        .data_sources
        .get("WithKv")
        .expect("WithKv data source should exist");
    assert!(!with_kv.get.is_stub);
    assert!(!with_kv.list.is_stub);
    assert!(!with_kv.save.is_stub);

    let default_ds = item.default_data_source().expect("Should have default ds");
    assert!(!default_ds.get.is_stub);
    assert!(!default_ds.list.is_stub);
    assert!(!default_ds.save.is_stub);
}

#[sqlx::test]
async fn default_data_source_skips_nested_manys(db: SqlitePool) {
    let idl = src_to_idl(
        r#"
        d1 { db }

        model Grade for db {
            primary {
                id: int
            }

            foreign(Student::id) {
                studentId
            }
        }

                model Teacher for db {
            primary {
                id: int
            }

            nav(Student::teacherId) {
                students
            }
        }

        model Student for db {
            primary {
                id: int
            }

            foreign(Teacher::id) {
                teacherId
            }

            nav(Grade::studentId) {
                grades
            }
        }
    "#,
    );

    let teacher = idl.models.get("Teacher").unwrap();
    let default_ds = teacher
        .default_data_source()
        .expect("Teacher should have default data source");
    let tree = &default_ds.tree;

    assert!(tree.0.contains_key("students"));
    let students_node = tree.0.get("students").unwrap();
    assert!(
        !students_node.0.contains_key("grades"),
        "Default data source should NOT recurse past 1:N"
    );

    exec_batch(
        &db,
        "CREATE TABLE Grade (id INTEGER PRIMARY KEY, studentId INTEGER NOT NULL);
         CREATE TABLE Teacher (id INTEGER PRIMARY KEY);
         CREATE TABLE Student (id INTEGER PRIMARY KEY, teacherId INTEGER NOT NULL);

         INSERT INTO Teacher (id) VALUES (1);
         INSERT INTO Student (id, teacherId) VALUES (10, 1), (20, 1);
         INSERT INTO Grade (id, studentId) VALUES (100, 10);",
    )
    .await;

    let rows = sqlx::query(&default_ds.get_query)
        .bind(1)
        .fetch_all(&db)
        .await
        .unwrap();
    assert_eq!(rows.len(), 2, "GET should return 2 rows (1 per student)");
    assert_eq!(rows[0].get::<u32, _>("id"), 1);
    assert_eq!(rows[0].get::<u32, _>("students.id"), 10);
    assert_eq!(rows[1].get::<u32, _>("students.id"), 20);

    let rows = sqlx::query(&default_ds.list_query)
        .bind(0)
        .bind(10)
        .fetch_all(&db)
        .await
        .unwrap();
    assert!(!rows.is_empty(), "LIST query should return rows");
}

#[sqlx::test]
async fn default_data_source_includes_multiple_one_to_ones(db: SqlitePool) {
    let idl = src_to_idl(
        r#"
        d1 { db }

        model Toy for db {
            primary {
                id: int
            }
            column {
                color: string
            }
        }

        model Dog for db {
            primary {
                id: int
            }
            column {
                breed: string
            }

            foreign(Toy::id) {
                toyId
                nav { toy }
            }
        }

        model Owner for db {
            primary {
                id: int
            }
            column {
                name: string
            }

            foreign(Dog::id) {
                dogId
                nav { dog }
            }
        }
    "#,
    );

    let owner = idl.models.get("Owner").unwrap();
    let default_ds = owner.default_data_source().unwrap();
    let tree = &default_ds.tree;

    let dog_node = tree.0.get("dog").expect("includes 'dog'");
    let toy_node = dog_node.0.get("toy").expect("includes 'dog.toy'");
    assert!(
        toy_node.0.is_empty(),
        "Default include should not recurse past leaf 1:1"
    );

    exec_batch(
        &db,
        "CREATE TABLE Toy (id INTEGER PRIMARY KEY, color TEXT NOT NULL);
         CREATE TABLE Dog (id INTEGER PRIMARY KEY, breed TEXT NOT NULL, toyId INTEGER NOT NULL REFERENCES Toy(id));
         CREATE TABLE Owner (id INTEGER PRIMARY KEY, name TEXT NOT NULL, dogId INTEGER NOT NULL REFERENCES Dog(id));

         INSERT INTO Toy (id, color) VALUES (1, 'red');
         INSERT INTO Dog (id, breed, toyId) VALUES (1, 'poodle', 1);
         INSERT INTO Owner (id, name, dogId) VALUES (1, 'Alice', 1);",
    )
    .await;

    let row = sqlx::query(&default_ds.get_query)
        .bind(1)
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(row.get::<String, _>("name"), "Alice");
    assert_eq!(row.get::<String, _>("dog.breed"), "poodle");
    assert_eq!(row.get::<String, _>("dog.toy.color"), "red");

    let rows = sqlx::query(&default_ds.list_query)
        .bind(0)
        .bind(10)
        .fetch_all(&db)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get::<String, _>("name"), "Alice");
}

#[sqlx::test]
async fn default_data_source_diamond_does_not_duplicate_traversal(db: SqlitePool) {
    let idl = src_to_idl(
        r#"
        d1 { db }

        model Team for db {
            primary {
                id: int
            }
            column {
                name: string
            }
        }

        model Department for db {
            primary {
                id: int
            }

            foreign(Team::id) {
                teamId
                nav { team }
            }
        }

        model Company for db {
            primary {
                id: int
            }

            foreign(Department::id) {
                departmentId
                nav { department }
            }

            foreign(Team::id) {
                directTeamId
                nav { team }
            }
        }
    "#,
    );

    let company = idl.models.get("Company").unwrap();
    let default_ds = company.default_data_source().unwrap();
    let tree = &default_ds.tree;

    assert!(tree.0.contains_key("department"));
    assert!(tree.0.contains_key("team"));
    let department_node = tree.0.get("department").unwrap();
    assert!(department_node.0.contains_key("team"));

    exec_batch(
        &db,
        "CREATE TABLE Team (id INTEGER PRIMARY KEY, name TEXT NOT NULL);
         CREATE TABLE Department (id INTEGER PRIMARY KEY, teamId INTEGER NOT NULL REFERENCES Team(id));
         CREATE TABLE Company (id INTEGER PRIMARY KEY, departmentId INTEGER NOT NULL REFERENCES Department(id), directTeamId INTEGER NOT NULL REFERENCES Team(id));

         INSERT INTO Team (id, name) VALUES (1, 'Alpha'), (2, 'Beta');
         INSERT INTO Department (id, teamId) VALUES (1, 1);
         INSERT INTO Company (id, departmentId, directTeamId) VALUES (1, 1, 2);",
    )
    .await;

    let row = sqlx::query(&default_ds.get_query)
        .bind(1)
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(row.get::<u32, _>("id"), 1);
    assert_eq!(row.get::<String, _>("team.name"), "Beta");
    assert_eq!(row.get::<String, _>("department.team.name"), "Alpha");

    let rows = sqlx::query(&default_ds.list_query)
        .bind(0)
        .bind(10)
        .fetch_all(&db)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
}

#[sqlx::test]
async fn default_data_source_composite_pk(db: SqlitePool) {
    let idl = src_to_idl(
        r#"
        d1 { db }

        model OrderItem for db {
            primary {
                orderId: int
                productId: int
            }
            column {
                qty: int
            }
        }
    "#,
    );

    exec_batch(
        &db,
        "CREATE TABLE OrderItem (orderId INTEGER NOT NULL, productId INTEGER NOT NULL, qty INTEGER NOT NULL, PRIMARY KEY (orderId, productId));
         INSERT INTO OrderItem (orderId, productId, qty) VALUES (1, 1, 5), (1, 2, 3), (2, 1, 7);",
    )
    .await;

    let ds = idl
        .models
        .get("OrderItem")
        .unwrap()
        .default_data_source()
        .unwrap();

    // GET by composite PK
    let row = sqlx::query(&ds.get_query)
        .bind(1)
        .bind(2)
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(row.get::<u32, _>("orderId"), 1);
    assert_eq!(row.get::<u32, _>("productId"), 2);
    assert_eq!(row.get::<u32, _>("qty"), 3);

    // LIST with seek pagination
    let rows = sqlx::query(&ds.list_query)
        .bind(0) // lastSeen_orderId
        .bind(0) // lastSeen_productId
        .bind(10) // limit
        .fetch_all(&db)
        .await
        .unwrap();
    assert_eq!(rows.len(), 3);
}

#[test]
fn custom_data_source_captures_stub_params_and_tags() {
    let idl = src_to_idl(
        r#"
        d1 { db }

        model Item for db {
            primary {
                id: int
            }
            column {
                price: int
            }
        }

        source ById for Item {
            include {}

            get([instance] id: int)
        }

        source PaginatedSince for Item {
            include {}

            list(lastId: int, limit: int)
        }
    "#,
    );

    let item = idl.models.get("Item").unwrap();

    let by_id = item.data_sources.get("ById").unwrap();
    assert!(by_id.get.is_stub, "ById's get should be a user stub");
    assert_eq!(by_id.get.parameters.len(), 1);
    assert_eq!(by_id.get.parameters[0].parameter.name, "id");
    assert!(by_id.get.parameters[0].instance_field);

    let paginated = item.data_sources.get("PaginatedSince").unwrap();
    assert!(
        paginated.list.is_stub,
        "PaginatedSince's list should be a user stub"
    );
    assert_eq!(paginated.list.parameters.len(), 2);
    assert_eq!(paginated.list.parameters[0].name, "lastId");
    assert_eq!(paginated.list.parameters[1].name, "limit");
}

#[test]
fn custom_data_source_save_stub() {
    let idl = src_to_idl(
        r#"
        d1 { db }

        model Item for db {
            primary {
                id: int
            }
        }

        source Audited for Item {
            include {}

            save(item: partial<Item>)
        }
    "#,
    );

    let audited = idl
        .models
        .get("Item")
        .unwrap()
        .data_sources
        .get("Audited")
        .unwrap();
    assert!(audited.save.is_stub, "Audited's save should be a user stub");
    assert_eq!(audited.save.parameters.len(), 1);
    assert_eq!(audited.save.parameters[0].name, "item");
}

#[test]
fn custom_data_source_inject_tag_is_captured() {
    let idl = src_to_idl(
        r#"
        d1 { db }

        model Item for db {
            primary {
                id: int
            }
        }

        source WithDb for Item {
            include {}

            [inject db]
            get(id: int)
        }
    "#,
    );

    let with_db = idl
        .models
        .get("Item")
        .unwrap()
        .data_sources
        .get("WithDb")
        .unwrap();
    assert!(with_db.get.is_stub, "WithDb's get should be a user stub");
    assert_eq!(with_db.get.injected, vec!["db"]);
}

#[test]
fn api_method_defaults_to_default_data_source() {
    let idl = src_to_idl(
        r#"
        d1 { db }

        model Item for db {
            primary {
                id: int
            }
        }

        source Custom for Item {
            include {}
        }

        api Item {
            get fetch(self) -> Item
            post fetchCustom([source Custom] self) -> Item
            post create() -> Item
        }
    "#,
    );

    let item = idl.models.get("Item").unwrap();

    let fetch = item.apis.iter().find(|m| m.name == "fetch").unwrap();
    assert_eq!(fetch.data_source, Some("Default"));

    let fetch_custom = item.apis.iter().find(|m| m.name == "fetchCustom").unwrap();
    assert_eq!(fetch_custom.data_source, Some("Custom"));

    let create = item.apis.iter().find(|m| m.name == "create").unwrap();
    assert_eq!(create.data_source, None);
}
