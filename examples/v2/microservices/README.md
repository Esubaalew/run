# Microservices Example

Configuration template for a multi-component HTTP API.

## Structure

This example shows the `run.toml` configuration for running multiple WASI components as microservices. The actual service implementations would be built separately using `cargo-component`.

## Files

- `run.toml` - Orchestration configuration
- `wit/api.wit` - Interface definitions

## Usage

To use this template:

1. Create your service implementations in Rust/Python/etc.
2. Build them with `cargo-component` or `componentize-py`
3. Update paths in `run.toml` to point to your `.wasm` files
4. Run with `run v2 dev`
