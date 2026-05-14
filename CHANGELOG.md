# [0.4.0] - Unreleased

### Added

- Basic zod-inspired validators for any field (Model, Poo, API method, Data Source method)
  - string validators: `regex`, `maxlen`, `minlen`, `len`
  - number validators: `gt`, `gte`, `lt`, `lte`, `step`
- `crud` tag for Models
- `instance` tag for Data Source `get` method
- `inject` tag for API methods to inject environment bindings or injected dependencies into the method.

### Changed

- Renamed `double` to `real`.
- Client CRUD methods are split per data source.
- ORM methods return a `CloesceResult` object.
- Key fields now can take a SQLite compatible type.
- `use` tag can no longer specify CRUD methods.
- All built-in types now use lowercase syntax.
- Services no longer have fields, and are not instantiable.
- `self` on a Service API method throws a semantic error
- Removed `env` as a type

### Fixed

# [0.3.9] - 2026-4-19

### Added

### Changed

### Fixed

- A bug where the `impl` function would widen the return type to `ApiResult` instead of inferring the specific return type of the method, making it difficult to work with the results without manually type asserting.

# [0.3.8] - 2026-4-17

### Added

### Changed

- Updated backend TypeScript codegen to reveal an `impl` function which massively reduces boilerplate
- Moved generated DataSources outside of a namespace to reduce boilerplate and improve readability

### Fixed

# [0.3.7] - 2026-4-17

### Added

### Changed

### Fixed

- Misc bugs in `cloesce fmt`
- `"Default"` was not correctly overriding on client crud methods, making users manually specify it.

# [0.3.6] - 2026-4-15

### Added

- Experimental `cloesce fmt` CLI command

### Changed

- Refactored post parsing AST to be lossless enabling `cloesce fmt` to work without assuming structure

### Fixed

# [0.3.5] - 2026-4-9

### Added

### Changed

- Standardized "infix notation" for primary, unique, optional, paginated
- Allow block form of `optional` and `paginated` modifiers in addition to infix

### Fixed

# [0.3.4] - 2026-4-8

### Added

### Changed

- Version checking (via GitHub API) is no longer run every command, instead using a cached value that is updated once an hour.
- `version` command always fetches the latest version from the GitHub API, ignoring the cache.

### Fixed

- GitHub API rate limit issues

# [0.3.3] - 2026-4-8

### Added

- `map`, `select` and `hydrate` methods generated in the namespace of each backend Model for convenience.

### Changed

- CLI print output standardized
- Converted most `except` calls involving file system operations in CLI to display user-friendly error messages.
- Feature gated regression test specific args

### Fixed

- Miscellaneous bug fixes in the CLI
- Timestamp name bug in the migrations CLI

# [0.3.2] - 2026-4-6

### Added

- Standalone Cloesce binary for Windows, Linux and MacOS
- Distribution scripts for the standalone binary
- `cloesce.jsonc` file for configuring the new CLI
- Environment support using the `--env` flag in the CLI
- Support for config files for each environment via `{env}.cloesce.jsonc` (e.g. `production.cloesce.jsonc`)

### Changed

- Removed NPM CLI for Cloesce in favor of the standalone binary

### Fixed

# [0.3.1] - 2026-4-6

### Added

### Changed

### Fixed

- A bug where Windows could not run migrations due to file path issues.

# [0.3.0] - 2026-4-6

### Added

- Cloesce Language
- Compile to a TypeScript RPC-style client, along with ORM functions.

### Changed

- Removed TypeScript extraction process

### Fixed

# [0.2.3] - 2026-3-24

### Added

- Added duplicate model error handling.

### Fixed

- Fixed Windows compilation issues.

# [0.2.2] - 2026-3-14

### Added

### Changed

- If unspecified in ORM functions, Data Sources will default to what is defined
  in the default data source.

### Fixed

# [0.2.0] - 2026-03-14

### Added

- Pagination support for KV, R2 and D1 via the `orm`
- Pagination support for D1 via the `LIST` CRUD method
- Support for `jsonc` Wrangler config files
- Support for composite keys (foreign and primary)
- New fluent API for extended Model definitions
- Support for multiple D1 databases in a single project
- Truncate extraneous fields during runtime validation
- Runtime validation for `DateISO` type
- Private and inline data sources per method
- KV and R2 objects added to the grammar (any method can accept or return these objects)

### Changed

- Moved runtime validation to WASM
- Optimized the ORM WASM binary size by 41% (657KB -> 383KB)
- Refactored Data Sources as per `proposal-1-data-sources.md`
- Removed `@OneToOne` and `@OneToMany` decorators

### Fixed

# [0.1.5] - 2026-2-20

### Added

### Changed

### Fixed

- A bug where `upsert` would not properly replace an undefined value with `null` when the column is nullable.

# [0.1.3] - 2026-2-12

### Added

### Changed

### Fixed

- A single missing value from the wrangler `d1_databases` would crash the generator
- `wrangler` tests were not being ran in CI/CD because of misnamed test folder

# [0.1.2] - 2026-2-3

### Added

### Changed

### Fixed

- boolean data types in the `upsert` expected integers
- boolean data types after `hydrate` should be booleans, not integers

# [0.1.1] - 2026-2-3

### Added

### Changed

### Fixed

- `create-cloesce` template defaulted to a CommonJS export, which is not compatible with ESM. This has been changed to an ESM export.
- A bug with windows file paths during Workers code generation.
