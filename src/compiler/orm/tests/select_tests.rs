mod common;

use common::setup::{MockStorage, tree};
use compiler_test::src_to_idl;
use idl::CloesceIdl;
use orm::query::select::{
    plan::{Select, SelectPlan, SqlArgument, SqlSegment},
    planner::SelectOperation,
};

use serde_json::{Value, json};

/// Find the SQL step hydrating the nav `field`, returning its `(sql, arguments)`.
fn sql_step<'p>(
    plan: &'p SelectPlan<'p>,
    field: &str,
) -> (&'p [SqlSegment], &'p [SqlArgument<'p>]) {
    let table = plan
        .tables
        .iter()
        .position(|t| t.parent.as_ref().is_some_and(|p| p.field == field))
        .expect("nav table to exist");
    plan.stages
        .iter()
        .flat_map(|s| &s.steps)
        .find_map(|step| match &step.query {
            Select::Sql { sql, arguments, .. } if step.table == table => {
                Some((sql.as_slice(), arguments.as_slice()))
            }
            _ => None,
        })
        .expect("nav sql step to exist")
}

/// Concatenate a statement's literal segments (binds render as `<?>`).
fn sql_literals(sql: &[SqlSegment]) -> String {
    sql.iter()
        .map(|seg| match seg {
            SqlSegment::Literal(text) => text.as_str(),
            SqlSegment::Bind(_) => "<?>",
        })
        .collect()
}

async fn seed(
    idl: &CloesceIdl<'_>,
    model: &str,
    include: Value,
    payload: Value,
    storage: &mut MockStorage,
) {
    let plan = orm::query::save::planner::plan(model, idl, &tree(include), &payload)
        .expect("seed save to plan");
    common::save_executor::execute(&plan, storage).await;
}

async fn execute_ok<'idl>(
    idl: &'idl CloesceIdl<'_>,
    op: SelectOperation,
    model: &str,
    include: Value,
    params: Value,
    storage: &MockStorage,
) -> (SelectPlan<'idl>, Value) {
    let Value::Object(params) = params else {
        panic!("params must be an object")
    };
    let plan = orm::query::select::planner::plan(op, model, idl, &tree(include));

    // Panics on any error, since the test is expected to succeed.
    let value = common::select_executor::execute(&plan, params, storage).await;

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
    for (id, name) in [(1, "Alice"), (2, "Bob"), (3, "Cara")] {
        seed(
            &idl,
            "Person",
            json!({}),
            json!({ "id": id, "name": name }),
            &mut storage,
        )
        .await;
    }

    // GET
    {
        // Act
        let (plan, body) = execute_ok(
            &idl,
            SelectOperation::Get,
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
            SelectOperation::List,
            "Person",
            json!({}),
            json!({ "lastSeen_id": 0, "limit": 2 }),
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

    // LIST
    {
        // Act
        let (_, body) = execute_ok(
            &idl,
            SelectOperation::List,
            "Person",
            json!({}),
            // seek past the last id seen on page 1, returning the remainder.
            json!({ "lastSeen_id": 2, "limit": 2 }),
            &storage,
        )
        .await;

        // Assert
        assert_eq!(
            body,
            json!([{ "id": 3, "name": "Cara" }]),
            "Seeking past id `2` returns only the records after it"
        );
    }
}

#[sqlx::test]
async fn composite_pk_list_seek() {
    // Arrange
    let idl = src_to_idl(
        r#"
        d1 { db }

        model Order for db {
            primary {
                region: int
                num: int
            }
            column { total: int }
        }
        "#,
    );

    let mut storage = MockStorage::from_idl(&idl, &[]).await;
    for (region, num, total) in [(1, 10, 100), (1, 20, 200), (2, 5, 50)] {
        seed(
            &idl,
            "Order",
            json!({}),
            json!({ "region": region, "num": num, "total": total }),
            &mut storage,
        )
        .await;
    }

    // Act
    let (plan, body) = execute_ok(
        &idl,
        SelectOperation::List,
        "Order",
        json!({}),
        json!({ "lastSeen_region": 1, "lastSeen_num": 10, "limit": 10 }),
        &storage,
    )
    .await;

    // Assert
    assert_eq!(
        body,
        json!([
            { "region": 1, "num": 20, "total": 200 },
            { "region": 2, "num": 5, "total": 50 },
        ]),
        "The row-value cursor advances lexicographically: `(1,20)` and `(2,5)` sort after \
         `(1,10)`, but `(2,5)` is not excluded despite its smaller `num`"
    );
    assert_eq!(plan.stages.len(), 1);
    assert_eq!(plan.stages[0].steps.len(), 1);
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
    for (pid, did, name) in [(1, 10, "Fido"), (2, 20, "Rex")] {
        seed(
            &idl,
            "Person",
            json!({ "dog": {} }),
            json!({ "id": pid, "dogId": did, "dog": { "id": did, "name": name } }),
            &mut storage,
        )
        .await;
    }

    // GET
    {
        // Act
        let (plan, body) = execute_ok(
            &idl,
            SelectOperation::Get,
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
            SelectOperation::List,
            "Person",
            json!({ "dog": {} }),
            json!({ "lastSeen_id": 0, "limit": 10 }),
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
    for (uid, posts) in [
        (
            1,
            json!([{ "id": 100, "title": "a" }, { "id": 102, "title": "c" }]),
        ),
        (
            2,
            json!([{ "id": 101, "title": "b" }, { "id": 103, "title": "d" }]),
        ),
    ] {
        seed(
            &idl,
            "User",
            json!({ "posts": {} }),
            json!({ "id": uid, "posts": posts }),
            &mut storage,
        )
        .await;
    }

    // GET
    {
        // Act
        let (plan, body) = execute_ok(
            &idl,
            SelectOperation::Get,
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
            SelectOperation::List,
            "User",
            json!({ "posts": {} }),
            json!({ "lastSeen_id": 0, "limit": 10 }),
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
    for (pid, did, name) in [(1, 10, "Fido"), (2, 20, "Rex")] {
        seed(
            &idl,
            "Person",
            json!({ "dog": {} }),
            json!({ "id": pid, "dogId": did, "dog": { "id": did, "name": name } }),
            &mut storage,
        )
        .await;
    }

    // Act
    let (plan, body) = execute_ok(
        &idl,
        SelectOperation::List,
        "Person",
        json!({ "dog": {} }),
        json!({ "lastSeen_id": 0, "limit": 10 }),
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
    for (uid, did, tid, toy) in [(1, 10, 100, "Bone"), (2, 20, 200, "Ball")] {
        seed(
            &idl,
            "User",
            json!({ "dog": { "toy": {} } }),
            json!({
                "id": uid,
                "dogId": did,
                "dog": { "id": did, "toyId": tid, "toy": { "id": tid, "name": toy } }
            }),
            &mut storage,
        )
        .await;
    }

    // Act
    let (plan, body) = execute_ok(
        &idl,
        SelectOperation::List,
        "User",
        json!({ "dog": { "toy": {} } }),
        json!({ "lastSeen_id": 0, "limit": 10 }),
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
    for (pid, title) in [(1, "rust"), (2, "go")] {
        seed(
            &idl,
            "SubReddit",
            json!({}),
            json!({ "pid": pid, "title": title, "subId": 7 }),
            &mut storage,
        )
        .await;
    }

    // Act
    let (plan, body) = execute_ok(
        &idl,
        SelectOperation::Get,
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
    for (cid, tid, name) in [(1, 100, "Acme"), (2, 200, "Globex")] {
        seed(
            &idl,
            "Company",
            json!({ "tenant": {} }),
            json!({ "id": cid, "tenantId": tid, "tenant": { "pid": 1, "name": name } }),
            &mut storage,
        )
        .await;
    }

    // Act
    let (plan, body) = execute_ok(
        &idl,
        SelectOperation::List,
        "Company",
        json!({ "tenant": {} }),
        json!({ "lastSeen_id": 0, "limit": 10 }),
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
    seed(
        &idl,
        "SubReddit",
        json!({ "posts": {} }),
        json!({
            "subId": 9,
            "pid": 1,
            "posts": [ { "id": 10, "body": "hi" }, { "id": 11, "body": "yo" } ]
        }),
        &mut storage,
    )
    .await;

    // Act
    let (plan, body) = execute_ok(
        &idl,
        SelectOperation::Get,
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
    // The banners belong to no page in particular (an empty-form nav), so the first
    // page's graph carries both.
    seed(
        &idl,
        "Page",
        json!({ "banners": {} }),
        json!({
            "id": 10,
            "banners": [ { "id": 1, "text": "sale" }, { "id": 2, "text": "news" } ]
        }),
        &mut storage,
    )
    .await;
    seed(&idl, "Page", json!({}), json!({ "id": 20 }), &mut storage).await;

    // Act
    let (plan, body) = execute_ok(
        &idl,
        SelectOperation::List,
        "Page",
        json!({ "banners": {} }),
        json!({ "lastSeen_id": 0, "limit": 10 }),
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
    seed(
        &idl,
        "Order",
        json!({ "lines": {} }),
        json!({ "region": 1, "num": 100, "lines": [ { "id": 1 }, { "id": 3 } ] }),
        &mut storage,
    )
    .await;
    // A line outside the order's region, saved directly.
    seed(
        &idl,
        "Line",
        json!({}),
        json!({ "id": 2, "region": 2, "orderNum": 100 }),
        &mut storage,
    )
    .await;

    // Act
    let (plan, body) = execute_ok(
        &idl,
        SelectOperation::Get,
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
async fn composite_fk_nav_emits_row_value_in() {
    // Arrange
    let idl = src_to_idl(
        r#"
        d1 { db }

        model Student for db {
            primary {
                id: int
                name: string
            }
            many StudentCourse::{studentId(id), studentName(name)} { enrollments }
        }

        model StudentCourse for db {
            primary { id: int }
            column { studentId: int }
            column { studentName: string }
            one Student::{id(studentId), name(studentName)} { student }
        }
        "#,
    );

    // Act
    let plan = orm::query::select::planner::plan(
        SelectOperation::List,
        "StudentCourse",
        &idl,
        &tree(json!({ "student": {} })),
    );

    // Assert
    let (sql, arguments) = sql_step(&plan, "student");
    let literal = sql_literals(sql);
    assert!(
        literal.contains(r#"("id", "name") IN (VALUES "#),
        "composite nav should emit a row-value `(...) IN (VALUES ...)`, got: {literal}"
    );
    assert!(
        !literal.contains(r#""id" IN ("#) && !literal.contains(r#""name" IN ("#),
        "composite nav must not emit per-column scalar `IN`s, got: {literal}"
    );
    assert!(
        matches!(arguments, [SqlArgument::Tuple(group)] if group.len() == 2),
        "composite nav should bind a single width-2 tuple argument, got: {arguments:?}"
    );
}

#[sqlx::test]
async fn single_key_nav_keeps_scalar_in() {
    // Arrange: a single-key FK nav should be untouched by the composite change.
    let idl = src_to_idl(
        r#"
        d1 { db }

        model Owner for db {
            primary { id: int }
            many Pet::ownerId(id) { pets }
        }

        model Pet for db {
            primary { id: int }
            column { ownerId: int }
        }
        "#,
    );

    // Act
    let plan = orm::query::select::planner::plan(
        SelectOperation::List,
        "Owner",
        &idl,
        &tree(json!({ "pets": {} })),
    );

    // Assert: the single-key nav keeps the scalar `"ownerId" IN (...)` form and a Spread arg.
    let (sql, arguments) = sql_step(&plan, "pets");
    let literal = sql_literals(sql);
    assert!(
        literal.contains(r#""ownerId" IN ("#) && !literal.contains("VALUES"),
        "single-key nav should keep the scalar `IN` form, got: {literal}"
    );
    assert!(
        matches!(arguments, [SqlArgument::Spread(_)]),
        "single-key nav should bind one spread argument, got: {arguments:?}"
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
    seed(&idl, "Owner", json!({}), json!({ "id": 1 }), &mut storage).await;
    for pid in [30, 10, 20] {
        seed(
            &idl,
            "Pet",
            json!({}),
            json!({ "id": pid, "ownerId": 1 }),
            &mut storage,
        )
        .await;
    }

    // Act
    let (plan, body) = execute_ok(
        &idl,
        SelectOperation::Get,
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
    seed(
        &idl,
        "User",
        json!({ "avatar": {} }),
        json!({ "id": 1, "avatar": { "url": "u1" } }),
        &mut storage,
    )
    .await;

    // Act
    let (plan, body) = execute_ok(
        &idl,
        SelectOperation::Get,
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
    for (id, url) in [(1, "u1"), (2, "u2")] {
        seed(
            &idl,
            "User",
            json!({ "avatar": {} }),
            json!({ "id": id, "avatar": { "url": url } }),
            &mut storage,
        )
        .await;
    }

    // Act
    let (plan, body) = execute_ok(
        &idl,
        SelectOperation::List,
        "User",
        json!({ "avatar": {} }),
        json!({ "lastSeen_id": 0, "limit": 10 }),
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
    seed(
        &idl,
        "User",
        json!({ "avatar": {} }),
        json!({ "id": 1, "avatar": { "url": "u1" } }),
        &mut storage,
    )
    .await;

    // Act
    let (plan, body) = execute_ok(
        &idl,
        SelectOperation::Get,
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
    seed(
        &idl,
        "Item",
        json!({ "entry": {} }),
        json!({ "id": 1, "region": "us", "entry": { "raw": { "hits": 3 }, "metadata": null } }),
        &mut storage,
    )
    .await;

    // Act
    let (plan, body) = execute_ok(
        &idl,
        SelectOperation::Get,
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
            "entry": { "value": { "hits": 3 } }
        }),
        "A Workers KV read wraps the value keyed by the composite format"
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
    for (id, v) in [(1, "a"), (2, "b")] {
        seed(
            &idl,
            "Item",
            json!({ "entry": {} }),
            json!({ "id": id, "entry": { "raw": v, "metadata": null } }),
            &mut storage,
        )
        .await;
    }

    // Act
    let (plan, body) = execute_ok(
        &idl,
        SelectOperation::List,
        "Item",
        json!({ "entry": {} }),
        json!({ "lastSeen_id": 0, "limit": 10 }),
        &storage,
    )
    .await;

    // Assert
    assert_eq!(
        body,
        json!([
            { "id": 1, "entry": { "value": "a"} },
            { "id": 2, "entry": { "value": "b"} },
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
    seed(
        &idl,
        "Entry",
        json!({ "top": {} }),
        json!({ "id": 1, "score": 99, "top": [1, 2, 3], "tenantId": 7 }),
        &mut storage,
    )
    .await;

    // Act
    let (plan, body) = execute_ok(
        &idl,
        SelectOperation::Get,
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
    seed(
        &idl,
        "Entry",
        json!({ "banner": {}, "top": {} }),
        json!({ "id": 1, "top": [1, 2], "tenantId": 7, "banner": { "img": "b7" } }),
        &mut storage,
    )
    .await;

    // Act
    let (plan, body) = execute_ok(
        &idl,
        SelectOperation::Get,
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
    for (oid, tid, top) in [(1, 100, "acme"), (2, 200, "globex")] {
        seed(
            &idl,
            "Org",
            json!({ "board": { "top": {} } }),
            json!({ "id": oid, "tenantId": tid, "board": { "pid": 1, "top": top } }),
            &mut storage,
        )
        .await;
    }

    // Act
    let (plan, body) = execute_ok(
        &idl,
        SelectOperation::List,
        "Org",
        json!({ "board": { "top": {} } }),
        json!({ "lastSeen_id": 0, "limit": 10 }),
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
    seed(
        &idl,
        "Settings",
        json!({ "config": {} }),
        json!({ "id": 1, "config": { "theme": "dark" } }),
        &mut storage,
    )
    .await;

    // Act
    let (plan, body) = execute_ok(
        &idl,
        SelectOperation::Get,
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
    seed(
        &idl,
        "Entry",
        json!({ "snapshot": {} }),
        json!({ "id": 1, "tenantId": 7, "snapshot": { "url": "s1" } }),
        &mut storage,
    )
    .await;

    // Act
    let (plan, body) = execute_ok(
        &idl,
        SelectOperation::Get,
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
    seed(
        &idl,
        "Board",
        json!({ "widgets": {} }),
        json!({ "pid": 1, "tenantId": 7, "widgets": [ { "id": 10 }, { "id": 11 } ] }),
        &mut storage,
    )
    .await;
    // A widget for another tenant, saved directly.
    seed(
        &idl,
        "Widget",
        json!({}),
        json!({ "id": 12, "tenantId": 8 }),
        &mut storage,
    )
    .await;

    // Act
    let (plan, body) = execute_ok(
        &idl,
        SelectOperation::Get,
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
    seed(
        &idl,
        "Owner",
        json!({ "entry": {} }),
        json!({ "ownerId": "o1", "entry": { "raw": "meta", "metadata": null } }),
        &mut storage,
    )
    .await;

    // Act
    let (plan, body) = execute_ok(
        &idl,
        SelectOperation::Get,
        "Owner",
        json!({ "entry": {} }),
        json!({ "ownerId": "o1" }),
        &storage,
    )
    .await;

    // Assert
    assert_eq!(
        body,
        json!({ "ownerId": "o1", "entry": { "value": "meta" } }),
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
    seed(
        &idl,
        "Board",
        json!({ "top": {} }),
        json!({ "tenantId": 7, "top": [9, 8, 7] }),
        &mut storage,
    )
    .await;

    // Act
    let (plan, body) = execute_ok(
        &idl,
        SelectOperation::Get,
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
    seed(
        &idl,
        "Owner",
        json!({ "dog": {} }),
        json!({ "dogId": 10, "dog": { "id": 10, "name": "Fido" } }),
        &mut storage,
    )
    .await;
    // A decoy dog no owner points at, saved directly.
    seed(
        &idl,
        "Dog",
        json!({}),
        json!({ "id": 20, "name": "Rex" }),
        &mut storage,
    )
    .await;

    // Act
    let (plan, body) = execute_ok(
        &idl,
        SelectOperation::Get,
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
    seed(
        &idl,
        "Owner",
        json!({}),
        json!({ "id": 1, "groupRef": "g1" }),
        &mut storage,
    )
    .await;

    // Act
    let (plan, body) = execute_ok(
        &idl,
        SelectOperation::Get,
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
    seed(
        &idl,
        "Ledger",
        json!({}),
        json!({ "id": 1, "region": "us" }),
        &mut storage,
    )
    .await;

    // Act
    let (plan, body) = execute_ok(
        &idl,
        SelectOperation::Get,
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
    assert_eq!(plan.stages[0].steps.len(), 1,);
}

#[sqlx::test]
async fn route_field_on_sql_nav_child() {
    // Arrange
    let idl = src_to_idl(
        r#"
        d1 { db }

        model Parent for db {
            primary { id: int }
            foreign Child::id { childId }
            column { region: string }
            one Child::{ id(childId), region(region) } { child }
        }

        model Child for db {
            primary { id: int }
            route { region: string }
        }
        "#,
    );

    let mut storage = MockStorage::from_idl(&idl, &[]).await;
    for (id, cid, region) in [(1, 10, "us"), (2, 20, "eu")] {
        seed(
            &idl,
            "Parent",
            json!({ "child": {} }),
            json!({ "id": id, "childId": cid, "region": region, "child": { "id": cid } }),
            &mut storage,
        )
        .await;
    }

    // Act
    let (plan, body) = execute_ok(
        &idl,
        SelectOperation::List,
        "Parent",
        json!({ "child": {} }),
        json!({ "lastSeen_id": 0, "limit": 10 }),
        &storage,
    )
    .await;

    // Assert
    assert_eq!(
        body,
        json!([
            { "id": 1, "childId": 10, "region": "us", "child": { "id": 10, "region": "us" } },
            { "id": 2, "childId": 20, "region": "eu", "child": { "id": 20, "region": "eu" } },
        ]),
        "The fetched child keeps its `id` and gains `region`; the route field must \
         ride onto the selected rows"
    );
    assert_eq!(
        plan.stages.len(),
        2,
        "`child` relies on `Parent::childId` and `Parent::region` which are only known after the first stage"
    );
    assert_eq!(plan.stages[0].steps.len(), 1);
    assert_eq!(plan.stages[1].steps.len(), 1);
}

#[sqlx::test]
async fn kv_key_straddles_nav_inherited_and_local_fields() {
    // Arrange
    let idl = src_to_idl(
        r#"
        d1 { db }

        durable BoardDo {
            shard { tenantId: int }
        }

        kv Cache {
            entry(tenantId: int, itemId: int) -> json {
                "e/{tenantId}/{itemId}"
            }
        }

        model Org for db {
            primary { id: int }
            column { tenantId: int }
            one Board::tenantId(tenantId) { board }
        }

        model Board for BoardDo(tenantId) {
            primary { pid: int }
            column { itemId: int }
            kv Cache::entry(tenantId, itemId) { entry }
        }
        "#,
    );

    let mut storage = MockStorage::from_idl(
        &idl,
        &[("BoardDo", vec![vec![json!(100)], vec![json!(200)]])],
    )
    .await;
    for (oid, tid, item, entry) in [(1, 100, 1, "a"), (2, 200, 2, "b")] {
        seed(
            &idl,
            "Org",
            json!({ "board": { "entry": {} } }),
            json!({
                "id": oid,
                "tenantId": tid,
                "board": { "pid": 1, "itemId": item, "entry": { "raw": entry, "metadata": null } }
            }),
            &mut storage,
        )
        .await;
    }

    // Act
    let (plan, body) = execute_ok(
        &idl,
        SelectOperation::List,
        "Org",
        json!({ "board": { "entry": {} } }),
        json!({ "lastSeen_id": 0, "limit": 10 }),
        &storage,
    )
    .await;

    // Assert
    assert_eq!(
        body,
        json!([
            {
                "id": 1, "tenantId": 100,
                "board": { "pid": 1, "itemId": 1, "tenantId": 100, "entry": { "value": "a" } }
            },
            {
                "id": 2, "tenantId": 200,
                "board": { "pid": 1, "itemId": 2, "tenantId": 200, "entry": { "value": "b" } }
            },
        ]),
        "`entry`'s key pairs each board's own `itemId` with the `tenantId` it inherited \
         from its owning org, without a cross product between mismatched boards/orgs \
         (only `e/100/1` and `e/200/2` are seeded; a cross product would request the \
         unseeded `e/100/2` / `e/200/1` and panic)"
    );
    assert_eq!(
        plan.stages.len(),
        3,
        "Org at stage 0; `board` at stage 1 (needs Org's tenantId); `entry` at stage 2 \
         (needs Board's own itemId — its inherited tenantId is already known from stage 0, \
         so straddling tables costs no extra staging)"
    );
}
