# [0.6.0] - Unreleased

### Added

- Cloesce Query Planner
- `cloesce explain` command to explain the query plan for a given query
- `one` and `many` syntax
- "spider initializer" syntax (`::{field(value), field2(value2)}`) for navigation properties
- Allow `one` and `many` relationships between any two Models, regardless of their backing
- `hydrate` and `hydrateAll` methods capable of loading any navigation property on any given Model seed

### Changed

- Removed `nav` syntax in favor of the new `one` and `many` syntax
- Removed the old Cloesce ORM in favor of the new Cloesce Query Planner
- Models no longer require unique indices for navigation properties
- Models no longer require relationships to span one database or durable object
- Durable Object KV can be used in any Model, not just those backed by that Durable Object
- Self referential FKs are allowed
- Route fields are allowed on any Model
- All CRUD operations are always supported on all Models

### Fixed

# [0.5.2] - 6/21/2026

### Added

- In-depth documentation with examples for all generated code.

### Changed

- Modified ORM code to reduce the WASM binary by 1.3MB

### Fixed

# [0.5.1] - 6/19/2026

### Added

- Optional `dir` field to `cloesce compile` command to specify the directory to compile in, defaulting to the cwd.

### Changed

- Calling `cloesce compile` in a directory with only a source file will correctly compile instead of throwing an error about a missing Wrangler config / `cloesce.jsonc`

### Fixed

- `self` methods would not inherit the durable context of its associated Data Sources `get` method
- Durable Object shards would not always correctly source themselves from the request context
- Extra Durable Object injects would be added to an API method if it used a DO's KV templates
- Errors would sometimes return `{}` instead of enumerating the error fields due to a bug in the error serialization logic.

# [0.5.0] - 6/18/2026

### Added

- Durable Object backed Models
- Durable Object execution context injection into API methods
- Durable Object Wrangler configuration generation
- Durable Object Migration generation
- Data Source stubs for `get`, `list`, `save`
- Binding templates for `kv`, `r2`, and `durable` allowing the declaration to list all key locations
- New Worker backed models to replace `keyfield`
- Allow D1 backed models to have a navigation property to a Worker backed model, but not the other way around.
- Semantic analysis for key formats overlapping

### Changed

- Removed `keyfield` syntax in favor of the new Worker backed models
- Data sources no longer accept inline SQL queries, instead generating stub functions
- Removed many to many relationship support in favor of manual join tables using composite keys
- Changed syntax of `nav` fields
- Upgraded the Cloudflare Env to the Cloesce Env which adds more methods to access bindings
- Allow a missing `include` block (defaults to default include tree)
- Allow dropping of outer parenthesis in statements that are not composite
- Allow a `nav` field with no keys (1:1 to a singleton model).
- Pass variardic args to `CloesceApp.register` instead of chaining calls
- Removed `paginated` from the schema, instead generating `list` methods on the binding templates for KV, R2 and Durable Objects.
- Removed `prefix` from the `KValue` interface

### Fixed

- A bug with `date` serialization and deserialization using improper ISO strings

# [0.4.1] - 5/20/2026

### Added

- Added `save` to each generated backend Data Source
- Added `include` to the generated ORM `map` method

### Changed

- Removed `service` syntax, opting for the "data-less model" pattern instead.

### Fixed

# [0.4.0] - 5/19/2026

### Added

- Basic zod-inspired validators for any field (Model, Poo, API method, Data Source method)
  - string validators: `regex`, `maxlen`, `minlen`, `len`
  - number validators: `gt`, `gte`, `lt`, `lte`, `step`
- `crud` tag for Models
- `instance` tag for Data Source `get` method
- `inject` tag for API methods to inject environment bindings or injected dependencies into the method.
- `column` block

### Changed

- Renamed `double` to `real`.
- Client CRUD methods are split per data source.
- ORM methods return a `CloesceResult` object.
- Key fields now can take a SQLite compatible type.
- `use` tag can no longer specify CRUD methods.
- All built-in types now use lowercase syntax.
- Services no longer have fields, and are not instantiable. They cannot be injected.
- `self` on a Service API method throws a semantic error.
- Removed `env` as a type.
- Removed `optional` and `paginated` blocks
- `unique` now references existing fields instead of declaring new ones.

### Fixed

# [0.3.9] - 4/19/2026

### Added

### Changed

### Fixed

- A bug where the `impl` function would widen the return type to `ApiResult` instead of inferring the specific return type of the method, making it difficult to work with the results without manually type asserting.

# [0.3.8] - 4/17/2026

### Added

### Changed

- Updated backend TypeScript codegen to reveal an `impl` function which massively reduces boilerplate
- Moved generated DataSources outside of a namespace to reduce boilerplate and improve readability

### Fixed

# [0.3.7] - 4/17/2026

### Added

### Changed

### Fixed

- Misc bugs in `cloesce fmt`
- `"Default"` was not correctly overriding on client crud methods, making users manually specify it.

# [0.3.6] - 4/15/2026

### Added

- Experimental `cloesce fmt` CLI command

### Changed

- Refactored post parsing AST to be lossless enabling `cloesce fmt` to work without assuming structure

### Fixed

# [0.3.5] - 4/9/2026

### Added

### Changed

- Standardized "infix notation" for primary, unique, optional, paginated
- Allow block form of `optional` and `paginated` modifiers in addition to infix

### Fixed

# [0.3.4] - 4/8/2026

### Added

### Changed

- Version checking (via GitHub API) is no longer run every command, instead using a cached value that is updated once an hour.
- `version` command always fetches the latest version from the GitHub API, ignoring the cache.

### Fixed

- GitHub API rate limit issues

# [0.3.3] - 4/8/2026

### Added

- `map`, `select` and `hydrate` methods generated in the namespace of each backend Model for convenience.

### Changed

- CLI print output standardized
- Converted most `except` calls involving file system operations in CLI to display user-friendly error messages.
- Feature gated regression test specific args

### Fixed

- Miscellaneous bug fixes in the CLI
- Timestamp name bug in the migrations CLI

# [0.3.2] - 4/6/2026

### Added

- Standalone Cloesce binary for Windows, Linux and MacOS
- Distribution scripts for the standalone binary
- `cloesce.jsonc` file for configuring the new CLI
- Environment support using the `--env` flag in the CLI
- Support for config files for each environment via `{env}.cloesce.jsonc` (e.g. `production.cloesce.jsonc`)

### Changed

- Removed NPM CLI for Cloesce in favor of the standalone binary

### Fixed

# [0.3.1] - 4/6/2026

### Added

### Changed

### Fixed

- A bug where Windows could not run migrations due to file path issues.

# [0.3.0] - 4/6/2026

### Added

- Cloesce Language
- Compile to a TypeScript RPC-style client, along with ORM functions.

### Changed

- Removed TypeScript extraction process

### Fixed

# [0.2.3] - 3/24/2026

### Added

- Added duplicate model error handling.

### Fixed

- Fixed Windows compilation issues.

# [0.2.2] - 3/14/2026

### Added

### Changed

- If unspecified in ORM functions, Data Sources will default to what is defined
  in the default data source.

### Fixed

# [0.2.0] - 3/14/2026

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

# [0.1.5] - 2/20/2026

### Added

### Changed

### Fixed

- A bug where `upsert` would not properly replace an undefined value with `null` when the column is nullable.

# [0.1.3] - 2/12/2026

### Added

### Changed

### Fixed

- A single missing value from the wrangler `d1_databases` would crash the generator
- `wrangler` tests were not being ran in CI/CD because of misnamed test folder

# [0.1.2] - 2/3/2026

### Added

### Changed

### Fixed

- boolean data types in the `upsert` expected integers
- boolean data types after `hydrate` should be booleans, not integers

# [0.1.1] - 2/3/2026

### Added

### Changed

### Fixed

- `create-cloesce` template defaulted to a CommonJS export, which is not compatible with ESM. This has been changed to an ESM export.
- A bug with windows file paths during Workers code generation.
