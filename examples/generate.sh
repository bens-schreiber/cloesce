# note: temporary script for v0.0.1 until npm orchestrates everything


# extract cidl
npx cloesce

# generate via rust
cd ../src/generator
cargo run generate d1 ../../examples/.generated/cidl.json ../../examples/migrations/d1.sql ../../examples/wrangler.toml
cargo run generate workers ../../examples/.generated/cidl.json ../../examples/.generated/workers.ts
cargo run generate client ../../examples/.generated/cidl.json ../../examples/.generated/client.ts api

# migrate wrangler
cd ../../examples/
echo y | npx wrangler d1 migrations apply example

# build
npx wrangler build

# run wrangler
npx wrangler dev --port 5000
