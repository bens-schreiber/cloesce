# cloesce

View the [internal documentation](https://cloesce.pages.dev)

### Unit Tests

- `src/extractor/ts` run `npm test`
- `src/generator` run `cargo test`

### Integration Tests

- To run the regression tests: `cargo run --bin test regression`
- To run the pass fail extractor tests: `cargo run --bin test pass-fail extractor`

Optionally, pass `--check` if new snapshots should not be created.

### E2E

- `tests/e2e` run `npm test`

### Code Formatting

- `cargo fmt`, `cargo clippy`, `npm run format:fix`
