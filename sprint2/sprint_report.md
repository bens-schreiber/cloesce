# Sprint 2 Report (v0.0.4)
*(9-30-2025 to 11-5-2025)*

## YouTube link of Sprint 2 Video  
*(Make this video unlisted)*  
[Unlisted Sprint 2 Demo Video](https://youtu.be/orEsxiUWxfo)

---

## What's New (User Facing)

* Introduced a **migrations engine** to manage SQLite schema evolution via `cloesce compile --migrations add` and `update`.
* Added **middleware support** for multiple scopes (global, model, and method-level) with customizable logic chains.
* Developed **ORM primitives in WASM** for CRUD operations (`create`, `read`, `update`, `delete`).
* Added **automatic CRUD endpoint generation** based on model decorators.
* Improved **authentication pipeline** using dependency injection and scoped middleware patterns.
* Finalized the **WASI build system** for the Generator, enabling universal deployment.
* Completed **semantic analysis** integration with structural hashing (Merkle trees) for CIDL change detection.

---

## Work Summary (Developer Facing)

This sprint focused on solidifying the core architecture needed to make Cloesce a fully functional compiler and runtime ecosystem.  
We successfully decoupled SQL schema management from the main compilation process by introducing a dedicated migrations engine, using structural hashing to track AST-level changes between versions. The development team worked extensively on the middleware system—adding support for scoped authentication logic at multiple levels of the request lifecycle—while ensuring the runtime could still inject dependencies like authenticated users and environment variables.  
Additionally, the team moved ORM logic into WASM, achieving platform-agnostic CRUD functionality and reducing language-specific inconsistencies. The generator’s WASI build was finalized, simplifying binary distribution. This sprint also required deep semantic validation to ensure that models, data sources, and migrations remained consistent across generated artifacts.

---

## Unfinished Work

Some planned optimizations to the ORM’s **upsert algorithm** and **hydration routines** remain unimplemented.  
Additionally, the **Rename decorator** and **interactive migration prompt** for resolving entity renames were designed but deferred to v0.0.5 to allow more testing of the base migration engine.

---

## Completed Issues/User Stories
- [#28](https://github.com/users/bens-schreiber/projects/9?pane=issue&itemId=127962871)  
- [#30](https://github.com/users/bens-schreiber/projects/9?pane=issue&itemId=127963606)  
- [#46](https://github.com/users/bens-schreiber/projects/9?pane=issue&itemId=130386296)  
- [#47](https://github.com/users/bens-schreiber/projects/9?pane=issue&itemId=130386749)  
- [#49](https://github.com/users/bens-schreiber/projects/9?pane=issue&itemId=130388427)  
- [#52](https://github.com/users/bens-schreiber/projects/9?pane=issue&itemId=130390234)  
- [#55](https://github.com/users/bens-schreiber/projects/9?pane=issue&itemId=130391130)
---

## Incomplete Issues/User Stories

Some work from this sprint could not be completed due to unexpected technical complexities:

1. **[#99 – Migrations Issues Dump](https://github.com/bens-schreiber/cloesce/issues/99):**  
   The migration scripts uncovered deeper schema conflicts that required additional debugging time.

2. **[#54 – Model-less Functions](https://github.com/bens-schreiber/cloesce/issues/54):**  
   Implementation delayed due to dependency refactors required to remove model-based logic.

3. **[#51 – Add Arrays/Objects to GET Requests](https://github.com/bens-schreiber/cloesce/issues/51):**  
   Testing stalled due to complications handling nested parameters in the API.

---

## Code Files for Review

Most files in the [Cloesce Repository](https://github.com/bens-schreiber/cloesce) were modified due to major refactoring.

---

## Retrospective Summary

### What Went Well
- Cleaned up major bugs and architectural flaws  
- Unified Cloesce under a single CLI for easier deployment  
- Added crucial authentication and migration support  

### Areas for Improvement
- Improve **time management** (balance school and work)  
- Better **documentation of progress** for Capstone tracking  

### Next Sprint Plans
- Bug fixes and extended testing  
- Add array/object handling to GET requests  
- Complete middleware implementation for frontend integration  
