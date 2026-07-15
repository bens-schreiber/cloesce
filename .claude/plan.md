# Integrate the new query planner into Cloesce

## Context

The new query planner (`src/compiler/orm/src/query/`: select + save planners, explain renderer) is complete and tested in Rust (`orm/tests/{select,save,validate,explain}_tests.rs`, with mock executors in `tests/common/`). The old ORM (WASM-emitted SQL strings, compiler-baked `get_query`/`list_query`) was deleted, leaving the repo mid-migration:

- **Workspace is broken**: `codegen/templates/backend.ts.jinja` still interpolates deleted `idl::DataSource` fields (`ds.include_query` :540/:602, `ds.get_query`, `ds.list_query`) → `cargo check --workspace` fails in codegen.
- **TS runtime is dangling**: `src/runtime/ts/src/router/orm.ts` calls old WASM exports (`map` :109, `select_model` :144, `upsert_model` :219) that no longer exist. `wasm.ts` already declares the new ABI (`plan_select`/`plan_save`/`validate_type`).
- **A complete reference implementation exists untracked** in `src/runtime/ts/dist/router/` (`plan.js/.d.ts`, `executor.js/.d.ts`, rewritten `orm.js`) — the source `.ts` files were never landed. Port from these; do not redesign.

Goal: wire the planner through compiler → TS runtime → e2e → examples, **phase-gated** (all tests in a phase pass before the next phase starts). Per user decisions:
- **Remove** the raw-SQL escape hatch (generated `Orm.get/list` taking a custom `D1PreparedStatement`, jinja ~:885-920).
- **Add a CLI `cloesce explain` command**, and embed the explain output as ` ``` `-fenced doc comments on each generated CRUD method.
- **Precompile select plans into the IDL**: get/list plans for every data source are compile-time-known; store the serialized `SelectPlan` on `idl::DataSource` so the runtime skips the WASM `plan_select` call for known plans. Save plans are payload-dependent and always go through WASM `plan_save`.

## The contract (fixes the API shape everything meets at)

Locked by `dist/router/orm.d.ts`/`orm.js`:
- `Orm.fromEnv(env)`; DO context detected via `env[ENV_DURABLE_TARGET_KEY]`.
- `get(meta, params, tree)` / `list(meta, params, tree)` / `save(meta, model, tree)` → `Promise<CloesceResult<...>>` (via `CloesceError.catchGeneric`), so generated `res.errors`/`res.value` handling is unchanged.
- Internally: plan (precompiled or WASM) → `storageResolver()` wiring `D1SqlStore`, `Local/RemoteDurableSqlStore` (via `__cloesceSqlBatch` RPC), `WorkerKvStore`, `R2KeyStore`, DO-KV (via `__cloesceKvGet`/`__cloesceKvPut` RPC) → `executeSelect`/`executeSave` → light `hydrateType` scalar coerce pass (no KV/R2 refetch).
- Generated CRUD: `Orm.fromEnv(env).get(Meta, { id }, this.tree)` etc. — no SQL, no `D1PreparedStatement`, D1 and DO-SQLite template branches collapse into one. Extension: pass the precompiled plan too, e.g. `get(Meta, params, this.tree, this.getPlan)`.
- Generated abstract DO class (jinja :140 `export abstract class {{ binding.name }} extends DurableObject`) gains `__cloesceSqlBatch`/`__cloesceKvGet`/`__cloesceKvPut` methods delegating to runtime `durableSqlBatch(storage, …)` / storage KV (bodies visible in `dist/router/orm.js` :192-218, :281).

---

## Phase 1 — Compiler (idl, semantic, codegen, CLI)

### 1a. Precompiled plans + explain text on the IDL
- `idl/src/lib.rs` — add to `DataSource`: `get_plan: Option<serde_json::Value>`, `list_plan: Option<serde_json::Value>` (serialized into `cidl.json`), and `get_explain: String` / `list_explain: String` (`#[serde(skip)]`, codegen-only). `SelectPlan<'src>` borrows from the IDL, so store serialized JSON, not the struct (same reason legacy stored `String` queries).
- `semantic` — post-pass after the full `CloesceIdl` is constructed (planning needs cross-model nav resolution): for each model × data source, call `orm::query::select::planner::plan(Get|List, …)`, serialize with `serde_json::to_value`, render `explain_select`, store on the `DataSource`. `semantic` depending on `orm` has legacy precedent (`use orm::select::SelectModel`).
- Note the planner runs at compile time here AND stays exported from WASM (`bin/orm.rs` unchanged) — save always plans at runtime; select keeps a WASM fallback.

### 1b. Codegen template rewrite — `codegen/templates/backend.ts.jinja`
- Delete `selectQuery`/`getQuery`/`listQuery` in all branches (:540-548, :601-611 etc.); collapse D1 + DO-SQLite branches (:537-598 vs :600-658) into one plan-based branch.
- Emit per-data-source: `tree` (unchanged, via `mapper.include_tree`), `getPlan`/`listPlan` JSON literals (new mapper method beside `include_tree` in `codegen/src/mappers.rs` — TS strings go in the mapper, never the template), and `get/list/save` bodies calling the contract API. Drop `seed` and the `SqlStatement` import if now unused.
- Doc comments: above each generated `get`/`list`, emit `/** ... */` containing the `*_explain` text inside a ` ``` ` fence (escape any `*/`). `save` gets no explain (payload-dependent); a one-line generic comment is fine.
- Add the three `__cloesce*` RPC methods to the abstract DO class block (:140).
- Remove the raw-SQL `Orm` escape-hatch surface (~:885-920); keep the plan-based `Orm` namespace (save/get/hydrate wrappers).
- Check `codegen/src/backend/mod.rs` `BackendTemplate` for any helper referencing removed fields.

### 1c. CLI explain command — `src/compiler/cli/`
- `cloesce explain <model> <data-source> <get|list|save>`: compile through semantic, plan, print `explain_select`/`explain_save`. `save` requires `--payload <file.json>`.

### 1d. Tests (gate)
- Regenerate `codegen/tests/snapshots/snapshot_tests__backend_code_generation_snapshot.snap` (insta); hand-review: no baked SQL, plan JSON + doc-comment explains + DO RPCs present. `client`/`durable_migration` snapshots must be unchanged.
- Semantic: add tests asserting `get_plan`/`list_plan` are populated and deserialize as valid plan JSON (existing `data_source_tests.rs` only asserts on `tree` — extend it).
- CLI: a smoke test for `explain` if the crate has test scaffolding; otherwise manual verification.
- **Verify**: `cargo test --workspace` green, then `make build-src` (never raw `cargo build`).

## Phase 2 — TS runtime (`src/runtime/ts/`)

### 2a. New source files (port from untracked dist, adding types)
- `src/router/plan.ts` ← `dist/router/plan.d.ts`: TS mirror of the plan IR (externally-tagged serde JSON: `SelectPlan`, `Select::Sql|Key|Synthesize`, `SqlArg`, `Mapping`, `SavePlan`, `SaveQuery`, `SqlStatement::Write|Hydrate`, `PathSegment`, `Database`).
- `src/router/executor.ts` ← `dist/router/executor.js`: `executeSelect(plan, params, storage, wrapKey)`, `executeSave(plan, storage)`, `StorageResolver`/`SqlStore`/`KeyStore`. Preserve: per-stage step parallelism, `?N` spread expansion (highest index first) + empty-`IN` short-circuit, DO shard fan-out, cardinality-aware join/attach via `PathSegment`, `$cloesce_tmp` hydrate read-back.

### 2b. Rewrite `src/router/orm.ts` ← `dist/router/orm.js`
- Delete all dead old-ABI code (`wasm.map`/`select_model`/`upsert_model` paths, old repositories the executor's stores replace).
- `get`/`list` accept an optional precompiled plan (from generated code / `cidl.json`); fall back to WASM `plan_select`. `save` always calls `plan_save`.
- Keep exporting `hydrateType` (used by `router.ts` for param validation and as the post-executor scalar coerce). Export `durableSqlBatch` for the generated DO RPCs.
- `router.ts`/`crud.ts` should need no changes.

### 2c. Tests (gate) — this is where legacy coverage is mirrored
- **Create `tests/executor.test.ts`** (vitest), mirroring the Rust mock executors (`orm/tests/common/{select,save}_executor.rs`) and the deleted legacy ORM coverage:
  - select: one/many cardinality, nested include joins, spread `IN` expansion + zero-row short-circuit, `?1` vs `?10` placeholder ordering, DO shard fan-out + dedup, Key steps (KV/R2, `wrapKey` KValue vs passthrough), Synthesize create/merge, seek pagination (`lastSeen_*`/`limit` params).
  - save: `SqlBatch` Write + Hydrate read-back, `$cloesce_tmp` autoincrement capture, KeyWrite (KV metadata, R2, DO-KV), nested `PathSegment` attach, `SaveArg::Payload` vs `Result` resolution, error semantics (missing param, batch failure).
  - TS-only: D1 vs local/remote durable store routing via `ENV_DURABLE_TARGET_KEY`, bind-value coercion.
  - Use in-memory/Miniflare-style fakes consistent with existing `orm.test.ts` patterns and `tests/builder.ts` helpers.
- **Update `tests/orm.test.ts`**: keep `hydrateType` scalar-coercion cases; delete cases asserting the old fetch-inside-hydrate KV/R2 behavior now owned by the executor (delete, don't repurpose — dead tests get deleted).
- **Verify**: `pnpm --filter cloesce run build` typechecks (or scoped `tsc -p src/runtime/ts`); `pnpm --filter cloesce run test` fully green.

## Phase 3 — e2e (`tests/e2e/`)

- Prereq: `make build-src` (fresh CLI + WASM + runtime dist).
- No harness changes expected; `globalSetup` regenerates fixtures with `target/release/cloesce`, so committed `backend.ts`/`client.ts`/`cidl.json` in every fixture change wholesale (expected — they now contain plan JSON + explain doc comments).
- Run **one suite at a time**, killing stale wranglers between (`pkill -f wrangler`), with `tsc` scoped to the fixture first — never full-package tsc. Order lowest→highest risk: d1_crud, foreign_keys, composite_keys, partials, multiple_db, bools_dates, poos, validators, advanced_data_sources, services, fail, blobs, kv, r2, durable_objects (DO last: exercises the new RPC surface).
- Failures here are fixed in Phase 1/2 code; re-run that phase's gate before resuming.

## Phase 4 — Examples (`examples/`)

- `examples/weather` (D1) and `examples/reddit` (DO — `UserDo`/`SubRedditDo` inherit the new `__cloesce*` RPCs from the generated abstract class; highest-risk surface). Regenerate `.cloesce/backend.ts` via the compiled CLI; run each example's vitest-pool-workers suite.
- Final gate: full `make test` (cargo → runtime tests → examples → e2e).

## Verification summary
1. Phase 1: `cargo test --workspace` → `make build-src`.
2. Phase 2: scoped tsc + `pnpm --filter cloesce run test`.
3. Phase 3: per-fixture tsc + one vitest suite at a time, wranglers killed between.
4. Phase 4: example suites, then `make test`.
5. Spot-check: `cloesce explain` on an e2e fixture schema; eyeball a regenerated `backend.ts` for explain doc comments and absence of SQL strings.

## Key files
- `src/compiler/idl/src/lib.rs` (DataSource fields)
- `src/compiler/semantic/` (plan/explain post-pass; extend `tests/data_source_tests.rs`)
- `src/compiler/codegen/templates/backend.ts.jinja`, `src/compiler/codegen/src/mappers.rs`, `backend/mod.rs`, snapshot regen
- `src/compiler/cli/` (explain command)
- `src/runtime/ts/src/router/{plan.ts,executor.ts}` (create; port from `src/runtime/ts/dist/router/`)
- `src/runtime/ts/src/router/orm.ts` (rewrite), `tests/executor.test.ts` (create), `tests/orm.test.ts` (prune)
