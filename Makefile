# Project directories
COMPILER_DIR := src/compiler
TS_DIR := src/runtime/ts
E2E_DIR := tests/e2e
REGRESSION_DIR := tests/regression
DOCS_DIR := docs

# Cargo workspace manifest (root)
CARGO_MANIFEST := Cargo.toml

# WASM target (ORM only)
WASM_TARGET := wasm32-unknown-unknown

.PHONY: check-deps
check-deps:
	@echo "CLOESCE: Checking required dependencies..."
	@command -v cargo >/dev/null 2>&1 || { echo "❌ cargo not found. Install Rust: https://rustup.rs/"; exit 1; }
	@command -v npm >/dev/null 2>&1 || { echo "❌ npm not found. Install Node.js: https://nodejs.org/"; exit 1; }
	@command -v pandoc >/dev/null 2>&1 || echo "⚠️  pandoc not found (optional, for docs). Install pandoc: https://github.com/jgm/pandoc"
	@command -v mdbook >/dev/null 2>&1 || echo "⚠️  mdbook not found (optional, for docs). Install mdbook: cargo install mdbook"
	@rustup target list --installed | grep -q $(WASM_TARGET) || { echo "❌ $(WASM_TARGET) target not installed. Run: rustup target add $(WASM_TARGET)"; exit 1; }
	@echo "✅ All required dependencies found!"

format:
	@echo "CLOESCE: Formatting Rust and TypeScript code..."
	npm run format:fix --prefix $(TS_DIR)
	cargo fmt --all --manifest-path $(CARGO_MANIFEST)
	npm run format:fix --prefix $(E2E_DIR)

.PHONY: format-check
format-check:
	@echo "CLOESCE: Checking Rust and TypeScript code formatting..."
	cargo fmt --all --manifest-path $(CARGO_MANIFEST) -- --check
	npm run format --prefix $(TS_DIR) -- --check
	npm run format --prefix $(E2E_DIR) -- --check

	@echo "CLOESCE: Linting Rust and TypeScript code..."
	cargo clippy --manifest-path $(CARGO_MANIFEST) --all-targets --all-features -- -D warnings
	npx --prefix $(TS_DIR) oxlint . --deny-warnings
	npx --prefix $(E2E_DIR) oxlint . --deny-warnings

# Cross-compilation targets for release binaries
CROSS_TARGETS := \
	x86_64-unknown-linux-gnu \
	aarch64-unknown-linux-gnu \
	x86_64-apple-darwin \
	aarch64-apple-darwin \
	x86_64-pc-windows-msvc

# Maps a Rust target triple to the release asset name
define asset_name
cloesce-compiler-$(subst x86_64-unknown-linux-gnu,x86_64-linux,$(subst aarch64-unknown-linux-gnu,aarch64-linux,$(subst x86_64-apple-darwin,x86_64-macos,$(subst aarch64-apple-darwin,aarch64-macos,$(subst x86_64-pc-windows-msvc,x86_64-windows,$(1))))))
endef

# Build a single cross-compile target: make build-cross TARGET=<triple> [USE_CROSS=1]
.PHONY: build-cross
build-cross:
	@[ -n "$(TARGET)" ] || { echo "Usage: make build-cross TARGET=<triple> [USE_CROSS=1]"; exit 1; }
	@echo "CLOESCE: Building CLI binaries for $(TARGET)..."
	@if [ "$(USE_CROSS)" = "1" ]; then \
		cross build --release --target $(TARGET) --manifest-path $(COMPILER_DIR)/cli/Cargo.toml --bin cloesce; \
	else \
		cargo build --release --target $(TARGET) --manifest-path $(COMPILER_DIR)/cli/Cargo.toml --bin cloesce; \
	fi

# Package a single cross-compile target into a release archive.
# Produces dist/<asset_name>.tar.gz (or .zip on Windows targets).
# Usage: make package-cross TARGET=<triple>
.PHONY: package-cross
package-cross:
	@[ -n "$(TARGET)" ] || { echo "Usage: make package-cross TARGET=<triple>"; exit 1; }
	$(eval ASSET := $(call asset_name,$(TARGET)))
	mkdir -p dist
	@if echo "$(TARGET)" | grep -q "windows"; then \
		cp target/$(TARGET)/release/cloesce.exe dist/cloesce.exe; \
		cd dist && 7z a $(ASSET).zip cloesce.exe && rm cloesce.exe; \
		echo "CLOESCE: Packaged dist/$(ASSET).zip"; \
	else \
		cp target/$(TARGET)/release/cloesce dist/cloesce; \
		cd dist && tar -czf $(ASSET).tar.gz cloesce && rm cloesce; \
		echo "CLOESCE: Packaged dist/$(ASSET).tar.gz"; \
	fi

.PHONY: build-src
build-src:
	@echo "CLOESCE: Installing dependencies for Rust and TypeScript code..."
	npm install --prefix $(TS_DIR)
	npm install --prefix $(E2E_DIR)

	@echo "CLOESCE: Building Rust and TypeScript code..."
	cargo build --release --manifest-path $(CARGO_MANIFEST) --bin cloesce
	cargo build --target $(WASM_TARGET) --release --manifest-path $(COMPILER_DIR)/orm/Cargo.toml

	npm run build --prefix $(TS_DIR)

.PHONY: test
test:
	@echo "CLOESCE: Running tests for Rust and TypeScript code..."
	cargo test --manifest-path $(CARGO_MANIFEST) --all-features
	npm run test --prefix $(TS_DIR)
	cargo run --manifest-path $(CARGO_MANIFEST) --bin regression -- --check
	npm run test --prefix $(E2E_DIR)

.PHONY: build-docs
build-docs:
	@echo "CLOESCE: Building documentation for Rust and TypeScript code..."
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
	cargo clean --manifest-path $(CARGO_MANIFEST)
	rm -rf $(TS_DIR)/build
	rm -rf $(TS_DIR)/node_modules
	rm -rf $(E2E_DIR)/node_modules
	rm -rf $(DOCS_DIR)/book
	@echo "✅ Build artifacts removed!"

.PHONY: all
all: check-deps format build-src format-check build-docs build-typedoc test