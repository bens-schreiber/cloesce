.PHONY: build
build:
	cd src && \
	cd orm && cargo build --target wasm32-unknown-unknown --release && \
	cd ../generator && cargo build --target wasm32-wasip1 --release && \
	cd ../ts && npm run build

.PHONY: test
test:
	cd src/ts && npm run test && \
	cd ../orm && cargo test && \
	cd ../generator && cargo test && \
	cd ../../tests/runner && cargo run --bin regression && \
	cd ../e2e && npm run test


.PHONY: format
format:
	cd src && \
	cd ts && npm run format:fix && \
	cd ../orm && cargo fmt --all && \
	cd ../generator && cargo fmt --all && \
	cd ../../tests/e2e && npm run format:fix && \
	cd ../runner && cargo fmt --all