# Hello World Example

This is a minimal Run 2.0 WASI component.

## Build

```bash
cargo component build --release
cp target/wasm32-wasip1/release/hello.wasm .
```

## Run

```bash
run dev
```

Or directly:

```bash
cargo run --features v2 -- dev
```

This proves Run 2.0 can load and execute real WASI components with wasmtime.
