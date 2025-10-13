# Common runtime (ensure wasm is installed, `rustup target add wasm32-unknown-unknown`)
cd ./runtime
cargo build --target wasm32-unknown-unknown --release
cd ..

# Generator (ensure wasi is installed, `rustup target add wasm32-wasip1`)
cd ./generator
cargo build --target wasm32-wasip1 --release
cd ..

# Frontend TS
cd ./frontend/ts
npm run build
