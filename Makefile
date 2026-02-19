.PHONY: format
format:
	@echo "CLOESCE: Formatting Rust and TypeScript code..."

	npm run format:fix --prefix src/ts
	cargo fmt --all --manifest-path src/orm/Cargo.toml
	cargo fmt --all --manifest-path src/generator/Cargo.toml
	npm run format:fix --prefix tests/e2e
	cargo fmt --all --manifest-path tests/regression/Cargo.toml

.PHONY: format-check
format-check:
	@echo "CLOESCE: Checking Rust and TypeScript code formatting..."

	cargo fmt --all --manifest-path src/orm/Cargo.toml -- --check 
	cargo clippy --manifest-path src/orm/Cargo.toml --all-targets --all-features -- -D warnings
	
	cargo fmt --all --manifest-path src/generator/Cargo.toml -- --check
	cargo clippy --manifest-path src/generator/Cargo.toml --all-targets --all-features -- -D warnings

	npm run format --prefix src/ts -- --check
	npx --prefix src/ts oxlint . --deny-warnings

	npm run format --prefix tests/e2e -- --check
	npx --prefix tests/e2e oxlint . --deny-warnings

.PHONY: build-src
build-src:
	@echo "CLOESCE: Building Rust and TypeScript code..."

	cargo build --target wasm32-unknown-unknown --release --manifest-path src/orm/Cargo.toml
	cargo build --target wasm32-wasip1 --release --manifest-path src/generator/Cargo.toml

	npm install --prefix src/ts
	npm run build --prefix src/ts

	npm install --prefix tests/e2e

.PHONY: test
test:
	@echo "CLOESCE: Running tests for Rust and TypeScript code..."

	cargo test --manifest-path src/orm/Cargo.toml
	cargo test --manifest-path src/generator/Cargo.toml
	npm run test --prefix src/ts
	cargo run --manifest-path tests/regression/Cargo.toml --bin regression -- --check
	npm run test --prefix tests/e2e

# note: requires pandoc to be installed
# https://pandoc.org/installing.html
.PHONY: build-docs
build-docs:
	@echo "CLOESCE: Building documentation for Rust and TypeScript code..."

	cd docs && mdbook-langtabs install
	cd docs && mdbook build
	cat docs/src/*.md > docs/book/llms-full.md
	pandoc docs/book/llms-full.md -o docs/book/llms-full.txt

.PHONY: build-typedoc
build-typedoc:
	@echo "CLOESCE: Building TypeScript documentation using TypeDoc..."
	
	cd ./src/ts && npx typedoc --out build

.PHONY: all
all: format build-src format-check build-docs build-typedoc test
