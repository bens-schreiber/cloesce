mod common;

use common::setup::{MockStorage, tree};
use compiler_test::src_to_idl;
use idl::CloesceIdl;
use orm::query::save::plan::{PathSegment, SaveArg, SavePlan, SaveQuery, SqlStatement};
use orm::query::save::planner::plan;
use serde_json::{Value, json};

fn batches<'a>(plan: &'a SavePlan, stage: usize, step: usize) -> &'a [SqlStatement<'a>] {
    match &plan.stages[stage].steps[step].query {
        SaveQuery::SqlBatch { statements, .. } => statements,
        other => panic!("expected SqlBatch, got {other:?}"),
    }
}

fn write_sql<'a>(stmt: &'a SqlStatement) -> &'a str {
    match stmt {
        SqlStatement::Write { sql, .. } | SqlStatement::Hydrate { sql, .. } => sql,
    }
}

async fn save_ok<'idl>(
    idl: &'idl CloesceIdl<'_>,
    model: &str,
    include: Value,
    payload: Value,
    storage: &mut MockStorage,
) -> (SavePlan<'idl>, Value) {
    let payload: &'static Value = Box::leak(Box::new(payload));
    let plan = plan(model, idl, &tree(include), payload).expect("plan to succeed");
    let body = common::save_executor::execute(&plan, storage).await;
    (plan, body)
}

#[sqlx::test]
async fn save_scalar_with_pk_upserts() {
    // Arrange
    let idl = src_to_idl(
        r#"
        d1 { db }

        model Horse for db {
            primary { id: int }
            column { name: string }
        }
        "#,
    );

    let mut storage = MockStorage::from_idl(&idl, &[]).await;

    // Insert
    {
        // Act
        let (plan, body) = save_ok(
            &idl,
            "Horse",
            json!({}),
            json!({ "id": 1, "name": "Spirit" }),
            &mut storage,
        )
        .await;

        // Assert
        assert_eq!(plan.stages.len(), 1);
        assert_eq!(plan.stages[0].steps.len(), 1, "one SqlBatch");
        assert_eq!(
            body,
            json!({ "id": 1, "name": "Spirit" }),
            "the response is the read-back row"
        );
    }

    // Update
    {
        let (_, body) = save_ok(
            &idl,
            "Horse",
            json!({}),
            json!({ "id": 1, "name": "Rain" }),
            &mut storage,
        )
        .await;
        assert_eq!(
            body,
            json!({ "id": 1, "name": "Rain" }),
            "On conflict update works"
        );
    }
}

#[sqlx::test]
async fn save_auto_increment_pk() {
    // Arrange
    let idl = src_to_idl(
        r#"
        d1 { db }

        model Horse for db {
            primary { id: int }
            column { name: string }
        }
        "#,
    );
    let mut storage = MockStorage::from_idl(&idl, &[]).await;

    // Act
    let (plan, body) = save_ok(
        &idl,
        "Horse",
        json!({}),
        json!({ "name": "Spirit" }),
        &mut storage,
    )
    .await;

    // Assert
    let stmts = batches(&plan, 0, 0);
    assert_eq!(stmts.len(), 4, "insert, tmp capture, hydrate, tmp delete");
    assert_eq!(
        write_sql(&stmts[0]),
        r#"INSERT INTO "Horse" ("name") VALUES (?1)"#
    );
    assert_eq!(
        write_sql(&stmts[1]),
        r#"INSERT OR REPLACE INTO "$cloesce_tmp" ("path", "primary_key") VALUES ('', json_object('id', last_insert_rowid()))"#
    );
    assert_eq!(
        write_sql(&stmts[2]),
        r#"SELECT "id", "name" FROM "Horse" WHERE "id" = (SELECT json_extract("primary_key", '$.id') FROM "$cloesce_tmp" WHERE "path" = '')"#
    );
    assert_eq!(write_sql(&stmts[3]), r#"DELETE FROM "$cloesce_tmp""#);

    assert_eq!(
        body,
        json!({ "id": 1, "name": "Spirit" }),
        "the generated id `1` comes back through the read-back hydrate"
    );
}

#[sqlx::test]
async fn save_one_to_one_same_db() {
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

    // Act
    let (plan, body) = save_ok(
        &idl,
        "Person",
        json!({ "dog": {} }),
        // dog has no PK, person's dogId resolves via in-batch tmp subquery
        json!({ "id": 5, "dog": { "name": "Fido" } }),
        &mut storage,
    )
    .await;

    // Assert
    assert_eq!(plan.stages.len(), 1);
    assert_eq!(plan.stages[0].steps.len(), 1, "one SqlBatch");

    let stmts = batches(&plan, 0, 0);
    assert_eq!(stmts.len(), 6);
    assert_eq!(
        write_sql(&stmts[2]),
        r#"INSERT INTO "Person" ("dogId", "id") VALUES ((SELECT json_extract("primary_key", '$.id') FROM "$cloesce_tmp" WHERE "path" = 'dog'), ?1) ON CONFLICT ("id") DO UPDATE SET "dogId" = "excluded"."dogId""#,
        "person's dogId is the dog's generated id (target column `id`) via the in-batch tmp subquery"
    );

    assert_eq!(
        body,
        json!({ "id": 5, "dogId": 1, "dog": { "id": 1, "name": "Fido" } }),
        "person holds the dog's generated FK; the dog is hydrated at its path"
    );
}

#[sqlx::test]
async fn save_one_to_many_same_db() {
    // Arrange
    let idl = src_to_idl(
        r#"
        d1 { db }

        model User for db {
            primary { id: int }
            many Dog::userId(id) { dogs }
        }

        model Dog for db {
            primary { id: int }
            foreign User::id { userId }
            column { name: string }
        }
        "#,
    );
    let mut storage = MockStorage::from_idl(&idl, &[]).await;

    // Act
    let (plan, body) = save_ok(
        &idl,
        "User",
        json!({ "dogs": {} }),
        json!({ "id": 1, "dogs": [ { "name": "A" }, { "name": "B" } ] }),
        &mut storage,
    )
    .await;

    // Assert
    assert_eq!(plan.stages.len(), 1);
    assert_eq!(plan.stages[0].steps.len(), 1);

    assert_eq!(
        body,
        json!({
            "id": 1,
            "dogs": [
                { "id": 1, "userId": 1, "name": "A" },
                { "id": 2, "userId": 1, "name": "B" },
            ]
        }),
        "the two dogs keep their payload order and each gets a generated id"
    );
}

#[sqlx::test]
async fn save_composite_pk() {
    // Arrange
    let idl = src_to_idl(
        r#"
        d1 { db }

        model Enrollment for db {
            primary {
                studentId: int
                courseId: int
            }
            column { grade: string }
        }
        "#,
    );
    let mut storage = MockStorage::from_idl(&idl, &[]).await;

    // Act
    let (plan, body) = save_ok(
        &idl,
        "Enrollment",
        json!({}),
        json!({ "studentId": 1, "courseId": 2, "grade": "A" }),
        &mut storage,
    )
    .await;

    // Assert
    let stmts = batches(&plan, 0, 0);
    assert_eq!(stmts.len(), 2, "insert + hydrate, no tmp");
    assert_eq!(
        write_sql(&stmts[0]),
        r#"INSERT INTO "Enrollment" ("grade", "studentId", "courseId") VALUES (?1, ?2, ?3) ON CONFLICT ("studentId", "courseId") DO UPDATE SET "grade" = "excluded"."grade""#
    );
    assert_eq!(body, json!({ "studentId": 1, "courseId": 2, "grade": "A" }));
}

#[sqlx::test]
async fn save_composite_pk_missing_errors() {
    // Arrange
    let idl = src_to_idl(
        r#"
        d1 { db }

        model Enrollment for db {
            primary {
                studentId: int
                courseId: int
            }
            column { grade: string }
        }
        "#,
    );

    // Act
    let payload: &'static Value = Box::leak(Box::new(json!({ "grade": "A" })));
    let err = plan("Enrollment", &idl, &tree(json!({})), payload).unwrap_err();

    // Assert
    assert!(
        matches!(err, orm::OrmErrorKind::ModelKeyCannotAutoIncrement { .. }),
        "a missing composite PK errors, got {err:?}"
    );
}

#[sqlx::test]
async fn save_junction_table_composite_fk_pk() {
    // Arrange
    let idl = src_to_idl(
        r#"
        d1 { db }

        model Post for db {
            primary { id: int }
            column { title: string }
            many PostTag::postId(id) { tags }
        }

        model PostTag for db {
            primary {
                foreign Post::id { postId }
                tagId: int
            }
        }
        "#,
    );
    let mut storage = MockStorage::from_idl(&idl, &[]).await;

    // Act
    let (_, body) = save_ok(
        &idl,
        "Post",
        json!({ "tags": {} }),
        json!({ "title": "Hi", "tags": [ { "tagId": 10 }, { "tagId": 20 } ] }),
        &mut storage,
    )
    .await;

    // Assert
    assert_eq!(
        body,
        json!({
            "id": 1,
            "title": "Hi",
            "tags": [
                { "postId": 1, "tagId": 10 },
                { "postId": 1, "tagId": 20 },
            ]
        }),
        "the junction rows carry the generated postId and their own tagId"
    );
}

#[sqlx::test]
async fn save_partial_update() {
    // Arrange
    let idl = src_to_idl(
        r#"
        d1 { db }

        model Horse for db {
            primary { id: int }
            column { name: string }
            column { age: int }
        }
        "#,
    );
    let mut storage = MockStorage::from_idl(&idl, &[]).await;
    save_ok(
        &idl,
        "Horse",
        json!({}),
        json!({ "id": 1, "name": "Spirit", "age": 3 }),
        &mut storage,
    )
    .await;

    // Act
    let (plan, body) = save_ok(
        &idl,
        "Horse",
        json!({}),
        // missing nullable column => update
        json!({ "id": 1, "name": "Rain" }),
        &mut storage,
    )
    .await;

    // Assert
    let stmts = batches(&plan, 0, 0);
    assert_eq!(
        write_sql(&stmts[0]),
        r#"UPDATE "Horse" SET "name" = ?1 WHERE "id" = ?2"#
    );
    assert_eq!(
        body,
        json!({ "id": 1, "name": "Rain", "age": 3 }),
        "the read-back returns full DB truth, not a payload echo"
    );
}

#[sqlx::test]
async fn save_missing_required_field_errors() {
    // Arrange
    let idl = src_to_idl(
        r#"
        d1 { db }

        model Horse for db {
            primary { id: int }
            column { name: string }
        }
        "#,
    );

    // Act
    let payload = json!({});
    let err = plan("Horse", &idl, &tree(json!({})), &payload).unwrap_err();

    // Assert
    assert!(
        matches!(err, orm::OrmErrorKind::MissingField { .. }),
        "a missing required non-nullable column on an insert errors, got {err:?}"
    );
}

#[sqlx::test]
async fn save_include_tree_gates_navs() {
    // Arrange
    let idl = src_to_idl(
        r#"
        d1 { db }

        model User for db {
            primary { id: int }
            many Dog::userId(id) { dogs }
        }

        model Dog for db {
            primary { id: int }
            foreign User::id { userId }
            column { name: string }
        }
        "#,
    );
    let mut storage = MockStorage::from_idl(&idl, &[]).await;

    // Act
    let (_, body) = save_ok(
        &idl,
        "User",
        json!({}),
        json!({ "id": 1, "dogs": [ { "name": "A" } ] }),
        &mut storage,
    )
    .await;

    // Assert
    assert_eq!(
        body,
        json!({ "id": 1 }),
        "an un-included nav is neither written nor present in the body"
    );
    let dog_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM Dog")
        .fetch_one(storage.d1.get("db").unwrap())
        .await
        .unwrap();
    assert_eq!(dog_count, 0, "the gated dog was never inserted");
}

#[sqlx::test]
async fn save_cross_db_child_concrete_fk() {
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

    // Act
    let (plan, body) = save_ok(
        &idl,
        "Person",
        json!({ "dog": {} }),
        json!({ "id": 1, "dogId": 10, "dog": { "id": 10, "name": "Fido" } }),
        &mut storage,
    )
    .await;

    // Assert
    assert_eq!(
        plan.stages.len(),
        1,
        "both batches in stage 0 (concrete FKs)"
    );
    assert_eq!(plan.stages[0].steps.len(), 2, "one batch per database");
    assert_eq!(
        body,
        json!({ "id": 1, "dogId": 10, "dog": { "id": 10, "name": "Fido" } })
    );
}

#[sqlx::test]
async fn save_cross_db_child_generated_fk() {
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

    // Act
    let (plan, body) = save_ok(
        &idl,
        "Person",
        json!({ "dog": {} }),
        json!({ "id": 1, "dog": { "name": "Fido" } }),
        &mut storage,
    )
    .await;

    // Assert
    assert_eq!(plan.stages.len(), 2, "person waits on the dog's read-back");
    assert_eq!(
        body,
        json!({ "id": 1, "dogId": 1, "dog": { "id": 1, "name": "Fido" } }),
        "person's dogId is the dog's generated id, read back across databases"
    );
}

#[sqlx::test]
async fn save_do_root() {
    // Arrange
    let idl = src_to_idl(
        r#"
        durable SubRedditDo {
            shard { subId: int }
        }

        model SubReddit for SubRedditDo(subId) {
            primary { pid: int }
            column { title: string }
            route { note: string }
        }
        "#,
    );
    let mut storage = MockStorage::from_idl(&idl, &[("SubRedditDo", vec![vec![json!(7)]])]).await;

    // Act
    let (plan, body) = save_ok(
        &idl,
        "SubReddit",
        json!({}),
        json!({ "pid": 1, "title": "rust", "subId": 7, "note": "hi" }),
        &mut storage,
    )
    .await;

    // Assert
    match &plan.stages[0].steps[0].query {
        SaveQuery::SqlBatch {
            shard, database, ..
        } => {
            assert_eq!(database.name, "SubRedditDo");
            assert_eq!(shard.len(), 1);
            assert_eq!(shard[0].0, "subId");
        }
        other => panic!("expected SqlBatch, got {other:?}"),
    }
    assert_eq!(
        body,
        json!({ "pid": 1, "title": "rust", "note": "hi", "subId": 7 }),
        "the DO row carries its non-shard route field `note` and its tagged shard `subId`"
    );
}

#[sqlx::test]
async fn save_do_child_shard_from_parent() {
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
            many Tenant::{ companyId(id), tenantId(tenantId) } { tenants }
        }

        model Tenant for TenantDo(tenantId) {
            primary { pid: int }
            column { companyId: int }
            column { name: string }
        }
        "#,
    );
    let mut storage = MockStorage::from_idl(&idl, &[("TenantDo", vec![vec![json!(100)]])]).await;

    // Act
    let (plan, body) = save_ok(
        &idl,
        "Company",
        json!({ "tenants": {} }),
        json!({
            "id": 1,
            "tenantId": 100,
            "tenants": [
                { "pid": 1, "name": "Acme" },
                { "pid": 2, "name": "Globex" },
            ]
        }),
        &mut storage,
    )
    .await;

    // Assert
    let do_batches = plan
        .stages
        .iter()
        .flat_map(|s| &s.steps)
        .filter(|step| matches!(&step.query, SaveQuery::SqlBatch { database, .. } if database.name == "TenantDo"))
        .count();
    assert_eq!(
        do_batches, 1,
        "both tenants route to the same stub -> one DO batch"
    );

    assert_eq!(
        body,
        json!({
            "id": 1,
            "tenantId": 100,
            "tenants": [
                { "pid": 1, "companyId": 1, "name": "Acme", "tenantId": 100 },
                { "pid": 2, "companyId": 1, "name": "Globex", "tenantId": 100 },
            ]
        }),
    );
}

#[sqlx::test]
async fn save_kv_parallel() {
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

    // Act
    let (plan, body) = save_ok(
        &idl,
        "Item",
        json!({ "entry": {} }),
        json!({ "id": 1, "entry": { "raw": { "hits": 3 }, "metadata": null } }),
        &mut storage,
    )
    .await;

    // Assert
    assert_eq!(
        plan.stages.len(),
        1,
        "parallel KV write shares the row's stage"
    );
    assert_eq!(
        storage.kv.get("Cache").unwrap().get("e/1"),
        Some(&json!({ "hits": 3 }))
    );
    assert_eq!(body, json!({ "id": 1, "entry": { "hits": 3 } }));
}

#[sqlx::test]
async fn save_kv_delayed() {
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

    // Act
    let (plan, body) = save_ok(
        &idl,
        "Item",
        json!({ "entry": {} }),
        json!({ "entry": { "raw": { "hits": 9 }, "metadata": null } }),
        &mut storage,
    )
    .await;

    // Assert
    assert_eq!(
        plan.stages.len(),
        2,
        "delayed KV write waits for the generated id"
    );
    assert_eq!(
        storage.kv.get("Cache").unwrap().get("e/1"),
        Some(&json!({ "hits": 9 }))
    );
    assert_eq!(body, json!({ "id": 1, "entry": { "hits": 9 } }));
}

#[sqlx::test]
async fn save_kv_object_unwrap() {
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

    // Act
    let (_, body) = save_ok(
        &idl,
        "Item",
        json!({ "entry": {} }),
        json!({ "id": 1, "entry": { "raw": { "hits": 5 }, "metadata": { "v": 1 } } }),
        &mut storage,
    )
    .await;

    // Assert
    assert_eq!(
        storage.kv.get("Cache").unwrap().get("e/1"),
        Some(&json!({ "hits": 5 })),
        "the unwrapped `raw` value is written to KV"
    );
    assert_eq!(body, json!({ "id": 1, "entry": { "hits": 5 } }));
}

#[sqlx::test]
async fn save_do_kv_field() {
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

    // Act
    let (_, body) = save_ok(
        &idl,
        "Entry",
        json!({ "top": {} }),
        json!({ "id": 1, "score": 99, "top": [1, 2, 3], "tenantId": 7 }),
        &mut storage,
    )
    .await;

    // Assert
    assert_eq!(
        storage
            .durable_kv
            .get("BoardDo")
            .unwrap()
            .get(&vec![json!(7)])
            .unwrap()
            .get("top"),
        Some(&json!([1, 2, 3]))
    );
    assert_eq!(
        body,
        json!({ "id": 1, "score": 99, "tenantId": 7, "top": [1, 2, 3] })
    );
}

#[sqlx::test]
async fn save_r2_parallel() {
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

    // Act
    let (plan, body) = save_ok(
        &idl,
        "User",
        json!({ "avatar": {} }),
        json!({ "id": 1, "avatar": { "url": "u1" } }),
        &mut storage,
    )
    .await;

    // Assert
    assert_eq!(
        plan.stages.len(),
        1,
        "parallel R2 write shares the row's stage"
    );
    assert_eq!(
        storage.r2.get("Bucket").unwrap().get("avatars/1"),
        Some(&json!({ "url": "u1" }))
    );
    assert_eq!(body, json!({ "id": 1, "avatar": { "url": "u1" } }));
}

#[sqlx::test]
async fn save_r2_delayed() {
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

    // Act
    let (plan, body) = save_ok(
        &idl,
        "User",
        json!({ "avatar": {} }),
        json!({ "avatar": { "url": "u1" } }),
        &mut storage,
    )
    .await;

    // Assert
    assert_eq!(
        plan.stages.len(),
        2,
        "delayed R2 write waits for the generated id"
    );
    assert_eq!(
        storage.r2.get("Bucket").unwrap().get("avatars/1"),
        Some(&json!({ "url": "u1" }))
    );
    assert_eq!(body, json!({ "id": 1, "avatar": { "url": "u1" } }));
}

#[sqlx::test]
async fn save_backingless_root() {
    // Arrange
    let idl = src_to_idl(
        r#"
        d1 { db }

        kv Cache {
            entry(ownerId: string) -> json {
                "e/{ownerId}"
            }
        }

        model Owner {
            route { ownerId: string }
            route { dogId: int }
            kv Cache::entry(ownerId) { entry }
            one Dog::id(dogId) { dog }
        }

        model Dog for db {
            primary { id: int }
            column { name: string }
        }
        "#,
    );
    let mut storage = MockStorage::from_idl(&idl, &[]).await;

    // Act
    let (_, body) = save_ok(
        &idl,
        "Owner",
        json!({ "entry": {}, "dog": {} }),
        json!({
            "ownerId": "o1",
            "dogId": 10,
            "entry": { "raw": { "seen": true }, "metadata": null },
            "dog": { "id": 10, "name": "Fido" }
        }),
        &mut storage,
    )
    .await;

    // Assert
    assert_eq!(
        storage.kv.get("Cache").unwrap().get("e/o1"),
        Some(&json!({ "seen": true }))
    );
    assert_eq!(
        body,
        json!({
            "ownerId": "o1",
            "dogId": 10,
            "entry": { "seen": true },
            "dog": { "id": 10, "name": "Fido" }
        }),
    );
}

#[sqlx::test]
async fn save_response_shape() {
    // Arrange
    let idl = src_to_idl(
        r#"
        d1 { db }

        model User for db {
            primary { id: int }
            foreign Dog::id { dogId }
            one Dog::id(dogId) { dog }
            many Post::userId(id) { posts }
        }

        model Dog for db {
            primary { id: int }
            column { name: string }
        }

        model Post for db {
            primary { id: int }
            foreign User::id { userId }
            column { title: string }
        }
        "#,
    );
    let mut storage = MockStorage::from_idl(&idl, &[]).await;

    // Act
    let (_, body) = save_ok(
        &idl,
        "User",
        json!({ "dog": {}, "posts": {} }),
        json!({
            "dog": { "name": "Fido" },
            "posts": [ { "title": "a" }, { "title": "b" } ]
        }),
        &mut storage,
    )
    .await;

    // Assert
    assert_eq!(
        body,
        json!({
            "id": 1,
            "dogId": 1,
            "dog": { "id": 1, "name": "Fido" },
            "posts": [
                { "id": 1, "userId": 1, "title": "a" },
                { "id": 2, "userId": 1, "title": "b" },
            ]
        }),
        "the response shape is the full graph, in payload order"
    );
}

#[sqlx::test]
async fn save_one_nav_do_child_shard_from_parent() {
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
    let mut storage = MockStorage::from_idl(&idl, &[("TenantDo", vec![vec![json!(100)]])]).await;

    // Act
    let (_, body) = save_ok(
        &idl,
        "Company",
        json!({ "tenant": {} }),
        json!({ "id": 1, "tenantId": 100, "tenant": { "pid": 1, "name": "Acme" } }),
        &mut storage,
    )
    .await;

    // Assert
    assert_eq!(
        body,
        json!({
            "id": 1,
            "tenantId": 100,
            "tenant": { "pid": 1, "name": "Acme", "tenantId": 100 },
        }),
    );
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM Tenant")
        .fetch_one(
            storage
                .durable
                .get("TenantDo")
                .unwrap()
                .get(&vec![json!(100)])
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(count, 1, "the tenant row was written to stub 100");
}

#[sqlx::test]
async fn save_reversed_one_nav() {
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

    // Act
    let (_, body) = save_ok(
        &idl,
        "Owner",
        json!({ "pet": {} }),
        json!({ "id": 1, "pet": { "id": 10 } }),
        &mut storage,
    )
    .await;

    // Assert
    assert_eq!(
        body,
        json!({ "id": 1, "pet": { "id": 10, "ownerId": 1 } }),
        "the pet's `ownerId` FK is filled from the owner's provided PK"
    );
}

#[sqlx::test]
async fn save_backingless_many_nav() {
    // Arrange
    let idl = src_to_idl(
        r#"
        d1 { db }

        model Owner {
            route { ownerId: int }
            many Dog::ownerId(ownerId) { dogs }
        }

        model Dog for db {
            primary { id: int }
            column { ownerId: int }
            column { name: string }
        }
        "#,
    );
    let mut storage = MockStorage::from_idl(&idl, &[]).await;

    // Act
    let (_, body) = save_ok(
        &idl,
        "Owner",
        json!({ "dogs": {} }),
        json!({ "ownerId": 7, "dogs": [ { "name": "A" }, { "name": "B" } ] }),
        &mut storage,
    )
    .await;

    // Assert
    assert_eq!(
        body,
        json!({
            "ownerId": 7,
            "dogs": [
                { "id": 1, "ownerId": 7, "name": "A" },
                { "id": 2, "ownerId": 7, "name": "B" },
            ]
        }),
        "each array element is written with the route-param FK and hydrated in payload order"
    );
}

#[sqlx::test]
async fn save_nav_child_route_field_from_parent() {
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

    // Act
    let (_, body) = save_ok(
        &idl,
        "Parent",
        json!({ "child": {} }),
        json!({ "id": 1, "childId": 10, "region": "us", "child": { "id": 10 } }),
        &mut storage,
    )
    .await;

    // Assert
    assert_eq!(
        body,
        json!({
            "id": 1,
            "childId": 10,
            "region": "us",
            "child": { "id": 10, "region": "us" },
        }),
        "the child's `region` route field is synthesized from the parent's payload value"
    );
}

#[sqlx::test]
async fn save_two_generated_deps_one_joined() {
    // Arrange
    let idl = src_to_idl(
        r#"
        d1 { animals }
        d1 { pets }

        model Owner for pets {
            primary { id: int }
            column { dogId: int }
            column { catId: int }
            one Dog::id(dogId) { dog }
            one Cat::id(catId) { cat }
        }

        model Dog for animals {
            primary { id: int }
            column { name: string }
        }

        model Cat for pets {
            primary { id: int }
            column { name: string }
        }
        "#,
    );
    let mut storage = MockStorage::from_idl(&idl, &[]).await;

    // Act
    let (plan, _body) = save_ok(
        &idl,
        "Owner",
        json!({ "dog": {}, "cat": {} }),
        json!({ "id": 1, "dog": { "name": "Fido" }, "cat": { "name": "Felix" } }),
        &mut storage,
    )
    .await;

    assert_body_deps_lift_stage(&plan);
}

/// Fail if any `SaveArg::Body(path)` is consumed in the same stage as (or earlier than) the
/// batch whose `Hydrate` writes that `path`.
fn assert_body_deps_lift_stage(plan: &SavePlan) {
    // Map every hydrated body path -> the stage that produces it.
    let mut produced_at: Vec<(Vec<PathSegment>, usize)> = Vec::new();
    for (stage, s) in plan.stages.iter().enumerate() {
        for step in &s.steps {
            if let SaveQuery::SqlBatch { statements, .. } = &step.query {
                for st in statements {
                    if let SqlStatement::Hydrate { result, .. } = st {
                        produced_at.push((result.clone(), stage));
                    }
                }
            }
        }
    }

    for (stage, s) in plan.stages.iter().enumerate() {
        for step in &s.steps {
            let SaveQuery::SqlBatch { statements, .. } = &step.query else {
                continue;
            };
            let args = statements.iter().flat_map(|st| match st {
                SqlStatement::Write { arguments, .. } | SqlStatement::Hydrate { arguments, .. } => {
                    arguments.iter()
                }
            });
            for arg in args {
                let SaveArg::Result(path) = arg else { continue };
                // A `SaveArg::Body(path)` reads a hydrated instance's column: `path` is that
                // instance's hydrate `result` plus the trailing PK column. The producer is the
                // hydrate with the LONGEST result path that is a prefix of `path` (the most
                // specific instance — the root hydrate `[]` is a prefix of everything and must
                // not shadow it).
                let producer = produced_at
                    .iter()
                    .filter(|(hp, _)| path.starts_with(hp) && hp.len() < path.len())
                    .max_by_key(|(hp, _)| hp.len())
                    .map(|(_, s)| *s)
                    .unwrap_or_else(|| panic!("no hydrate produces body path {path:?}"));
                assert!(
                    stage > producer,
                    "a batch in stage {stage} binds SaveArg::Body({path:?}), produced by a \
                     read-back in stage {producer}; it must run in a strictly later stage \
                     (same-stage steps run in parallel)"
                );
            }
        }
    }
}
