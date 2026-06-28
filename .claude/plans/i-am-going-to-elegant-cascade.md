# Proposal 0 (ORM v2) — Syntax & Semantic Analysis

## Context

[proposal-0-clorm.md](../../proposals/proposal-0-clorm.md) redesigns Cloesce relationships so **any Model can relate to any other Model**, regardless of backing (D1, Durable Object, Worker), with explicit cardinality. Today relationships use a single `nav` block whose cardinality is inferred from whether a local key is present, and the semantic layer *forbids* cross-backing navigations (a nav target must share the current model's D1 binding, or be a worker-backed 1:1 — see [model.rs:716](../../src/compiler/semantic/src/model.rs#L716)).

This is the **first step**: the syntax + semantic-analysis (frontend + semantic → CIDL) changes only. No codegen, runtime, or query-planner work. The semantic layer should accept cross-backing relationships and emit a richer IR; downstream codegen will be stubbed/deferred to a later phase.

### Scope constraints (hard rules)

- **Only the `frontend`, `semantic`, and `idl` crates are in scope for edits.** The rest of the compiler (codegen, migrations, runtime, etc.) must remain **untouched**.
- **If a downstream crate stops compiling** because of the AST/IR changes, **temporarily comment out that module/crate** (e.g. in the workspace/`mod` declarations) — do **not** migrate or fix it. A later phase handles it.
- **CloesceIDL changes live only in [idl/src/lib.rs](../../src/compiler/idl/src/lib.rs).** Do not propagate IR changes to any consumer.
- **Only migrate tests in `semantic` (analysis_tests) and any `frontend` parser tests.** Do **not** touch tests or fixtures belonging to codegen/migrations/runtime — if they break to compile, comment the module out.

Per decisions:
1. **Replace `nav` entirely** with `one` / `many` keywords (no back-compat).
2. **Lift the same-backing restriction** — validate cross-backing relationships, producing richer IR (no codegen).
3. **Include the spider initializer** `Model::{f(local), g(local2)}` for `one`/`many` and for `kv`/`r2` (enabling DO-KV with separate shard vs. template args).

## Current state (key files)

- Grammar: [frontend/src/parser/model.rs](../../src/compiler/frontend/src/parser/model.rs) — `nav_block` at [L150-192](../../src/compiler/frontend/src/parser/model.rs#L150-L192); `binding_ref` (kv/r2) at [L45-99](../../src/compiler/frontend/src/parser/model.rs#L45-L99).
- AST: [frontend/src/lib.rs](../../src/compiler/frontend/src/lib.rs) — `NavAdj`/`NavigationBlock` [L310-337](../../src/compiler/frontend/src/lib.rs#L310-L337), `ModelBlockKind` [L382-391](../../src/compiler/frontend/src/lib.rs#L382-L391), `contextual_keywords!` macro [L18-113](../../src/compiler/frontend/src/lib.rs#L18-L113).
- Semantic: [semantic/src/model.rs](../../src/compiler/semantic/src/model.rs) — `nav()` [L570-816](../../src/compiler/semantic/src/model.rs#L570-L816), `kv_field()`/`r2_field()` [L855-971](../../src/compiler/semantic/src/model.rs#L855-L971), `resolve_binding_ref()` [L982+](../../src/compiler/semantic/src/model.rs#L982).
- IR: [idl/src/lib.rs](../../src/compiler/idl/src/lib.rs) — `NavigationFieldKind`/`NavigationField` [L182-217](../../src/compiler/idl/src/lib.rs#L182-L217), `ModelBacking`/`BackingKind` [L424-442](../../src/compiler/idl/src/lib.rs#L424-L442), `KvField`/`R2Field` [L342-368](../../src/compiler/idl/src/lib.rs#L342-L368).
- Errors: [semantic/src/err.rs](../../src/compiler/semantic/src/err.rs) — nav variants at L79-107, ariadne rendering at L472-575.
- Tests: [semantic/tests/analysis_tests.rs](../../src/compiler/semantic/tests/analysis_tests.rs).

## Syntax keyword note

`durable`, `shard`, `kv`, `r2`, `for`, `route`, `primary`, `column` already exist as contextual keywords (`one`/`many` do **not**). Keep keywords contextual (not lexer-hard-reserved) per the existing pattern — only `self`/`ctx`/`env` are hard-reserved in the lexer.

---

## Plan

### Phase A — Grammar (frontend)

**Keywords** ([lib.rs:18-113](../../src/compiler/frontend/src/lib.rs#L18-L113)): add `One => "one"`, `Many => "many"`; remove `Nav => "nav"`.

**AST** ([lib.rs:310-413](../../src/compiler/frontend/src/lib.rs#L310-L413)):
- Replace `NavigationBlock`'s implicit cardinality with an explicit `kind: Cardinality { One, Many }` field; drop the `is_one_to_one()` heuristic.
- Generalize `NavAdj` so an adj entry's *target side* can be a single field (`Model::field(local)`) OR the spider form (`Model::{f1(l1), f2(l2)}`). Concretely: keep `NavAdj { model, field, local_key }` as the per-pair shape and introduce the spider initializer as **a list of `(field, local_key)` pairs grouped under one model**. Recommended representation:
  ```rust
  pub struct RelationBlock<'src> {        // replaces NavigationBlock
      pub cardinality: Cardinality,       // One | Many
      pub model: Symbol<'src>,            // target model
      pub keys: Vec<RelationKey<'src>>,   // empty => discriminator-less
      pub field: Spd<Symbol<'src>>,       // result field name in `{ ... }`
  }
  pub struct RelationKey<'src> {          // `targetField(localField)` pair
      pub target: Symbol<'src>,
      pub local: Option<Symbol<'src>>,    // None for `many Model::tenantId(...)`? see note
  }
  ```
  Rename `ModelBlockKind::Navigation(NavigationBlock)` → `Relation(RelationBlock)`. Update `ModelBlockKind::symbols()`, `navigation_blocks()` (rename → `relation_blocks()`), and any other matches on these variants.

**Parser** ([model.rs:150-204](../../src/compiler/frontend/src/parser/model.rs#L150-L204)): replace `nav_block` with `relation_block` accepting:
- `one|many Model { field }` — no discriminator.
- `one|many Model::target(local) { field }` — single direct form.
- `one|many Model::{ target(local), target2(local2) } { field }` — spider form (brace-delimited list of `target(local)` pairs).
- Allow `target` with no `(local)` for the shard-only `many Post::tenantId(tenantId)` example (decide whether shorthand `tenantId` desugars to `tenantId(tenantId)` — match proposal §"Only Durable Shard Discriminators"). Surface this representation choice in the AST `RelationKey.local`.

**Spider init for kv/r2** ([model.rs:45-99](../../src/compiler/frontend/src/parser/model.rs#L45-L99)): extend `binding_ref` / `kv_field_block` / `r2_field_block` to additionally accept the `Binding::{ value(a, b), doId(shard) }` form. The proposal's DO-KV needs to distinguish **template args** (`value(key1,key2)`) from **shard args** (`doId(doId)`). Extend `KvFieldBlock`/`R2FieldBlock` ([lib.rs:349-375](../../src/compiler/frontend/src/lib.rs#L349-L375)) with an optional `shard_args: Vec<Symbol>` (or a structured spider list) so the shard discriminator is captured separately from `args`.

### Phase B — IR (idl)

[idl/src/lib.rs](../../src/compiler/idl/src/lib.rs):
- Keep `NavigationField` but enrich `NavigationFieldKind` so it can describe **cross-backing** resolution. At minimum each kind needs: the local→target field mapping (already `fields`/`columns`), plus enough to know the resolution shape (same-DB join vs. multi-step). Recommended: add the target's backing context to `NavigationField` (e.g. resolved `model_reference` already gives the name; downstream can look up backing). Avoid over-designing the query-plan IR here — this step only needs the validated relationship + its key mapping. Document that codegen/query-planner consumes it later.
- Add `shard_args`/spider info to `KvField`/`R2Field` if DO-KV shard discriminators must round-trip to codegen. If the resolved `key_format` + a separate `shard_key_format`/`shard_args` is enough, store that; keep it minimal.

### Phase C — Semantic analysis (semantic)

[semantic/src/model.rs](../../src/compiler/semantic/src/model.rs):
- Rename/rewrite `nav()` → `relation()`. Drive cardinality from the explicit `one`/`many` keyword instead of `local_key.is_some()`.
- **Remove the cross-backing rejection** at [L716-719](../../src/compiler/semantic/src/model.rs#L716-L719) (`NavigationReferencesDifferentBacking`). Replace with validation that, for each backing-pair (D1↔D1 same/diff DB, D1↔DO, DO↔DO, Worker↔*), the **required discriminators are supplied**:
  - Target DO: all shard fields + the route/primary discriminator must be provided (proposal §"D1 -> DO", §"Only Durable Shard Discriminators").
  - Target Worker: must be `one` (or `many` mapped to ≤1); all route fields supplied (keep existing route logic from [L619-711](../../src/compiler/semantic/src/model.rs#L619-L711), but now reachable from any source backing, not just same-binding).
  - Discriminator-less `one Unindexed {}` / `many Post {}` — allowed (proposal §"Unindexed Relationships").
  - Type-match each local field against the referenced target field (reuse existing type-equality checks).
- Keep FK existence checks where they still apply (same-DB 1:1/1:M), but they become **optional** for cross-backing relations (no FK across databases/DOs). Gate FK lookup on "same D1 binding".
- `kv_field()`/`r2_field()` ([L855-971](../../src/compiler/semantic/src/model.rs#L855-L971)): handle the spider form. For DO-KV, validate shard args against the durable binding's `shard` block (`durable_bindings`), and template args against the template `params` (existing `resolve_binding_ref`). Emit an error if a DO template is referenced without its required shard args (proposal §"DO KV").

[semantic/src/err.rs](../../src/compiler/semantic/src/err.rs):
- Rename nav error variants → relation naming. Replace `NavigationReferencesDifferentBacking` with finer-grained variants, e.g. `RelationMissingDiscriminator { field, missing }` (DO shard/route field not supplied) and keep `RelationMixedAdjacency` semantics if still relevant (likely obsolete — explicit cardinality removes mixing). Add `RelationCardinalityInvalid` for `many` to a non-indexable target where the proposal mandates `one`. Update ariadne rendering at L472-575.

### Phase D — Tests

- Migrate every `nav ...` in [analysis_tests.rs](../../src/compiler/semantic/tests/analysis_tests.rs) and any `.cloesce` fixtures to `one`/`many`. Delete tests rendered meaningless by the redesign (don't repurpose them) rather than asserting something unrelated.
- Add positive tests for each proposal scenario: D1→Worker, Worker→D1, D1→DO (spider), DO→D1, DO→DO (spider), D1→D1 (same/diff DB), unindexed `one`/`many`, shard-only `many`, and DO-KV spider.
- Add negative tests: DO relation missing shard discriminator; DO-KV missing `doId`; `many` to a worker target where proposal forbids true listing (assert it's accepted but flagged as ≤1 if that's the chosen semantic).
- `frontend` parser tests (if present) for the new `one`/`many`/spider grammar.

### Out of scope (later phases)
Codegen, runtime query planner, migrations, and the generated TS/Rust. Per scope constraints: where existing code outside `frontend`/`semantic`/`idl` pattern-matches the renamed/changed AST/IR variants and won't compile, **comment out the offending module/crate** (workspace member or `mod` line) — do **not** stub-implement or migrate it. Phase-gate: all frontend + semantic tests must pass before advancing.

## Verification

Build/test scoped to the in-scope crates (downstream crates may be commented out and won't build):
```
cargo build -p cloesce-frontend -p cloesce-semantic   # (idl builds transitively)
cargo test  -p cloesce-frontend   # parser tests
cargo test  -p cloesce-semantic   # analysis_tests + new relation tests
```
(Use the exact crate names from each `Cargo.toml`; `make build-src` only if the commented-out modules still let the workspace build.)
- Confirm a representative cross-backing schema (e.g. proposal's Reddit-clone snippet, reduced) parses and analyzes to a CIDL with the expected `NavigationField` kinds and key mappings (inspect `to_json()` in a test).
- Confirm the old `nav` keyword now fails to parse (negative test) — proves full replacement.
- Confirm DO-KV spider form resolves shard vs. template args correctly and rejects a missing shard discriminator.
