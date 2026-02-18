# [0.2.0] - Unreleased
### Added
- Truncate extraneous fields during runtime validation
- Runtime validation for `DateISO` type

### Changed
- KV and R2 objects added to the grammar (any method can accept or return these objects)
- Moved runtime validation to WASM

### Fixed

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