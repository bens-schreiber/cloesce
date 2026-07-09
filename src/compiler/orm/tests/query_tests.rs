mod common;

use common::executor::{Backends, ExecError, execute};
use common::setup::{ShardSpec, seed, setup};
use compiler_test::src_to_idl;
use idl::{CloesceIdl, IncludeTree};
use orm::query::planner::{Operation, plan};
use serde_json::{Value, json};

/// Build a `'static` [IncludeTree] from a JSON object of nested nav names.
fn tree(value: Value) -> IncludeTree<'static> {
    let s = serde_json::to_string(&value).unwrap();
    serde_json::from_str(Box::leak(s.into_boxed_str())).unwrap()
}

/// Build a [ShardSpec] from `(binding, tuples)` pairs.
fn shards(entries: &[(&'static str, Vec<Vec<Value>>)]) -> ShardSpec {
    entries.iter().cloned().collect()
}

/// Run GET/LIST end-to-end and return the hydrated body plus any step errors.
async fn run(
    idl: &CloesceIdl<'_>,
    op: Operation,
    model: &str,
    include: Value,
    params: Value,
    backends: &Backends,
) -> (Value, Vec<ExecError>) {
    let Value::Object(params) = params else {
        panic!("params must be an object")
    };
    let plan = plan(op, model, idl, &tree(include));
    execute(&plan, params, backends).await
}

/// [run] asserting no step errors, returning only the hydrated body.
async fn run_ok(
    idl: &CloesceIdl<'_>,
    op: Operation,
    model: &str,
    include: Value,
    params: Value,
    backends: &Backends,
) -> Value {
    let (body, errors) = run(idl, op, model, include, params, backends).await;
    assert!(errors.is_empty(), "{errors:?}");
    body
}

#[sqlx::test]
async fn scalar_d1_get_and_list() {
    // Arrange
    let idl = src_to_idl(
        r#"
        d1 { db }

        model Person for db {
            primary { id: int }
            column { name: string }
        }
        "#,
    );

    let backends = setup(&idl, &shards(&[])).await;
    seed(
        &backends.d1["db"],
        "INSERT INTO Person (id, name) VALUES (1, 'Alice'), (2, 'Bob'), (3, 'Cara')",
    )
    .await;

    // GET
    {
        // Act
        let body = run_ok(
            &idl,
            Operation::Get,
            "Person",
            json!({}),
            json!({ "id": 2 }),
            &backends,
        )
        .await;

        // Assert
        assert_eq!(
            body,
            json!({ "id": 2, "name": "Bob" }),
            "The record with id `2` should be returned"
        );
    }

    // LIST
    {
        // Act
        let body = run_ok(
            &idl,
            Operation::List,
            "Person",
            json!({}),
            json!({ "limit": 2 }),
            &backends,
        )
        .await;

        // Assert
        assert_eq!(
            body,
            json!([
                { "id": 1, "name": "Alice" },
                { "id": 2, "name": "Bob" },
            ]),
            "The first two records should be returned, in order by ascending primary key"
        );
    }
}

#[sqlx::test]
async fn one_nav_same_db() {
    // Arrange
    let idl = src_to_idl(
        r#"
        d1 { db }

        model Person for db {
            primary { id: int }
            foreign Dog::id { dogId }
            one Dog::id(dogId) { dog }
        }

        model Dog for db {
            primary { id: int }
            column { name: string }
        }
        "#,
    );

    let backends = setup(&idl, &shards(&[])).await;
    seed(
        &backends.d1["db"],
        "INSERT INTO Dog (id, name) VALUES (10, 'Fido'), (20, 'Rex');
         INSERT INTO Person (id, dogId) VALUES (1, 10), (2, 20)",
    )
    .await;

    // GET
    {
        // Act
        let body = run_ok(
            &idl,
            Operation::Get,
            "Person",
            json!({ "dog": {} }),
            json!({ "id": 1 }),
            &backends,
        )
        .await;

        // Assert
        assert_eq!(
            body,
            json!({ "id": 1, "dogId": 10, "dog": { "id": 10, "name": "Fido" } }),
            "The `dog` relation should be hydrated for the person with id `1`"
        );
    }

    // LIST
    {
        // Act
        let body = run_ok(
            &idl,
            Operation::List,
            "Person",
            json!({ "dog": {} }),
            json!({ "limit": 10 }),
            &backends,
        )
        .await;

        // Assert
        assert_eq!(
            body,
            json!([
                { "id": 1, "dogId": 10, "dog": { "id": 10, "name": "Fido" } },
                { "id": 2, "dogId": 20, "dog": { "id": 20, "name": "Rex" } },
            ]),
            "The `dog` relation should be hydrated for all persons"
        );
    }
}

#[sqlx::test]
async fn many_nav_same_db() {
    // Arrange
    let idl = src_to_idl(
        r#"
        d1 { db }

        model User for db {
            primary { id: int }
            many Post::userId(id) { posts }
        }

        model Post for db {
            primary { id: int }
            foreign User::id { userId }
            column { title: string }
        }
        "#,
    );

    let backends = setup(&idl, &shards(&[])).await;
    seed(
        &backends.d1["db"],
        "INSERT INTO User (id) VALUES (1), (2);
         INSERT INTO Post (id, userId, title) VALUES
            (100, 1, 'a'), (101, 2, 'b'), (102, 1, 'c'), (103, 2, 'd')",
    )
    .await;

    // GET
    {
        // Act
        let body = run_ok(
            &idl,
            Operation::Get,
            "User",
            json!({ "posts": {} }),
            json!({ "id": 1 }),
            &backends,
        )
        .await;

        // Assert
        assert_eq!(
            body,
            json!({
                "id": 1,
                "posts": [
                    { "id": 100, "userId": 1, "title": "a" },
                    { "id": 102, "userId": 1, "title": "c" },
                ]
            }),
            "The `posts` relation should be hydrated for the user with id `1`"
        );
    }

    // LIST
    {
        // Act
        let body = run_ok(
            &idl,
            Operation::List,
            "User",
            json!({ "posts": {} }),
            json!({ "limit": 10 }),
            &backends,
        )
        .await;

        // Assert
        assert_eq!(
            body,
            json!([
                { "id": 1, "posts": [
                    { "id": 100, "userId": 1, "title": "a" },
                    { "id": 102, "userId": 1, "title": "c" },
                ] },
                { "id": 2, "posts": [
                    { "id": 101, "userId": 2, "title": "b" },
                    { "id": 103, "userId": 2, "title": "d" },
                ] },
            ]),
            "The `posts` relation should be hydrated for all users"
        );
    }
}

#[sqlx::test]
async fn cross_database_nav() {
    // Arrange
    let idl = src_to_idl(
        r#"
        d1 { people }
        d1 { animals }

        model Person for people {
            primary { id: int }
            column { dogId: int }
            one Dog::id(dogId) { dog }
        }

        model Dog for animals {
            primary { id: int }
            column { name: string }
        }
        "#,
    );

    let backends = setup(&idl, &shards(&[])).await;
    seed(
        &backends.d1["animals"],
        "INSERT INTO Dog (id, name) VALUES (10, 'Fido'), (20, 'Rex')",
    )
    .await;
    seed(
        &backends.d1["people"],
        "INSERT INTO Person (id, dogId) VALUES (1, 10), (2, 20)",
    )
    .await;

    // Act
    let body = run_ok(
        &idl,
        Operation::List,
        "Person",
        json!({ "dog": {} }),
        json!({ "limit": 10 }),
        &backends,
    )
    .await;

    // Assert
    assert_eq!(
        body,
        json!([
            { "id": 1, "dogId": 10, "dog": { "id": 10, "name": "Fido" } },
            { "id": 2, "dogId": 20, "dog": { "id": 20, "name": "Rex" } },
        ]),
        "The `dog` relation should be hydrated for all persons, across databases"
    );
}

#[sqlx::test]
async fn nested_depth_two() {
    // Arrange
    let idl = src_to_idl(
        r#"
        d1 { db }

        model User for db {
            primary { id: int }
            foreign Dog::id { dogId }
            one Dog::id(dogId) { dog }
        }

        model Dog for db {
            primary { id: int }
            foreign Toy::id { toyId }
            one Toy::id(toyId) { toy }
        }

        model Toy for db {
            primary { id: int }
            column { name: string }
        }
        "#,
    );

    let backends = setup(&idl, &shards(&[])).await;
    seed(
        &backends.d1["db"],
        "INSERT INTO Toy (id, name) VALUES (100, 'Bone'), (200, 'Ball');
         INSERT INTO Dog (id, toyId) VALUES (10, 100), (20, 200);
         INSERT INTO User (id, dogId) VALUES (1, 10), (2, 20)",
    )
    .await;

    // Act
    let body = run_ok(
        &idl,
        Operation::List,
        "User",
        json!({ "dog": { "toy": {} } }),
        json!({ "limit": 10 }),
        &backends,
    )
    .await;

    // Assert
    assert_eq!(
        body,
        json!([
            { "id": 1, "dogId": 10, "dog": {
                "id": 10, "toyId": 100, "toy": { "id": 100, "name": "Bone" }
            } },
            { "id": 2, "dogId": 20, "dog": {
                "id": 20, "toyId": 200, "toy": { "id": 200, "name": "Ball" }
            } },
        ]),
        "The `dog` and `toy` relations should be hydrated for all users, across two levels of depth"
    );
}

#[sqlx::test]
async fn do_root_get() {
    // Arrange
    let idl = src_to_idl(
        r#"
        durable SubRedditDo {
            shard { subId: int }
        }

        model SubReddit for SubRedditDo(subId) {
            primary { pid: int }
            column { title: string }
        }
        "#,
    );

    let backends = setup(&idl, &shards(&[("SubRedditDo", vec![vec![json!(7)]])])).await;
    seed(
        &backends.durable["SubRedditDo"][&vec![json!(7)]],
        "INSERT INTO SubReddit (pid, title) VALUES (1, 'rust'), (2, 'go')",
    )
    .await;

    // Act
    let body = run_ok(
        &idl,
        Operation::Get,
        "SubReddit",
        json!({}),
        json!({ "subId": 7, "pid": 2 }),
        &backends,
    )
    .await;

    // Assert
    assert_eq!(
        body,
        json!({ "pid": 2, "title": "go" }),
        "The record with pid `2` should be returned from the SubRedditDo shard with subId `7`"
    );
}

#[sqlx::test]
async fn d1_root_do_child_fanout() {
    // Arrange
    let idl = src_to_idl(
        r#"
        d1 { db }

        durable TenantDo {
            shard { tenantId: int }
        }

        model Company for db {
            primary { id: int }
            column { tenantId: int }
            one Tenant::tenantId(tenantId) { tenant }
        }

        model Tenant for TenantDo(tenantId) {
            primary { pid: int }
            column { name: string }
        }
        "#,
    );

    let backends = setup(
        &idl,
        &shards(&[("TenantDo", vec![vec![json!(100)], vec![json!(200)]])]),
    )
    .await;
    seed(
        &backends.d1["db"],
        "INSERT INTO Company (id, tenantId) VALUES (1, 100), (2, 200)",
    )
    .await;
    seed(
        &backends.durable["TenantDo"][&vec![json!(100)]],
        "INSERT INTO Tenant (pid, name) VALUES (1, 'Acme')",
    )
    .await;
    seed(
        &backends.durable["TenantDo"][&vec![json!(200)]],
        "INSERT INTO Tenant (pid, name) VALUES (1, 'Globex')",
    )
    .await;

    // Act
    let body = run_ok(
        &idl,
        Operation::List,
        "Company",
        json!({ "tenant": {} }),
        json!({ "limit": 10 }),
        &backends,
    )
    .await;

    // Assert
    assert_eq!(
        body,
        json!([
            { "id": 1, "tenantId": 100, "tenant": { "pid": 1, "name": "Acme", "tenantId": 100 } },
            { "id": 2, "tenantId": 200, "tenant": { "pid": 1, "name": "Globex", "tenantId": 200 } },
        ]),
        "The `tenant` relation should be hydrated for all companies, with each company 
        fetching from the correct TenantDo shard based on its `tenantId`"
    );
}

#[sqlx::test]
async fn do_root_d1_child() {
    // Arrange
    let idl = src_to_idl(
        r#"
        d1 { db }

        durable SubRedditDo {
            shard { subId: int }
        }

        model SubReddit for SubRedditDo(subId) {
            primary { pid: int }
            many Post::subPid(pid) { posts }
        }

        model Post for db {
            primary { id: int }
            column { subPid: int }
            column { body: string }
        }
        "#,
    );

    let backends = setup(&idl, &shards(&[("SubRedditDo", vec![vec![json!(9)]])])).await;
    seed(
        &backends.durable["SubRedditDo"][&vec![json!(9)]],
        "INSERT INTO SubReddit (pid) VALUES (1)",
    )
    .await;
    seed(
        &backends.d1["db"],
        "INSERT INTO Post (id, subPid, body) VALUES (10, 1, 'hi'), (11, 1, 'yo')",
    )
    .await;

    // Act
    let body = run_ok(
        &idl,
        Operation::Get,
        "SubReddit",
        json!({ "posts": {} }),
        json!({ "subId": 9, "pid": 1 }),
        &backends,
    )
    .await;

    // Assert
    assert_eq!(
        body,
        json!({
            "pid": 1,
            "posts": [
                { "id": 10, "subPid": 1, "body": "hi" },
                { "id": 11, "subPid": 1, "body": "yo" },
            ]
        }),
        "The `posts` relation should be hydrated for the SubReddit with pid `1` from the 
        SubRedditDo shard with subId `9`"
    );
}

#[sqlx::test]
async fn empty_form_nav() {
    // Arrange
    let idl = src_to_idl(
        r#"
        d1 { db }

        model Page for db {
            primary { id: int }
            many Banner { banners }
        }

        model Banner for db {
            primary { id: int }
            column { text: string }
        }
        "#,
    );

    let backends = setup(&idl, &shards(&[])).await;
    seed(
        &backends.d1["db"],
        "INSERT INTO Banner (id, text) VALUES (1, 'sale'), (2, 'news');
         INSERT INTO Page (id) VALUES (10), (20)",
    )
    .await;

    // Act
    let body = run_ok(
        &idl,
        Operation::List,
        "Page",
        json!({ "banners": {} }),
        json!({ "limit": 10 }),
        &backends,
    )
    .await;

    // Assert
    let both = json!([
        { "id": 1, "text": "sale" },
        { "id": 2, "text": "news" },
    ]);
    assert_eq!(
        body,
        json!([
            { "id": 10, "banners": both },
            { "id": 20, "banners": both },
        ]),
        "The `banners` relation should be hydrated for all pages, even though the nav has 
        no foreign key or join condition"
    );
}

#[sqlx::test]
async fn composite_pk_spider_nav() {
    // Arrange
    let idl = src_to_idl(
        r#"
        d1 { db }

        model Order for db {
            primary {
                region: int
                num: int
            }
            many Line::{ region(region), orderNum(num) } { lines }
        }

        model Line for db {
            primary { id: int }
            column { region: int }
            column { orderNum: int }
        }
        "#,
    );

    let backends = setup(&idl, &shards(&[])).await;
    seed(
        &backends.d1["db"],
        "INSERT INTO \"Order\" (region, num) VALUES (1, 100), (2, 100);
         INSERT INTO Line (id, region, orderNum) VALUES
            (1, 1, 100), (2, 2, 100), (3, 1, 100)",
    )
    .await;

    // Act
    let body = run_ok(
        &idl,
        Operation::List,
        "Order",
        json!({ "lines": {} }),
        json!({ "limit": 10 }),
        &backends,
    )
    .await;

    // Assert
    assert_eq!(
        body,
        json!([
            { "region": 1, "num": 100, "lines": [
                { "id": 1, "region": 1, "orderNum": 100 },
                { "id": 3, "region": 1, "orderNum": 100 },
            ] },
            { "region": 2, "num": 100, "lines": [
                { "id": 2, "region": 2, "orderNum": 100 },
            ] },
        ]),
        "The `lines` relation should be hydrated for all orders, with the correct lines"
    );
}

#[sqlx::test]
async fn cardinality_one_coercion() {
    // Arrange
    let idl = src_to_idl(
        r#"
        d1 { db }

        model Owner for db {
            primary { id: int }
            one Pet::ownerId(id) { pet }
        }

        model Pet for db {
            primary { id: int }
            column { ownerId: int }
        }
        "#,
    );

    let backends = setup(&idl, &shards(&[])).await;
    seed(
        &backends.d1["db"],
        "INSERT INTO Owner (id) VALUES (1);
         INSERT INTO Pet (id, ownerId) VALUES (30, 1), (10, 1), (20, 1)",
    )
    .await;

    // Act
    let body = run_ok(
        &idl,
        Operation::Get,
        "Owner",
        json!({ "pet": {} }),
        json!({ "id": 1 }),
        &backends,
    )
    .await;

    // Assert
    assert_eq!(
        body,
        json!({ "id": 1, "pet": { "id": 10, "ownerId": 1 } }),
        "
        The `pet` relation should be coerced to a single object, with the lowest id winning"
    );
}

#[sqlx::test]
async fn spread_dedup_and_nulls() {
    // Arrange
    let idl = src_to_idl(
        r#"
        d1 { db }

        model Person for db {
            primary { id: int }
            foreign Dog::id optional { dogId }
            one Dog::id(dogId) { dog }
        }

        model Dog for db {
            primary { id: int }
            column { name: string }
        }
        "#,
    );

    let backends = setup(&idl, &shards(&[])).await;
    seed(
        &backends.d1["db"],
        "INSERT INTO Dog (id, name) VALUES (100, 'Fido'), (200, 'Rex');
         INSERT INTO Person (id, dogId) VALUES
            (1, 100), (2, 200), (3, 100), (4, NULL), (5, 200)",
    )
    .await;

    // Act
    // The five persons share two distinct non-null dogIds; the spread dedups {100, 200}
    // and drops person 4's NULL.
    let body = run_ok(
        &idl,
        Operation::List,
        "Person",
        json!({ "dog": {} }),
        json!({ "limit": 10 }),
        &backends,
    )
    .await;

    // Assert
    assert_eq!(
        body,
        json!([
            { "id": 1, "dogId": 100, "dog": { "id": 100, "name": "Fido" } },
            { "id": 2, "dogId": 200, "dog": { "id": 200, "name": "Rex" } },
            { "id": 3, "dogId": 100, "dog": { "id": 100, "name": "Fido" } },
            { "id": 4, "dogId": null, "dog": null },
            { "id": 5, "dogId": 200, "dog": { "id": 200, "name": "Rex" } },
        ])
    );
}
