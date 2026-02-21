# Project directories
ORM_DIR := src/orm
GEN_DIR := src/generator
TS_DIR := src/ts
E2E_DIR := tests/e2e
REGRESSION_DIR := tests/regression
DOCS_DIR := docs

# Cargo manifests
ORM_MANIFEST := $(ORM_DIR)/Cargo.toml
GEN_MANIFEST := $(GEN_DIR)/Cargo.toml
REGRESSION_MANIFEST := $(REGRESSION_DIR)/Cargo.toml

# WASM targets
WASM_TARGET := wasm32-unknown-unknown
WASI_TARGET := wasm32-wasip1

.PHONY: check-deps
check-deps:
	@echo "CLOESCE: Checking required dependencies..."
	@command -v cargo >/dev/null 2>&1 || { echo "❌ cargo not found. Install Rust: https://rustup.rs/"; exit 1; }
	@command -v npm >/dev/null 2>&1 || { echo "❌ npm not found. Install Node.js: https://nodejs.org/"; exit 1; }
	@command -v wasm-opt >/dev/null 2>&1 || echo "⚠️  wasm-opt not found (optional). Install wasm-opt: https://github.com/WebAssembly/binaryen"
	@command -v pandoc >/dev/null 2>&1 || echo "⚠️  pandoc not found (optional, for docs). Install pandoc: https://github.com/jgm/pandoc"
	@command -v mdbook >/dev/null 2>&1 || echo "⚠️  mdbook not found (optional, for docs). Install mdbook: cargo install mdbook"
	@rustup target list --installed | grep -q $(WASM_TARGET) || { echo "❌ $(WASM_TARGET) target not installed. Run: rustup target add $(WASM_TARGET)"; exit 1; }
	@rustup target list --installed | grep -q $(WASI_TARGET) || { echo "❌ $(WASI_TARGET) target not installed. Run: rustup target add $(WASI_TARGET)"; exit 1; }
	@echo "✅ All required dependencies found!"

format:
	@echo "CLOESCE: Formatting Rust and TypeScript code..."
	npm run format:fix --prefix $(TS_DIR)
	cargo fmt --all --manifest-path $(ORM_MANIFEST)
	cargo fmt --all --manifest-path $(GEN_MANIFEST)
	npm run format:fix --prefix $(E2E_DIR)
	cargo fmt --all --manifest-path $(REGRESSION_MANIFEST)

.PHONY: format-check
format-check:
	@echo "CLOESCE: Checking Rust and TypeScript code formatting..."
	cargo fmt --all --manifest-path $(ORM_MANIFEST) -- --check 
	cargo fmt --all --manifest-path $(GEN_MANIFEST) -- --check
	cargo fmt --all --manifest-path $(REGRESSION_MANIFEST) -- --check
	npm run format --prefix $(TS_DIR) -- --check
	npm run format --prefix $(E2E_DIR) -- --check

	@echo "CLOESCE: Linting Rust and TypeScript code..."
	cargo clippy --manifest-path $(ORM_MANIFEST) --all-targets --all-features -- -D warnings
	cargo clippy --manifest-path $(GEN_MANIFEST) --all-targets --all-features -- -D warnings
	npx --prefix $(TS_DIR) oxlint . --deny-warnings
	npx --prefix $(E2E_DIR) oxlint . --deny-warnings

.PHONY: build
build: build-src

.PHONY: build-src
build-src:
	@echo "CLOESCE: Installing dependencies for Rust and TypeScript code..."
	npm install --prefix $(TS_DIR)
	npm install --prefix $(E2E_DIR)

	@echo "CLOESCE: Building Rust and TypeScript code..."
	cargo build --target $(WASM_TARGET) --release --manifest-path $(ORM_MANIFEST)
	
	@if command -v wasm-opt >/dev/null 2>&1; then \
		echo "CLOESCE: Optimizing WASM with wasm-opt..."; \
		wasm-opt -Oz --enable-bulk-memory --strip-debug --strip-producers \
			$(ORM_DIR)/target/$(WASM_TARGET)/release/orm.wasm \
			-o $(ORM_DIR)/target/$(WASM_TARGET)/release/orm.wasm; \
	else \
		echo "CLOESCE: wasm-opt not found, skipping WASM optimization (https://github.com/WebAssembly/binaryen)"; \
	fi

	cargo build --target $(WASI_TARGET) --release --manifest-path $(GEN_MANIFEST)
	npm run build --prefix $(TS_DIR)

.PHONY: test
test:
	@echo "CLOESCE: Running tests for Rust and TypeScript code..."
	cargo test --manifest-path $(ORM_MANIFEST)
	cargo test --manifest-path $(GEN_MANIFEST)
	npm run test --prefix $(TS_DIR)
	cargo run --manifest-path $(REGRESSION_MANIFEST) --bin regression -- --check
	npm run test --prefix $(E2E_DIR)

.PHONY: build-docs
build-docs:
	@echo "CLOESCE: Building documentation for Rust and TypeScript code..."
	cd $(DOCS_DIR) && mdbook-langtabs install
	cd $(DOCS_DIR) && mdbook build

	@if command -v pandoc >/dev/null 2>&1; then \
		cat $(DOCS_DIR)/src/*.md > $(DOCS_DIR)/book/llms-full.md; \
		echo "CLOESCE: Converting Markdown documentation to plain text with pandoc..."; \
		pandoc $(DOCS_DIR)/book/llms-full.md -o $(DOCS_DIR)/book/llms-full.txt; \
	else \
		echo "CLOESCE: pandoc not found, skipping llms-full.txt generation"; \
	fi

.PHONY: build-typedoc
build-typedoc:
	@echo "CLOESCE: Building TypeScript documentation using TypeDoc..."
	cd ./$(TS_DIR) && npx typedoc --out build

.PHONY: clean
clean:
	@echo "CLOESCE: Cleaning build artifacts..."
	cargo clean --manifest-path $(ORM_MANIFEST)
	cargo clean --manifest-path $(GEN_MANIFEST)
	cargo clean --manifest-path $(REGRESSION_MANIFEST)
	rm -rf $(TS_DIR)/build
	rm -rf $(TS_DIR)/node_modules
	rm -rf $(E2E_DIR)/node_modules
	rm -rf $(DOCS_DIR)/book
	@echo "✅ Build artifacts removed!"

.PHONY: all
all: check-deps format  build-src format-check build-docs build-typedoc test
