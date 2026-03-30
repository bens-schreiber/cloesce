use compiler_test::src_to_ast;
use sqlx::{Row, SqlitePool};

async fn create_tables(db: &SqlitePool, ddl: &str) {
    for stmt in ddl.split(';').map(str::trim).filter(|s| !s.is_empty()) {
        sqlx::query(stmt).execute(db).await.unwrap();
    }
}

#[sqlx::test]
async fn default_data_sources(db: SqlitePool) {
    // Act
    let ast = src_to_ast(
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

    // Assert
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
        !default_ds.is_internal,
        "Default data source should not be internal"
    );
    assert_eq!(
        default_ds.name, "Default",
        "Data source should be named 'default'"
    );

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

#[test]
fn default_data_source_methods() {
    // Act
    let ast = src_to_ast(
        r#"
        env {
            db: d1
            my_kv: kv
        }

        @d1(db)
        @crud(get, list)
        model Item {
            [primary id]
            id: int

            @keyparam
            tag: string

            @kv(my_kv, "{tag}")
            cached: json
        }

        source WithKv for Item {
            include { cached }
        }
    "#,
    );

    // Assert
    let item = ast.models.get("Item").unwrap();
    let with_kv = item
        .data_sources
        .iter()
        .find(|ds| ds.name == "WithKv")
        .expect("WithKv data source should exist");
    assert!(
        with_kv.get.is_some(),
        "WithKv should have a default get method"
    );
    assert!(
        with_kv.list.is_some(),
        "WithKv should have a default list method"
    );

    let default_ds = item.default_data_source().expect("Should have default ds");
    assert!(default_ds.get.is_some());
    assert!(default_ds.list.is_some());
}

#[sqlx::test]
async fn default_data_sources_does_not_include_manys(db: SqlitePool) {
    // Act
    let ast = src_to_ast(
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

    // Assert
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

    // Assert
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
async fn default_data_sources_includes_multiple_one_to_ones(db: SqlitePool) {
    // Act
    let ast = src_to_ast(
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

    // Assert
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

    // Assert
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
async fn diamond_does_not_duplicate_traversal(db: SqlitePool) {
    // Act
    let ast = src_to_ast(
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

    // Assert
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

    // Assert
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
async fn default_data_sources_composite_pk(db: SqlitePool) {
    // Act
    let ast = src_to_ast(
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

    // Assert
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

#[test]
fn resolve_sql_params() {
    // Act
    let ast = src_to_ast(
        r#"
        env { db: d1 }

        @d1(db)
        model Item {
            [primary id]
            id: int
            price: int
        }

        source ById for Item {
            include {}
            sql get(itemId: int) { "SELECT * FROM Item WHERE id = $itemId AND id != $itemId" }
        }

        source ByPriceRange for Item {
            include {}
            sql list(minPrice: int, maxPrice: int, limit: int) {
                "SELECT * FROM Item WHERE price >= $minPrice AND price <= $maxPrice LIMIT $limit"
            }
        }
    "#,
    );

    // Assert
    let item = ast.models.get("Item").unwrap();
    let by_id = item
        .data_sources
        .iter()
        .find(|ds| ds.name == "ById")
        .unwrap();
    let get_sql = &by_id.get.as_ref().unwrap().raw_sql;
    assert!(!get_sql.contains("$itemId"), "got: {get_sql}");
    assert_eq!(get_sql.matches("?1").count(), 2, "got: {get_sql}");

    let by_range = item
        .data_sources
        .iter()
        .find(|ds| ds.name == "ByPriceRange")
        .unwrap();
    let list_sql = &by_range.list.as_ref().unwrap().raw_sql;
    assert!(!list_sql.contains('$'), "got: {list_sql}");
    let pos = |n: &str| list_sql.find(n).unwrap();
    assert!(
        pos("?1") < pos("?2") && pos("?2") < pos("?3"),
        "got: {list_sql}"
    );
}

#[sqlx::test]
async fn include_placeholder_expands_to_select(db: SqlitePool) {
    // Arrange
    let ast = src_to_ast(
        r#"
        env { db: d1 }

        @d1(db)
        model Post {
            [primary id]
            id: int
            title: string
        }

        source Recent for Post {
            include {}
            sql get(id: int) { "$include WHERE Post.id = $id" }
        }
    "#,
    );

    let ds = ast
        .models
        .get("Post")
        .unwrap()
        .data_sources
        .iter()
        .find(|ds| ds.name == "Recent")
        .unwrap();

    let raw_sql = &ds.get.as_ref().unwrap().raw_sql;

    // $include should be gone and replaced with a SELECT statement
    assert!(!raw_sql.contains("$include"), "got: {raw_sql}");
    assert!(raw_sql.to_uppercase().contains("SELECT"), "got: {raw_sql}");
    // $id should be resolved to ?1
    assert!(!raw_sql.contains("$id"), "got: {raw_sql}");
    assert!(raw_sql.contains("?1"), "got: {raw_sql}");

    // The expanded SQL should be executable
    create_tables(&db, "CREATE TABLE Post (id INTEGER PRIMARY KEY, title TEXT NOT NULL)").await;
    sqlx::query("INSERT INTO Post (id, title) VALUES (1, 'hello'), (2, 'world')")
        .execute(&db)
        .await
        .unwrap();

    let row = sqlx::query(raw_sql).bind(1).fetch_one(&db).await.unwrap();
    assert_eq!(row.get::<u32, _>("id"), 1);
    assert_eq!(row.get::<String, _>("title"), "hello");
}
