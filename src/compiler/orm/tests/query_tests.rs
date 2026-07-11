mod common;

use common::setup::MockStorage;
use compiler_test::src_to_idl;
use idl::{CloesceIdl, IncludeTree};
use orm::query::{
    plan::QueryPlan,
    planner::{Operation, plan},
};

use serde_json::{Value, json};

fn tree(value: Value) -> IncludeTree<'static> {
    let s = serde_json::to_string(&value).unwrap();
    serde_json::from_str(Box::leak(s.into_boxed_str())).unwrap()
}

async fn execute_ok<'idl>(
    idl: &'idl CloesceIdl<'_>,
    op: Operation,
    model: &str,
    include: Value,
    params: Value,
    storage: &MockStorage,
) -> (QueryPlan<'idl>, Value) {
    let Value::Object(params) = params else {
        panic!("params must be an object")
    };
    let plan = plan(op, model, idl, &tree(include));

    // Panics on any error, since the test is expected to succeed.
    let value = common::executor::QueryExecutor::execute(&plan, params, storage).await;

    (plan, value)
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

    let mut storage = MockStorage::from_idl(&idl, &[]).await;
    storage
        .seed_d1(
            "db",
            "INSERT INTO Person (id, name) VALUES (1, 'Alice'), (2, 'Bob'), (3, 'Cara')",
        )
        .await;

    // GET
    {
        // Act
        let (plan, body) = execute_ok(
            &idl,
            Operation::Get,
            "Person",
            json!({}),
            json!({ "id": 2 }),
            &storage,
        )
        .await;

        // Assert
        assert_eq!(
            body,
            json!({ "id": 2, "name": "Bob" }),
            "The record with id `2` should be returned"
        );

        assert_eq!(plan.stages.len(), 1);
        assert_eq!(plan.stages[0].steps.len(), 1);
    }

    // LIST
    {
        // Act
        let (plan, body) = execute_ok(
            &idl,
            Operation::List,
            "Person",
            json!({}),
            json!({ "limit": 2 }),
            &storage,
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
        assert_eq!(plan.stages.len(), 1);
        assert_eq!(plan.stages[0].steps.len(), 1);
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

    let mut storage = MockStorage::from_idl(&idl, &[]).await;
    storage
        .seed_d1(
            "db",
            "INSERT INTO Dog (id, name) VALUES (10, 'Fido'), (20, 'Rex');
         INSERT INTO Person (id, dogId) VALUES (1, 10), (2, 20)",
        )
        .await;

    // GET
    {
        // Act
        let (plan, body) = execute_ok(
            &idl,
            Operation::Get,
            "Person",
            json!({ "dog": {} }),
            json!({ "id": 1 }),
            &storage,
        )
        .await;

        // Assert
        assert_eq!(
            body,
            json!({ "id": 1, "dogId": 10, "dog": { "id": 10, "name": "Fido" } }),
            "The `dog` relation should be hydrated for the person with id `1`"
        );
        assert_eq!(plan.stages.len(), 2, " `dog` relies on `Person::dogId`");
        assert_eq!(plan.stages[0].steps.len(), 1);
        assert_eq!(plan.stages[1].steps.len(), 1);
    }

    // LIST
    {
        // Act
        let (plan, body) = execute_ok(
            &idl,
            Operation::List,
            "Person",
            json!({ "dog": {} }),
            json!({ "limit": 10 }),
            &storage,
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
        assert_eq!(plan.stages.len(), 2, "`dog` relies on `Person::dogId`");
        assert_eq!(plan.stages[0].steps.len(), 1);
        assert_eq!(plan.stages[1].steps.len(), 1);
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

    let mut storage = MockStorage::from_idl(&idl, &[]).await;
    storage
        .seed_d1(
            "db",
            "INSERT INTO User (id) VALUES (1), (2);
         INSERT INTO Post (id, userId, title) VALUES
            (100, 1, 'a'), (101, 2, 'b'), (102, 1, 'c'), (103, 2, 'd')",
        )
        .await;

    // GET
    {
        // Act
        let (plan, body) = execute_ok(
            &idl,
            Operation::Get,
            "User",
            json!({ "posts": {} }),
            json!({ "id": 1 }),
            &storage,
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
        assert_eq!(
            plan.stages.len(),
            1,
            "`posts` relies on `User::id` which is given, only one stage is needed"
        );
        assert_eq!(plan.stages[0].steps.len(), 2);
    }

    // LIST
    {
        // Act
        let (plan, body) = execute_ok(
            &idl,
            Operation::List,
            "User",
            json!({ "posts": {} }),
            json!({ "limit": 10 }),
            &storage,
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
        assert_eq!(
            plan.stages.len(),
            2,
            " `posts` relies on `User::id` which is only known after the first stage"
        );
        assert_eq!(plan.stages[0].steps.len(), 1);
        assert_eq!(plan.stages[1].steps.len(), 1);
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

    let mut storage = MockStorage::from_idl(&idl, &[]).await;
    storage
        .seed_d1(
            "animals",
            "INSERT INTO Dog (id, name) VALUES (10, 'Fido'), (20, 'Rex')",
        )
        .await;
    storage
        .seed_d1(
            "people",
            "INSERT INTO Person (id, dogId) VALUES (1, 10), (2, 20)",
        )
        .await;

    // Act
    let (plan, body) = execute_ok(
        &idl,
        Operation::List,
        "Person",
        json!({ "dog": {} }),
        json!({ "limit": 10 }),
        &storage,
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
    assert_eq!(plan.stages.len(), 2, " `dog` relies on `Person::dogId`");
    assert_eq!(plan.stages[0].steps.len(), 1);
    assert_eq!(plan.stages[1].steps.len(), 1);
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

    let mut storage = MockStorage::from_idl(&idl, &[]).await;
    storage
        .seed_d1(
            "db",
            "INSERT INTO Toy (id, name) VALUES (100, 'Bone'), (200, 'Ball');
         INSERT INTO Dog (id, toyId) VALUES (10, 100), (20, 200);
         INSERT INTO User (id, dogId) VALUES (1, 10), (2, 20)",
        )
        .await;

    // Act
    let (plan, body) = execute_ok(
        &idl,
        Operation::List,
        "User",
        json!({ "dog": { "toy": {} } }),
        json!({ "limit": 10 }),
        &storage,
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
    assert_eq!(
        plan.stages.len(),
        3,
        " `dog` relies on `User::dogId` and `toy` relies on `Dog::toyId`"
    );
    assert_eq!(plan.stages[0].steps.len(), 1);
    assert_eq!(plan.stages[1].steps.len(), 1);
    assert_eq!(plan.stages[2].steps.len(), 1);
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

    let mut storage = MockStorage::from_idl(&idl, &[("SubRedditDo", vec![vec![json!(7)]])]).await;
    storage
        .seed_do(
            "SubRedditDo",
            vec![json!(7)],
            "INSERT INTO SubReddit (pid, title) VALUES (1, 'rust'), (2, 'go')",
        )
        .await;

    // Act
    let (plan, body) = execute_ok(
        &idl,
        Operation::Get,
        "SubReddit",
        json!({}),
        json!({ "subId": 7, "pid": 2 }),
        &storage,
    )
    .await;

    // Assert
    assert_eq!(
        body,
        json!({ "pid": 2, "title": "go", "subId": 7 }),
        "The record with pid `2` should be returned from the SubRedditDo shard with subId `7`,
         carrying its route field `subId`"
    );
    assert_eq!(plan.stages.len(), 1);
    assert_eq!(plan.stages[0].steps.len(), 1);
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

    let mut storage = MockStorage::from_idl(
        &idl,
        &[("TenantDo", vec![vec![json!(100)], vec![json!(200)]])],
    )
    .await;
    storage
        .seed_d1(
            "db",
            "INSERT INTO Company (id, tenantId) VALUES (1, 100), (2, 200)",
        )
        .await;
    storage
        .seed_do(
            "TenantDo",
            vec![json!(100)],
            "INSERT INTO Tenant (pid, name) VALUES (1, 'Acme')",
        )
        .await;
    storage
        .seed_do(
            "TenantDo",
            vec![json!(200)],
            "INSERT INTO Tenant (pid, name) VALUES (1, 'Globex')",
        )
        .await;

    // Act
    let (plan, body) = execute_ok(
        &idl,
        Operation::List,
        "Company",
        json!({ "tenant": {} }),
        json!({ "limit": 10 }),
        &storage,
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
    assert_eq!(
        plan.stages.len(),
        2,
        "`tenant` relies on `Company::tenantId`"
    );
    assert_eq!(plan.stages[0].steps.len(), 1);
    assert_eq!(plan.stages[1].steps.len(), 1);
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

    let mut storage = MockStorage::from_idl(&idl, &[("SubRedditDo", vec![vec![json!(9)]])]).await;
    storage
        .seed_do(
            "SubRedditDo",
            vec![json!(9)],
            "INSERT INTO SubReddit (pid) VALUES (1)",
        )
        .await;
    storage
        .seed_d1(
            "db",
            "INSERT INTO Post (id, subPid, body) VALUES (10, 1, 'hi'), (11, 1, 'yo')",
        )
        .await;

    // Act
    let (plan, body) = execute_ok(
        &idl,
        Operation::Get,
        "SubReddit",
        json!({ "posts": {} }),
        json!({ "subId": 9, "pid": 1 }),
        &storage,
    )
    .await;

    // Assert
    assert_eq!(
        body,
        json!({
            "pid": 1,
            "subId": 9,
            "posts": [
                { "id": 10, "subPid": 1, "body": "hi" },
                { "id": 11, "subPid": 1, "body": "yo" },
            ]
        }),
        "The `posts` relation should be hydrated for the SubReddit with pid `1` from the
        SubRedditDo shard with subId `9`, whose root carries its route field `subId`"
    );
    assert_eq!(
        plan.stages.len(),
        1,
        "`posts` relies on `SubReddit::pid` which is given"
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

    let mut storage = MockStorage::from_idl(&idl, &[]).await;
    storage
        .seed_d1(
            "db",
            "INSERT INTO Banner (id, text) VALUES (1, 'sale'), (2, 'news');
         INSERT INTO Page (id) VALUES (10), (20)",
        )
        .await;

    // Act
    let (plan, body) = execute_ok(
        &idl,
        Operation::List,
        "Page",
        json!({ "banners": {} }),
        json!({ "limit": 10 }),
        &storage,
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
    assert_eq!(
        plan.stages.len(),
        1,
        "`banners` has no join condition and can be fetched in one go"
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

    let mut storage = MockStorage::from_idl(&idl, &[]).await;
    storage
        .seed_d1(
            "db",
            "INSERT INTO \"Order\" (region, num) VALUES (1, 100);
         INSERT INTO Line (id, region, orderNum) VALUES
            (1, 1, 100), (2, 2, 100), (3, 1, 100)",
        )
        .await;

    // Act
    let (plan, body) = execute_ok(
        &idl,
        Operation::Get,
        "Order",
        json!({ "lines": {} }),
        json!({ "region": 1, "num": 100 }),
        &storage,
    )
    .await;

    // Assert
    assert_eq!(
        body,
        json!(
            { "region": 1, "num": 100, "lines": [
                { "id": 1, "region": 1, "orderNum": 100 },
                { "id": 3, "region": 1, "orderNum": 100 },
            ]}
        ),
        "The `lines` relation should be hydrated for the order with region `1` and num `100`",
    );
    assert_eq!(
        plan.stages.len(),
        1,
        "`lines` relies on `Order::region` and `Order::num` which are given"
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

    let mut storage = MockStorage::from_idl(&idl, &[]).await;
    storage
        .seed_d1(
            "db",
            "INSERT INTO Owner (id) VALUES (1);
         INSERT INTO Pet (id, ownerId) VALUES (30, 1), (10, 1), (20, 1)",
        )
        .await;

    // Act
    let (plan, body) = execute_ok(
        &idl,
        Operation::Get,
        "Owner",
        json!({ "pet": {} }),
        json!({ "id": 1 }),
        &storage,
    )
    .await;

    // Assert
    assert_eq!(
        body,
        json!({ "id": 1, "pet": { "id": 10, "ownerId": 1 } }),
        "
        The `pet` relation should be coerced to a single object, with the lowest id winning"
    );
    assert_eq!(
        plan.stages.len(),
        1,
        "`pet` relies on `Owner::id` which is given"
    )
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

    let mut storage = MockStorage::from_idl(&idl, &[]).await;
    storage
        .seed_d1(
            "db",
            "INSERT INTO Dog (id, name) VALUES (100, 'Fido'), (200, 'Rex');
         INSERT INTO Person (id, dogId) VALUES
            (1, 100), (2, 200), (3, 100), (4, NULL), (5, 200)",
        )
        .await;

    // Act
    let (plan, body) = execute_ok(
        &idl,
        Operation::List,
        "Person",
        json!({ "dog": {} }),
        json!({ "limit": 10 }),
        &storage,
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
    assert_eq!(
        plan.stages.len(),
        2,
        "`dog` relies on `Person::dogId` which is only known after the first stage"
    );
}

#[sqlx::test]
async fn r2_field_on_d1_root() {
    // Arrange
    let idl = src_to_idl(
        r#"
        d1 { db }

        r2 Bucket {
            avatar(id: int) {
                "avatars/{id}"
            }
        }

        model User for db {
            primary { id: int }
            r2 Bucket::avatar(id) { avatar }
        }
        "#,
    );

    let mut storage = MockStorage::from_idl(&idl, &[]).await;
    storage
        .seed_d1("db", "INSERT INTO User (id) VALUES (1)")
        .await;
    storage.seed_r2("Bucket", "avatars/1", json!({ "url": "u1" }));

    // Act
    let (plan, body) = execute_ok(
        &idl,
        Operation::Get,
        "User",
        json!({ "avatar": {} }),
        json!({ "id": 1 }),
        &storage,
    )
    .await;

    // Assert
    assert_eq!(
        body,
        json!({ "id": 1, "avatar": { "url": "u1" } }),
        "The R2 `avatar` field should be read from the bucket at the per-row key"
    );
    assert_eq!(
        plan.stages.len(),
        1,
        "`avatar` relies on `User::id` which is given"
    );
    assert_eq!(plan.stages[0].steps.len(), 2);
}

#[sqlx::test]
async fn r2_field_list_fanout() {
    // Arrange
    let idl = src_to_idl(
        r#"
        d1 { db }

        r2 Bucket {
            avatar(id: int) {
                "avatars/{id}"
            }
        }

        model User for db {
            primary { id: int }
            r2 Bucket::avatar(id) { avatar }
        }
        "#,
    );

    let mut storage = MockStorage::from_idl(&idl, &[]).await;
    storage
        .seed_d1("db", "INSERT INTO User (id) VALUES (1), (2)")
        .await;
    storage.seed_r2("Bucket", "avatars/1", json!({ "url": "u1" }));
    storage.seed_r2("Bucket", "avatars/2", json!({ "url": "u2" }));

    // Act
    let (plan, body) = execute_ok(
        &idl,
        Operation::List,
        "User",
        json!({ "avatar": {} }),
        json!({ "limit": 10 }),
        &storage,
    )
    .await;

    // Assert
    assert_eq!(
        body,
        json!([
            { "id": 1, "avatar": { "url": "u1" } },
            { "id": 2, "avatar": { "url": "u2" } },
        ]),
        "Each listed row reads its own R2 object from its per-row key"
    );
    assert_eq!(
        plan.stages.len(),
        2,
        "`avatar` relies on `User::id` which is only known after the first stage"
    );
    assert_eq!(plan.stages[0].steps.len(), 1);
    assert_eq!(plan.stages[1].steps.len(), 1);
}

#[sqlx::test]
async fn r2_field_not_included_is_absent() {
    // Arrange
    let idl = src_to_idl(
        r#"
        d1 { db }

        r2 Bucket {
            avatar(id: int) {
                "avatars/{id}"
            }
        }

        model User for db {
            primary { id: int }
            r2 Bucket::avatar(id) { avatar }
        }
        "#,
    );

    let mut storage = MockStorage::from_idl(&idl, &[]).await;
    storage
        .seed_d1("db", "INSERT INTO User (id) VALUES (1)")
        .await;
    storage.seed_r2("Bucket", "avatars/1", json!({ "url": "u1" }));

    // Act
    let (plan, body) = execute_ok(
        &idl,
        Operation::Get,
        "User",
        json!({}),
        json!({ "id": 1 }),
        &storage,
    )
    .await;

    // Assert
    assert_eq!(
        body,
        json!({ "id": 1 }),
        "An un-included R2 field emits no step and is absent from the body"
    );
    assert_eq!(
        plan.stages.len(),
        1,
        "`avatar` is not included and does not require a stage"
    );
    assert_eq!(
        plan.stages[0].steps.len(),
        1,
        "The single step is the D1 read"
    );
}

#[sqlx::test]
async fn kv_field_on_d1_root() {
    // Arrange
    let idl = src_to_idl(
        r#"
        d1 { db }

        kv Cache {
            entry(region: string, id: int) -> json {
                "e/{region}/{id}"
            }
        }

        model Item for db {
            primary { id: int }
            column { region: string }
            kv Cache::entry(region, id) { entry }
        }
        "#,
    );

    let mut storage = MockStorage::from_idl(&idl, &[]).await;
    storage
        .seed_d1("db", "INSERT INTO Item (id, region) VALUES (1, 'us')")
        .await;
    storage.seed_kv(
        "Cache",
        "e/us/1",
        json!({ "hits": 3 }),
        Some(json!({ "ttl": 60 })),
    );

    // Act
    let (plan, body) = execute_ok(
        &idl,
        Operation::Get,
        "Item",
        json!({ "entry": {} }),
        json!({ "id": 1 }),
        &storage,
    )
    .await;

    // Assert
    assert_eq!(
        body,
        json!({
            "id": 1,
            "region": "us",
            "entry": { "value": { "hits": 3 }, "metadata": { "ttl": 60 } }
        }),
        "A Workers KV read wraps the value and metadata, keyed by the composite format"
    );
    assert_eq!(
        plan.stages.len(),
        2,
        "`entry` relies on `Item::region` and `Item::id` which are only known after the first stage"
    );
    assert_eq!(plan.stages[0].steps.len(), 1);
    assert_eq!(plan.stages[1].steps.len(), 1);
}

#[sqlx::test]
async fn kv_field_list_fanout() {
    // Arrange
    let idl = src_to_idl(
        r#"
        d1 { db }

        kv Cache {
            entry(id: int) -> json {
                "e/{id}"
            }
        }

        model Item for db {
            primary { id: int }
            kv Cache::entry(id) { entry }
        }
        "#,
    );

    let mut storage = MockStorage::from_idl(&idl, &[]).await;
    storage
        .seed_d1("db", "INSERT INTO Item (id) VALUES (1), (2)")
        .await;
    storage.seed_kv("Cache", "e/1", json!("a"), None);
    storage.seed_kv("Cache", "e/2", json!("b"), None);

    // Act
    let (plan, body) = execute_ok(
        &idl,
        Operation::List,
        "Item",
        json!({ "entry": {} }),
        json!({ "limit": 10 }),
        &storage,
    )
    .await;

    // Assert
    assert_eq!(
        body,
        json!([
            { "id": 1, "entry": { "value": "a", "metadata": null } },
            { "id": 2, "entry": { "value": "b", "metadata": null } },
        ]),
        "Each listed row reads its own KV entry; absent metadata is null"
    );
    assert_eq!(
        plan.stages.len(),
        2,
        "`entry` relies on `Item::id` which is only known after the first stage"
    );
    assert_eq!(plan.stages[0].steps.len(), 1);
    assert_eq!(plan.stages[1].steps.len(), 1);
}

#[sqlx::test]
async fn do_kv_field_on_do_sqlite_model() {
    // Arrange
    let idl = src_to_idl(
        r#"
        durable BoardDo {
            shard { tenantId: int }

            topCache() -> json {
                "top"
            }
        }

        model Entry for BoardDo(tenantId) {
            primary { id: int }
            column { score: int }
            kv BoardDo::{ topCache(), tenantId(tenantId) } { top }
        }
        "#,
    );

    let mut storage = MockStorage::from_idl(&idl, &[("BoardDo", vec![vec![json!(7)]])]).await;
    storage
        .seed_do(
            "BoardDo",
            vec![json!(7)],
            "INSERT INTO Entry (id, score) VALUES (1, 99)",
        )
        .await;
    storage.seed_durable_kv("BoardDo", vec![json!(7)], "top", json!([1, 2, 3]));

    // Act
    let (plan, body) = execute_ok(
        &idl,
        Operation::Get,
        "Entry",
        json!({ "top": {} }),
        json!({ "tenantId": 7, "id": 1 }),
        &storage,
    )
    .await;

    // Assert
    assert_eq!(
        body,
        json!({ "id": 1, "score": 99, "tenantId": 7, "top": [1, 2, 3] }),
        "The DO-KV `top` field reads from tenant 7's storage, routed by the root's `tenantId` \
         route field (tagged from the param)"
    );
    assert_eq!(
        plan.stages.len(),
        1,
        "`top` relies on `Entry::tenantId` which is given from the route param"
    );
    assert_eq!(plan.stages[0].steps.len(), 2,);
}

#[sqlx::test]
async fn route_param_key_fields_on_do_root() {
    // Arrange
    let idl = src_to_idl(
        r#"
        durable BoardDo {
            shard { tenantId: int }

            topCache() -> json {
                "top"
            }
        }

        r2 Bucket {
            banner(tenantId: int) {
                "banner/{tenantId}"
            }
        }

        model Entry for BoardDo(tenantId) {
            primary { id: int }
            r2 Bucket::banner(tenantId) { banner }
            kv BoardDo::{ topCache, tenantId(tenantId) } { top }
        }
        "#,
    );

    let mut storage = MockStorage::from_idl(&idl, &[("BoardDo", vec![vec![json!(7)]])]).await;
    storage
        .seed_do(
            "BoardDo",
            vec![json!(7)],
            "INSERT INTO Entry (id) VALUES (1)",
        )
        .await;
    storage.seed_r2("Bucket", "banner/7", json!({ "img": "b7" }));
    storage.seed_durable_kv("BoardDo", vec![json!(7)], "top", json!([1, 2]));

    // Act
    let (plan, body) = execute_ok(
        &idl,
        Operation::Get,
        "Entry",
        json!({ "banner": {}, "top": {} }),
        json!({ "tenantId": 7, "id": 1 }),
        &storage,
    )
    .await;

    // Assert
    assert_eq!(
        body,
        json!({ "id": 1, "tenantId": 7, "banner": { "img": "b7" }, "top": [1, 2] }),
        "Both key fields hydrate correctly from the root's merged route field"
    );
    assert_eq!(
        plan.stages.len(),
        1,
        "param-only key fields hydrate in the root's stage"
    );
    assert_eq!(plan.stages[0].steps.len(), 3,);
}

#[sqlx::test]
async fn do_kv_fanout_from_d1_list() {
    // Arrange
    let idl = src_to_idl(
        r#"
        d1 { db }

        durable BoardDo {
            shard { tenantId: int }

            topCache() -> json {
                "top"
            }
        }

        model Org for db {
            primary { id: int }
            column { tenantId: int }
            one Board::tenantId(tenantId) { board }
        }

        model Board for BoardDo(tenantId) {
            primary { pid: int }
            kv BoardDo::{ topCache(), tenantId(tenantId) } { top }
        }
        "#,
    );

    let mut storage = MockStorage::from_idl(
        &idl,
        &[("BoardDo", vec![vec![json!(100)], vec![json!(200)]])],
    )
    .await;
    storage
        .seed_d1(
            "db",
            "INSERT INTO Org (id, tenantId) VALUES (1, 100), (2, 200)",
        )
        .await;
    storage
        .seed_do(
            "BoardDo",
            vec![json!(100)],
            "INSERT INTO Board (pid) VALUES (1)",
        )
        .await;
    storage
        .seed_do(
            "BoardDo",
            vec![json!(200)],
            "INSERT INTO Board (pid) VALUES (1)",
        )
        .await;
    storage.seed_durable_kv("BoardDo", vec![json!(100)], "top", json!("acme"));
    storage.seed_durable_kv("BoardDo", vec![json!(200)], "top", json!("globex"));

    // Act
    let (plan, body) = execute_ok(
        &idl,
        Operation::List,
        "Org",
        json!({ "board": { "top": {} } }),
        json!({ "limit": 10 }),
        &storage,
    )
    .await;

    // Assert
    assert_eq!(
        body,
        json!([
            { "id": 1, "tenantId": 100, "board": { "pid": 1, "tenantId": 100, "top": "acme" } },
            { "id": 2, "tenantId": 200, "board": { "pid": 1, "tenantId": 200, "top": "globex" } },
        ]),
        "Each org's board reads its DO-KV `top` from that board's own tenant shard"
    );
    assert_eq!(
        plan.stages.len(),
        2,
        "`board` relies on `Org::tenantId` which is only known after the first stage, 
        and `top` relies on `Board::tenantId` which is a known param"
    );
}

#[sqlx::test]
async fn shardless_do_kv() {
    // Arrange
    let idl = src_to_idl(
        r#"
        durable GlobalDo {
            config() -> json {
                "config"
            }
        }

        model Settings for GlobalDo {
            primary { id: int }
            kv GlobalDo::config { config }
        }
        "#,
    );

    let mut storage = MockStorage::from_idl(&idl, &[("GlobalDo", vec![vec![]])]).await;
    storage
        .seed_do("GlobalDo", vec![], "INSERT INTO Settings (id) VALUES (1)")
        .await;
    storage.seed_durable_kv("GlobalDo", vec![], "config", json!({ "theme": "dark" }));

    // Act
    let (plan, body) = execute_ok(
        &idl,
        Operation::Get,
        "Settings",
        json!({ "config": {} }),
        json!({ "id": 1 }),
        &storage,
    )
    .await;

    // Assert
    assert_eq!(
        body,
        json!({ "id": 1, "config": { "theme": "dark" } }),
        "A shardless DO-KV field reads from the single global instance"
    );
    assert_eq!(
        plan.stages.len(),
        1,
        "`config` relies on `Settings::id` which is given"
    );
    assert_eq!(plan.stages[0].steps.len(), 2);
}

#[sqlx::test]
async fn r2_key_uses_do_root_route_field() {
    // Arrange
    let idl = src_to_idl(
        r#"
        durable BoardDo {
            shard { tenantId: int }
        }

        r2 Bucket {
            snapshot(tenantId: int, id: int) {
                "snap/{tenantId}/{id}"
            }
        }

        model Entry for BoardDo(tenantId) {
            primary { id: int }
            r2 Bucket::snapshot(tenantId, id) { snapshot }
        }
        "#,
    );

    let mut storage = MockStorage::from_idl(&idl, &[("BoardDo", vec![vec![json!(7)]])]).await;
    storage
        .seed_do(
            "BoardDo",
            vec![json!(7)],
            "INSERT INTO Entry (id) VALUES (1)",
        )
        .await;
    storage.seed_r2("Bucket", "snap/7/1", json!({ "url": "s1" }));

    // Act
    let (plan, body) = execute_ok(
        &idl,
        Operation::Get,
        "Entry",
        json!({ "snapshot": {} }),
        json!({ "tenantId": 7, "id": 1 }),
        &storage,
    )
    .await;

    // Assert
    assert_eq!(
        body,
        json!({ "id": 1, "tenantId": 7, "snapshot": { "url": "s1" } }),
        "The R2 key template resolves `tenantId` from the root's merged route field"
    );
    assert_eq!(
        plan.stages.len(),
        1,
        "`snapshot` relies on `Entry::tenantId` and `Entry::id` which are given"
    );
    assert_eq!(plan.stages[0].steps.len(), 2,);
}

#[sqlx::test]
async fn nav_local_key_is_do_root_route_field() {
    // Arrange
    let idl = src_to_idl(
        r#"
        d1 { db }

        durable BoardDo {
            shard { tenantId: int }
        }

        model Board for BoardDo(tenantId) {
            primary { pid: int }
            many Widget::tenantId(tenantId) { widgets }
        }

        model Widget for db {
            primary { id: int }
            column { tenantId: int }
        }
        "#,
    );

    let mut storage = MockStorage::from_idl(&idl, &[("BoardDo", vec![vec![json!(7)]])]).await;
    storage
        .seed_do(
            "BoardDo",
            vec![json!(7)],
            "INSERT INTO Board (pid) VALUES (1)",
        )
        .await;
    storage
        .seed_d1(
            "db",
            "INSERT INTO Widget (id, tenantId) VALUES (10, 7), (11, 7), (12, 8)",
        )
        .await;

    // Act
    let (plan, body) = execute_ok(
        &idl,
        Operation::Get,
        "Board",
        json!({ "widgets": {} }),
        json!({ "tenantId": 7, "pid": 1 }),
        &storage,
    )
    .await;

    // Assert
    assert_eq!(
        body,
        json!({
            "pid": 1,
            "tenantId": 7,
            "widgets": [
                { "id": 10, "tenantId": 7 },
                { "id": 11, "tenantId": 7 },
            ]
        }),
        "The nav spreads the root's tagged `tenantId` route field into the child select"
    );
    assert_eq!(
        plan.stages.len(),
        1,
        "`widgets` relies on `Board::tenantId` which is given"
    );
    assert_eq!(plan.stages[0].steps.len(), 2,);
}

#[sqlx::test]
async fn backingless_root_get() {
    // Arrange
    let idl = src_to_idl(
        r#"
        kv Cache {
            entry(ownerId: string) -> json {
                "e/{ownerId}"
            }
        }

        model Owner {
            route { ownerId: string }
            kv Cache::entry(ownerId) { entry }
        }
        "#,
    );

    let mut storage = MockStorage::from_idl(&idl, &[]).await;
    storage.seed_kv("Cache", "e/o1", json!("meta"), None);

    // Act
    let (plan, body) = execute_ok(
        &idl,
        Operation::Get,
        "Owner",
        json!({ "entry": {} }),
        json!({ "ownerId": "o1" }),
        &storage,
    )
    .await;

    // Assert
    assert_eq!(
        body,
        json!({ "ownerId": "o1", "entry": { "value": "meta", "metadata": null } }),
        "The root object is synthesized from the route param, then its KV field is read"
    );
    assert_eq!(
        plan.stages.len(),
        1,
        "`entry` relies on `Owner::ownerId` which is given from the route param"
    );
    assert_eq!(plan.stages[0].steps.len(), 2);
}

#[sqlx::test]
async fn kv_only_do_root_get() {
    // Arrange
    let idl = src_to_idl(
        r#"
        durable BoardDo {
            shard { tenantId: int }

            topCache() -> json {
                "top"
            }
        }

        model Board for BoardDo(tenantId) {
            kv BoardDo::{ topCache(), tenantId(tenantId) } { top }
        }
        "#,
    );

    let mut storage = MockStorage::from_idl(&idl, &[]).await;
    storage.seed_durable_kv("BoardDo", vec![json!(7)], "top", json!([9, 8, 7]));

    // Act
    let (plan, body) = execute_ok(
        &idl,
        Operation::Get,
        "Board",
        json!({ "top": {} }),
        json!({ "tenantId": 7 }),
        &storage,
    )
    .await;

    // Assert
    assert_eq!(
        body,
        json!({ "tenantId": 7, "top": [9, 8, 7] }),
        "The synthesized shard object routes the DO-KV read to tenant 7"
    );
    assert_eq!(
        plan.stages.len(),
        1,
        "`top` relies on `Board::tenantId` which is given from the route param"
    );
}

#[sqlx::test]
async fn backingless_parent_sqlite_child() {
    // Arrange
    let idl = src_to_idl(
        r#"
        d1 { db }

        model Owner {
            route { dogId: int }
            one Dog::id(dogId) { dog }
        }

        model Dog for db {
            primary { id: int }
            column { name: string }
        }
        "#,
    );

    let mut storage = MockStorage::from_idl(&idl, &[]).await;
    storage
        .seed_d1(
            "db",
            "INSERT INTO Dog (id, name) VALUES (10, 'Fido'), (20, 'Rex')",
        )
        .await;

    // Act
    let (plan, body) = execute_ok(
        &idl,
        Operation::Get,
        "Owner",
        json!({ "dog": {} }),
        json!({ "dogId": 10 }),
        &storage,
    )
    .await;

    // Assert
    assert_eq!(
        body,
        json!({ "dogId": 10, "dog": { "id": 10, "name": "Fido" } }),
        "The synthesized root spreads `dogId` into the D1 select for `dog`"
    );
    assert_eq!(
        plan.stages.len(),
        1,
        "`dog` relies on `Owner::dogId` which is given from the route param"
    );
}

#[sqlx::test]
async fn many_nav_to_backingless_is_singleton_array() {
    // Arrange
    let idl = src_to_idl(
        r#"
        d1 { db }

        model Owner for db {
            primary { id: int }
            column { groupRef: string }
            many Member::ref(groupRef) { members }
        }

        model Member {
            route { ref: string }
        }
        "#,
    );

    let mut storage = MockStorage::from_idl(&idl, &[]).await;
    storage
        .seed_d1("db", "INSERT INTO Owner (id, groupRef) VALUES (1, 'g1')")
        .await;

    // Act
    let (plan, body) = execute_ok(
        &idl,
        Operation::Get,
        "Owner",
        json!({ "members": {} }),
        json!({ "id": 1 }),
        &storage,
    )
    .await;

    // Assert
    assert_eq!(
        body,
        json!({ "id": 1, "groupRef": "g1", "members": [{ "ref": "g1" }] }),
        "A Many nav to a backing-less target yields a single synthesized child in an array"
    );
    assert_eq!(
        plan.stages.len(),
        2,
        "`members` relies on `Owner::groupRef` which is given from the D1 select"
    );
}

#[sqlx::test]
async fn route_field_on_d1_root_merges_in_stage_zero() {
    // Arrange
    let idl = src_to_idl(
        r#"
        d1 { db }

        model Ledger for db {
            primary { id: int }
            route { region: string }
        }
        "#,
    );

    let mut storage = MockStorage::from_idl(&idl, &[]).await;
    storage
        .seed_d1("db", "INSERT INTO Ledger (id) VALUES (1)")
        .await;

    // Act
    let (plan, body) = execute_ok(
        &idl,
        Operation::Get,
        "Ledger",
        json!({}),
        json!({ "id": 1, "region": "us" }),
        &storage,
    )
    .await;

    // Assert
    assert_eq!(
        body,
        json!({ "id": 1, "region": "us" }),
        "The D1 row carries its non-shard route field, merged from the param"
    );
    assert_eq!(
        plan.stages.len(),
        1,
        "the route field is merged in stage zero"
    );
    assert_eq!(plan.stages[0].steps.len(), 2);
}
