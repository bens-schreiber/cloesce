# [0.1.3] - 2026-2-3

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