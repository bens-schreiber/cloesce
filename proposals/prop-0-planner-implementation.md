# Handoff: Query Planner — GET/LIST for SQLite relationships

Implement `Operation::Get` and `Operation::List` in the Cloesce query planner for
SQLite-backed Models (D1 and Durable Objects), plus a test harness with a mock
runtime executor that actually runs generated plans against sqlx SQLite pools.

**Out of scope:** KV/R2 steps, `Operation::Save`, cursor pagination, wiring the
planner into `bin/orm.rs` / codegen / semantic. The planner stays a pure,
standalone function. Do not modify anything outside the `orm` crate, and inside
it only touch `src/query/` and `tests/`.

## Context

- Proposal: `proposals/proposal-0-clorm.md` (read the Query Planner + GET/LIST sections).
- IR: `src/compiler/orm/src/query/plan.rs` — types are final for this phase.
- Skeleton: `src/compiler/orm/src/query/planner.rs` — `plan()` signature is final;
  `select_model` is a stub ending in `todo!`.
- IDL types: `src/compiler/idl/src/lib.rs` (`Model`, `NavigationField`,
  `NavigationKeyMapping`, `ModelBacking`, `IncludeTree`).
- Reference for column selection style: `src/compiler/orm/src/select.rs`
  (explicit column lists via sea_query — do the same, never `SELECT *`).
- Test utilities: `compiler_test::src_to_idl` (real compiler syntax → IDL) and
  the migration approach in `src/compiler/orm/tests/common/mod.rs`.

## Decisions already made (do not relitigate)

1. **No SQL JOINs, ever.** Every Model is resolved by its own query, even when
   parent and child share a database. The scratch comment block at the bottom of
   `planner.rs` describes an older JOIN design — it is superseded; replace it with
   the real implementation.
2. **Stage index == include-tree depth.** Root step is stage 0; a nav field at
   depth `d` in the include tree becomes a step in stage `d`. Carry a
   `depth: usize` through the DFS and push with `plan.stage_at(depth)`. Children
   always go one stage after their parent, even same-DB (parent rows must exist
   to supply bindings).
3. **Error model: always partial.** The executor collects step failures into a
   sink, skips steps whose inputs depend on a failed step, and returns the
   partial hydrated body plus the errors. No error is fatal to the whole plan.
4. **Every SELECT is ordered by all primary key columns ASC** (deterministic
   `Cardinality::One` coercion). `ORDER BY` comes before `LIMIT`.
5. **Related steps always use `Binding::Spread` + `IN`**, including under GET
   (a single parent row is just a 1-element set). Uniform runtime path; the
   executor dedups, drops nulls, and chunks by a configurable bind limit.

## Planner implementation

### Root step (stage 0)

- Resolve the model
  Models with no backing or `!uses_sqlite()` are skipped silently (matches
  `select.rs` behavior).
- `Database`: `backing.binding`; `DatabaseKind::D1` or
  `DatabaseKind::DurableObject { shard }` where `shard` is
  `ModelBacking::fields` mapped to `Binding::Param` (route fields supply the DO
  id at the root — they are *not* SQL columns and never appear in WHERE).
- GET: `WHERE pk = ?N` for each primary column, bindings `Binding::Param(pk name)`
  appended after any shard params; `Mapping { cardinality: One, stitch: vec![] }`.
- LIST: no WHERE; `LIMIT ?N` bound to `Binding::Param("limit")`;
  `Mapping { cardinality: Many, stitch: vec![] }`.
- `result: ObjectPath::Root`. Explicit column list (primary + scalar columns).

### Nav steps (stage = depth)

For each `NavigationField` whose name appears in the include tree (recurse into
subtrees; skip targets that don't use sqlite):

- `result`: `ObjectPath::Field(path)` where `path` is the chain of nav field
  names from the root (e.g. `["dog", "toy"]`).
- **Partition `nav.keys`** using `nav.target_backing`:
  - Keys whose `target` is in `target_backing.fields` (DO shard/route fields,
    not columns) become the stub's shard bindings:
    `DatabaseKind::DurableObject { shard: vec![Binding::Spread(parent_path + local)] }`.
    Spread shard values fan the step out: one stub per distinct value. The
    executor tags each returned row with the shard value under the shard field
    name so stitching stays uniform; add a stitch pair for it too.
  - Remaining keys become SQL predicates + stitch pairs:
    `WHERE target IN (?N)` with `Binding::Spread(ObjectPath::Field(parent_path + [local]))`,
    and `StitchKeys { parent_key: local, child_key: target }`.
- Multi-key (spider) navs: emit one `IN` predicate per key (`a IN (…) AND b IN (…)`)
  plus all stitch pairs. The SQL is a superset prefilter; stitch pairs (ALL must
  match) guarantee exact pairing. Do not bother with row-value `(a,b) IN (…)`.
- Empty-form navs (`nav.keys` empty): no WHERE, `stitch: vec![]` — every parent
  receives the first row (`One`) or all rows (`Many`).
- `Mapping.cardinality` = `nav.cardinality.into()`.

MAke sure to always ORDER BY for any SELECT statement, even if there is a unique index.

## Mock runtime executor (test-only)

Put it in `tests/common/` (e.g. `tests/common/executor.rs`). It executes a
`&QueryPlan` exactly as written — no IDL access — against:

```rust
struct Backends {
    /// D1 binding name -> pool
    d1: HashMap<String, SqlitePool>,
    /// DO binding name -> (shard value tuple -> pool); each DO instance is its own db
    durable: HashMap<String, HashMap<Vec<Value>, SqlitePool>>,
}
```

- Signature roughly:
  `async fn execute(plan, params: Map<String, Value>, backends, chunk_size) -> (Value, Vec<ExecError>)`.
- Stages sequential; steps within a stage concurrent (`futures::future::join_all`
  or sequential-with-shuffled-order is acceptable if adding a futures dep is
  annoying — but preserve the semantics that steps can't see siblings' output).
- Binding resolution: `Param` from `params`; `Scalar`/`Spread` via `ObjectPath`
  traversal of the hydrated result, flattening across arrays; `Spread` dedups
  and drops nulls, then chunks by `chunk_size`, replacing the `IN (?N)`
  placeholder with the expanded placeholder list per chunk and unioning rows.
- Missing param / missing result value / SQL error → push to the error sink,
  skip the step (and any later step whose bindings resolve through its result
  path), keep going.
- Mapping: group child rows by stitch pairs (all pairs must match), attach as
  object (`One`, first row wins) or array (`Many`) at `result` path on each
  parent; empty stitch = same value(s) to every parent; a `One` nav with no
  matching row = `null`.
- DO fan-out: group Spread shard values, look up (or error) the pool per shard
  tuple, run the query per pool, tag rows with shard values before stitching.

## Test harness

- Build IDLs with `compiler_test::src_to_idl` using the **new** `one`/`many`
  syntax (check `frontend` parser tests or the proposal for exact grammar).
- Schema setup: adapt the migration pattern from `tests/common/mod.rs`, but
  **per database**: group `uses_sqlite()` models by `backing.binding`, generate
  a migration per group, and apply it to the matching pool (and to every shard
  pool of a DO binding).
- Plan-shape unit tests: build expected `QueryPlan` values (everything is
  `PartialEq`) or compare `serde_json::to_value(plan)` against expected JSON.
- End-to-end tests: seed rows with plain INSERTs, run `plan()`, execute via the
  mock executor, assert the hydrated JSON equals the expected value exactly.

### Scenarios to cover

1. Scalar model on D1: GET by pk, LIST with limit (limit chunk order asserted).
2. `one` nav, same D1 db (Person→Dog): GET + LIST; LIST asserts each person gets
   *their* dog (stitch, not first-row).
3. `many` nav, same db (User→Posts): GET + LIST; LIST with interleaved ownership
   to prove stitch distribution.
4. Cross-database navs: two `d1` bindings, parent in one, child in the other.
5. Nested include tree depth 2 (User→dog→toy): three stages; toy stitched onto
   the correct dog under the correct user.
6. DO-backed root: GET with shard param from route fields (single-shard pool).
7. D1 root → DO-backed child: LIST fanning out over multiple shard pools.
8. DO root → D1 child (Reddit-clone shape from the proposal).
9. Empty-form nav: every parent receives the same value(s).
10. Composite pk root + spider nav with two keys.
11. `Cardinality::One` coercion: multiple matching rows → first by pk order.
12. Spread mechanics: chunk_size 2 with 5 parents (batching), duplicate FK values
    (dedup + shared child cloned onto both parents), null FK (dropped from IN,
    nav is `null`, **no** error).
13. Errors: missing runtime param → step error + dependents skipped + partial
    body; root step SQL failure → empty body + error; unknown model →
    `Err(UnknownModel)` from `plan()` itself.

## Conventions and constraints

- Rust style: functional > declarative > imperative; no comments that narrate
  code. Never delete Ben's existing comments except the superseded JOIN scratch
  block noted above.
- **Pre-existing failures:** `map_tests` / `select_tests` (and possibly others)
  currently fail because they use the old `nav` syntax. That is expected and out
  of scope — do not fix, delete, or port them. Gate on: `cargo check -p orm`
  clean and the new `query` tests passing (`cargo test -p orm --test <new test file>`).
- Full builds go through `make build-src`, not raw `cargo build`.
