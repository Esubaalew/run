# Run 2.0 Examples

Examples and configuration templates for Run 2.0.

## Working Examples

These examples have complete, buildable code:

### polyglot/
Cross-language composition: Rust calling Python via WIT.

```bash
cd polyglot
run v2 build
run v2 test
```

### polyglot-sdk/
Same WIT interface implemented in Go, JS, TS, and Zig.

```bash
cd polyglot-sdk
run v2 build
```

## Configuration Templates

These show `run.toml` configuration patterns. Add your own component implementations:

| Template | Purpose |
|----------|---------|
| microservices/ | Multi-component orchestration |
| edge-deploy/ | Cloudflare/AWS/Vercel deployment |
| docker-hybrid/ | Docker Compose migration |
| ci-cd/ | Reproducible builds |
| dev-experience/ | Hot reload development |

## Quick Start

```bash
# Working example
cd polyglot
run v2 build
run v2 test

# Configuration template (add your components first)
cd microservices
# Edit run.toml, add your .wasm files
run v2 dev
```
