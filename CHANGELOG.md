# [0.2.0] - Unreleased
### Added
- Truncate extraneous fields during runtime validation
- Runtime validation for `DateISO` type
- Private and inline data sources per method

### Changed
- KV and R2 objects added to the grammar (any method can accept or return these objects)
- Moved runtime validation to WASM
- Optimized the ORM WASM binary size by 41% (657KB -> 383KB)
- Refactored Data Sources as per `proposal-1-data-sources.md`

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
