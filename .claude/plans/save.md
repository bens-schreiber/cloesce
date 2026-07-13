# SAVE Operation for the Query Planner

TODO: IT may be best if the Save Planner and Select Planner are not separate IRs but share a single IR. This is because the save planner will have to generate select stages and steps to read back the saved data, which would use the select IR types. This would also be nice because one single IR can do both select and save at the same time in parallel.

## Context

The new query planner (`src/compiler/orm/src/query/`) supports GET/LIST; `Operation::Save` is a `todo!()`. We now implement SAVE, reproducing the legacy SeaQuery upsert's semantics (`src/compiler/orm/src/upsert.rs` — reference only, do not modify) but spanning D1, DO, KV, and R2. No SeaQuery — in-house SQL strings.

**Decisions made with the user:**
1. **Payload-dependent planning** — the save planner receives the actual payload (`&'src serde_json::Value`); it sees array lengths, which PKs are present (INSERT vs ON CONFLICT upsert vs partial UPDATE), and which KV/R2 keys resolve immediately.
2. **Batch per database** — a step carries an ordered statement list executed as one transaction (D1 batch semantics); `$cloesce_tmp` carries generated PKs *within* a batch; cross-database/storage writes go to later stages.
3. **Each batch ends with SELECTs** — the response body is hydrated by read-back SELECTs inside each batch (no "merge tmp rows into body" mechanism). Generated PKs enter the body because the read-back rows contain them.
4. **Separate IRs** — the planner splits into a **Select Planner** and a **Save Planner**, each with its own IR, sharing the stage/step semantic (stages sequential, steps within a stage parallel) and common leaf types.
5. Legacy mapping: **delayed KV write → next stage; parallel KV write → sibling step, same stage**. R2 writes are in scope: value comes from a runtime param holding an already-existing stream/body.
6. The include tree gates what gets written (navs AND kv/r2 fields — deliberate divergence from legacy, which wrote KV unconditionally).
7. Tests split: `query_tests.rs` → `select_tests.rs`, new `save_tests.rs`. Then retrofit GET/LIST seed INSERTs to use SAVE.

**No external consumers**: nothing outside `orm/tests` uses `query::planner::Operation` or `plan()` (verified). Everything in the query module may change freely on merit.

**Verified facts:**
- `$cloesce_tmp` is already created by the migrations crate (`migrations/src/lib.rs:95-111`) via `tests/common/setup.rs::migration_for` — every mock D1/DO pool has it.
- The existing `tests/select_tests.rs` is dead legacy code, 100% commented out — delete it before the rename. `tests/upsert_tests.rs` is also all-commented; its scenarios seed the save test matrix (delete it after porting, per the dead-tests rule).
- `QueryExecutor` holds `&'a MockStorage` (`executor.rs:27`) — save writes need `&mut`. `run_sql` panics on `Value::Null` binds — the save path must bind typed None.

**Scope**: only `src/compiler/orm/src/query/` and `src/compiler/orm/tests/`. Do not touch legacy `upsert.rs`/`select.rs`/`bin/orm.rs`, semantic, or runtime.

## 1. Module split

```
src/compiler/orm/src/query/
  mod.rs        — pub mod select; pub mod save; shared leaf types
  select/
    plan.rs     — today's plan.rs, unchanged types (SelectPlan, Stage, Step,
                  Query::{Sql, Key, Synthesize}, SqlArg, Mapping, …)
    planner.rs  — today's planner.rs; Operation loses Save (becomes {Get, List}),
                  both todo!() arms deleted
  save/
    plan.rs     — the save IR (below)
    planner.rs  — plan_save + DFS/batching/SQL generation
```

Shared leaf types move to `query/mod.rs` (or `query/common.rs`): `Database`, `DatabaseKind`, `KeySegment`, `ValueArg`, `MapCardinality`. The select IR keeps `Step::result: Vec<&'src str>` — **no PathSeg churn on the read side**; existing select tests only need the module-path rename in imports.

## 2. Save IR — `src/query/save/plan.rs`

Same stage/step shell, save-specific step types:

```rust
pub struct SavePlan<'src> { pub stages: Vec<SaveStage<'src>> }   // + stage_at like SelectPlan
pub struct SaveStage<'src> { pub steps: Vec<SaveStep<'src>> }

pub struct SaveStep<'src> {
    pub query: SaveQuery<'src>,
    /// Body path this step attaches/writes at. Empty = root. Unused ([]) for SqlBatch,
    /// whose hydration paths live on its Hydrate statements.
    pub result: Vec<PathSeg<'src>>,
}

/// One hop in a hydrated-body path. Save paths address individual payload
/// array elements, so indices are first-class.
#[derive(Serialize)] #[serde(untagged)]
pub enum PathSeg<'src> { Field(&'src str), Index(usize) }

pub enum SaveQuery<'src> {
    /// Ordered statements executed as one transaction / D1 batch against a
    /// single database (a single DO stub when sharded). Not atomic across
    /// steps or stages — atomicity is per batch.
    SqlBatch {
        database: Database<'src>,
        statements: Vec<SaveStatement<'src>>,
        /// Concrete stub routing for DurableObject; empty otherwise. The payload
        /// is known, so shard args are Param/Value/Body — never a spread.
        shard: Vec<(&'src str, SaveArg<'src>)>,
    },
    /// Write a value to KV / R2 / DO-KV at the key described by `segments`,
    /// then attach the written value at [SaveStep::result].
    KeyWrite {
        database: Database<'src>,
        segments: Vec<KeySegment<'src>>,   // KeySegment::Value(ValueArg) — see note
        value: WriteSource<'src>,
        /// Workers-KV metadata unwrapped from a `KvObject { raw, metadata }` payload.
        metadata: Option<&'src serde_json::Value>,
        shard: Vec<(&'src str, ValueArg<'src>)>,
    },
    /// Set fields on the object at [SaveStep::result] (route fields onto the
    /// body; a backingless model's whole state). Same semantics as the select
    /// IR's Synthesize.
    Synthesize {
        fields: Vec<(&'src str, ValueArg<'src>)>,
        create: bool,
    },
}

pub enum SaveStatement<'src> {
    /// INSERT / UPDATE / tmp-capture / tmp-DELETE. `?N` placeholders reference
    /// `arguments` (1-based); inline expressions (tmp subqueries,
    /// last_insert_rowid()) consume no placeholder.
    Write { sql: String, arguments: Vec<SaveArg<'src>> },
    /// A trailing read-back SELECT for ONE saved instance; its single row
    /// is attached at `result` (intermediate objects/arrays created on demand).
    Hydrate { sql: String, arguments: Vec<SaveArg<'src>>, result: Vec<PathSeg<'src>> },
}

pub enum SaveArg<'src> {
    /// A scalar runtime parameter.
    Param(&'src str),
    /// A literal value from the SAVE payload, bound positionally.
    Value(&'src serde_json::Value),
    /// A value read from the hydrated body at an exact path (a generated PK
    /// hydrated by an earlier stage's read-back).
    Body(Vec<PathSeg<'src>>),
}

pub enum WriteSource<'src> {
    /// Literal JSON value from the payload (KV / DO-KV).
    Literal(&'src serde_json::Value),
    /// Runtime param name holding an out-of-band body (R2 streams), named by
    /// the field's dotted payload path, e.g. "dogs.0.avatar".
    Param(String),
}
```

`ValueArg` gains a `Value(&'src serde_json::Value)` variant (save-only usage; harmless to the select side) so parallel key writes inline payload values directly into key segments, while delayed ones use `ParentField` resolved against the hydrated instance.

### Hydration model (replaces "merge by path")

- Every batch ends with **one `Hydrate` SELECT per saved instance**, in DFS order (parents before children), then `DELETE FROM "$cloesce_tmp"` if the batch used it.
- WHERE keys: provided PK ⇒ bound `SaveArg::Value`; generated PK ⇒ inline `(SELECT json_extract("primary_key", '$.<col>') FROM "$cloesce_tmp" WHERE "path" = '<tmp_path>')`.
- Each row attaches at its exact indexed path (`["dogs", 1]`), so **the body is payload-ordered** — array indices are stable. This makes `SaveArg::Body(indexed path)` reliable for cross-batch FKs and `ValueArg::ParentField` reliable for delayed key templates, and it means partial UPDATEs return full DB truth (not a payload echo).
- `$cloesce_tmp` path encoding (planner-internal contract): dotted body path, array elements by index, root = `""` — `"dog"`, `"dogs.1"`, `"dogs.1.toys.0"`. Inlined as SQL literals — safe: identifiers and indices only, never user data.

## 3. Save planner — `src/query/save/planner.rs`

```rust
pub fn plan_save<'src>(
    model: &str,
    idl: &'src CloesceIdl<'src>,
    tree: &IncludeTree<'src>,
    payload: &'src serde_json::Value,
) -> crate::Result<SavePlan<'src>>
```

Fallible (reuses `OrmErrorKind`); payload values validated via `crate::validate::validate_cidl_type`, like legacy. Reuses `key_segments`, `database`, `Params` from the select planner (made `pub(super)` / moved to `query/mod.rs`).

State: `batches: Vec<Batch>` keyed by `(stage, binding, shard args)` — linear scan (`SaveArg` isn't `Hash`), first-visit order = deterministic emission — plus `steps: Vec<(usize, SaveStep)>` for KeyWrite/Synthesize. `Batch { stage, database, shard, writes, hydrates, uses_tmp }` (hydrates kept separate, appended after all writes at emission).

PK provenance flowing through the DFS:

```rust
enum PkSource<'src> {
    Provided(&'src Value),                                       // concrete payload value
    Param(&'src str),                                            // route-field runtime param
    Generated { batch: usize, stage: usize, tmp_path: String },  // auto-increment
}
```

DFS `visit(model, payload_obj, tree, path: Vec<PathSeg>, parent_ctx)` mirroring legacy `UpsertModel::dfs`, payload- and index-aware:

1. **Backingless / non-sqlite model**: no SQL. Emit `Synthesize { create: true }` (root: from route params) so the instance object exists in the body; recurse included navs (children source nav-key values from route params, like the read planner); emit KV/R2 writes (step 6). SQL-backed root gets a `Synthesize { create: false }` merge of non-shard route fields (matches read shape).
2. **1:1 (`One`) navs first** (post-order — this row holds the FK): recurse each nav present in tree ∩ payload; record `local_key -> PkSource` from the child ctx.
3. **Pick the batch**: compute `(binding, shard)` — root shard from route params (`SaveArg::Param`); nested DO target shard from payload (`SaveArg::Value`) or a generated parent PK (`SaveArg::Body`). If a dep's batch has the same binding + identical shard args ⇒ join it (same transaction, in-batch tmp subqueries). Else `stage = max over deps(Provided/Param ? dep.stage : dep.stage + 1)` (root: 0); find-or-create. Cross-batch generated FKs become `SaveArg::Body(dep_path + pk_field)` — resolvable because the dep's batch hydrated that exact path.
4. **Resolve columns** (legacy decision table):

   | condition | result |
   |---|---|
   | value in payload | validate, bind `SaveArg::Value` |
   | FK from ctx, source `Provided`/`Param` | bind `SaveArg::Value` / `SaveArg::Param` |
   | FK `Generated`, same batch | inline `(SELECT json_extract("primary_key", '$.<col>') FROM "$cloesce_tmp" WHERE "path" = '<tmp_path>')` |
   | FK `Generated`, other batch | bind `SaveArg::Body(path)` (lifts stage per rule 3) |
   | missing single `Int` PK | auto-increment (composite ⇒ `Err(ModelKeyCannotAutoIncrement)`) |
   | PK missing entirely + column nullable | bind `Value::Null` |
   | PK present, non-nullable non-PK missing | flag partial ⇒ plain UPDATE |
   | otherwise | `Err(MissingField)` |

5. **Emit statements**: the row `Write` (shapes below); if auto-increment, the tmp capture `Write` right after (`uses_tmp = true`), returning `PkSource::Generated { tmp_path: dotted(path) }`. Always queue the instance's `Hydrate` SELECT (WHERE per PK: bound value, or tmp subquery when generated) with `result = path`.
6. **KV/R2 writes** (tree-gated): key segments — placeholder route-covered ⇒ `ValueArg::Param`; present in this payload instance ⇒ `ValueArg::Value` (**parallel**: step at `instance_stage`); otherwise (references a generated PK) ⇒ `ValueArg::ParentField` (**delayed**: step at `instance_stage + 1`, resolved against the hydrated instance). KV value = payload field, `KvObject` unwrapped to value/metadata (missing ⇒ `Err`); R2 value = `WriteSource::Param(dotted_field_path)`. `result = path + [Field(name)]`; the executor attaches the written value there so the response includes it.
7. **Many navs after the row**: recurse per array element with `path + [Field(nav), Index(i)]`, passing this ctx as parent. Non-array payload for a many nav: silently ignored (legacy).

**Emission**: per stage — batch steps (writes, then hydrates, then tmp DELETE) in first-visit order, then Synthesize/KeyWrite steps.

## 4. SQL shapes (in-house `format!`; user data always bound, never inlined)

```sql
INSERT INTO "Person" ("name", "dogId") VALUES (?1, (SELECT json_extract("primary_key", '$.id') FROM "$cloesce_tmp" WHERE "path" = 'dog'))
INSERT OR REPLACE INTO "$cloesce_tmp" ("path", "primary_key") VALUES ('dogs.1', json_object('id', last_insert_rowid()))
INSERT INTO "Horse" ("id", "name", "age") VALUES (?1, ?2, ?3) ON CONFLICT ("id") DO UPDATE SET "name" = "excluded"."name", "age" = "excluded"."age"
UPDATE "Horse" SET "name" = ?1 WHERE "id" = ?2
INSERT INTO "Horse" DEFAULT VALUES
-- hydrate (per instance; generated-PK variant shown)
SELECT "id", "name" FROM "Dog" WHERE "id" = (SELECT json_extract("primary_key", '$.id') FROM "$cloesce_tmp" WHERE "path" = 'dogs.1')
DELETE FROM "$cloesce_tmp"
```

ON CONFLICT only when all PKs resolved ∧ PKs exist ∧ non-PK columns exist (legacy rule). Inlined text is limited to identifiers, tmp paths, and `last_insert_rowid()`/`json_extract` expressions.

## 5. Mock executor — `tests/common/`

Add `execute_save(plan: &SavePlan, params, storage: &mut MockStorage) -> Value` (new fn or new `save_executor.rs`; factor shared helpers — bind, `row_to_json`, key resolution — out of the existing read executor rather than duplicating). The read executor and its `&MockStorage` stay as they are.

- **SqlBatch**: resolve pool (D1 by name; DO by resolving shard args to a concrete tuple — shards pre-declared via `shard_inits`; extend `MockStorage` to retain per-binding migrations and lazily create new shard pools if a test saves to a brand-new shard). `pool.begin()`; `Write` ⇒ bind + execute (`Value::Null` ⇒ `Option::<String>::None`, bools ⇒ i64); `Hydrate` ⇒ `fetch_one`, attach the row at its `result` path, creating intermediate objects/arrays on demand (parents hydrate first by construction); `commit`.
- **SaveArg::Value(v)** ⇒ bind `v`; **SaveArg::Body(path)** ⇒ exact body-path lookup (panics if absent, consistent with the mock's hard-failure style).
- **KeyWrite**: resolve key segments against the parent object at `result[..len-1]` + params; resolve value (`Literal` clone / `Param` lookup); write into `storage.kv` / `storage.r2` / `storage.durable_kv[binding][shard]` via `entry().or_default()`; attach the written value at `result`. Mock ignores metadata for storage (already-documented simplification) but may record it for assertions.
- **Synthesize**: same semantics as the read executor, over `PathSeg` paths.

## 6. Tests

1. **Delete** dead all-commented `tests/select_tests.rs`; **rename** `tests/query_tests.rs` → `tests/select_tests.rs` (plain `mv`, no git commands); update imports for the module split. Delete `tests/upsert_tests.rs` once its scenarios are ported.
2. **New `tests/save_tests.rs`** with helper `save_ok(idl, model, include, payload, params, &mut storage) -> (SavePlan, Value)` (leak payload for `'static`, same trick as `tree()`). Each test asserts plan shape (stage/step/statement counts; exact SQL strings for a few), storage state (SQL via pool / map lookups), and the response body (read-back truth incl. generated PKs).

Matrix:
1. `save_scalar_with_pk_upserts` — ON CONFLICT; re-save updates.
2. `save_auto_increment_pk` — body returns `id: 1`; batch = insert + tmp capture + hydrate + tmp delete.
3. `save_one_to_one_same_db` — dog before person, FK via in-batch tmp subquery, 1 stage, 1 batch.
4. `save_one_to_many_same_db` — indexed tmp paths `dogs.0`/`dogs.1`; per-instance hydrates keep payload order (fixes legacy tmp-path clobber).
5. `save_composite_pk` / 6. `save_composite_pk_missing_errors` (`ModelKeyCannotAutoIncrement`).
7. `save_junction_table_composite_fk_pk` — junction keyed by generated parent PK + provided FK.
8. `save_partial_update` — UPDATE path; response is full DB truth from read-back.
9. `save_missing_required_field_errors`.
10. `save_include_tree_gates_navs` — payload nav not in tree ⇒ not written, absent from body.
11. `save_cross_db_child_concrete_fk` — both batches stage 0.
12. `save_cross_db_child_generated_fk` — child batch stage 1 with `SaveArg::Body`.
13. `save_do_root` — shard from route params; route field synthesized onto body.
14. `save_do_child_fanout` — two distinct shard values ⇒ one SqlBatch per stub.
15. `save_kv_parallel` / 16. `save_kv_delayed` — same stage vs next stage; assert `storage.kv`.
17. `save_kv_object_unwrap` — `{raw, metadata}`; missing `raw` errors.
18. `save_do_kv_field` — sharded DO storage write.
19. `save_r2_parallel` / 20. `save_r2_delayed` — value from `params["avatar"]` / `params["dogs.0.avatar"]`; assert `storage.r2`.
21. `save_backingless_root` — worker model: KV/R2 + sqlite child via route params.
22. `save_response_shape` — nested graph, full body assertion.

3. **Follow-up (same task)**: swap seed INSERTs in `select_tests.rs` for `plan_save` executions via a `seed_via_save` helper wherever the payload can express the seed; keep raw SQL only where save can't express the shape.

## 7. Implementation order & verification

1. Module split (`select/`, shared types in `query/mod.rs`), drop `Operation::Save` → `cargo check -p orm` clean, `cargo test -p orm --test select_tests` green (after the rename) **before any save code** (phase gate).
2. `save/plan.rs` IR → check clean.
3. `save/planner.rs` + `execute_save` + `save_tests.rs`, built incrementally per area (SQL-only scalar → navs/tmp → cross-DB/DO → KV/R2).
4. Seed-via-save retrofit → full `cargo test -p orm`.

Full builds via `make build-src` if needed; scoped `cargo test -p orm` for iteration. No git commands at any point.

## 8. Known consequences / flagged items

- Atomicity is per `SqlBatch` only; cross-stage/cross-storage saves are not one transaction (inherent). Doc-comment on `SqlBatch`.
- Read-back means the response is DB truth: partial updates return the full row; a many-nav's response contains only the saved instances (per-instance hydrates), not all DB children — matches "what you saved, hydrated".
- KV fields in the save response are the raw written values, not the read-side `{value}` wrapper.
- KV/R2 writes are include-tree-gated, diverging from legacy's unconditional KV writes (per decision).
- Blob columns: mock binder has no blob path; out of the matrix, `TODO` if hit.
