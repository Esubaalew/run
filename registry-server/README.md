# Run Registry

Minimal but real WASI component registry with persistence and auth.

## What it does

- Stores WASI components with immutable versions
- SHA-256 verification on publish
- Namespace ownership via API tokens
- Local persistence via SQLite + on-disk artifacts
- HTTP API matching the Run 2.0 client
- Search endpoint for discovery

## What it does NOT do (yet)

- Signature verification
- Social features
- Malware scanning

## API

```
POST /api/v1/packages                    - Publish component (auth required)
GET  /api/v1/packages/:name/versions     - List versions
GET  /api/v1/packages/:name/:version     - Get metadata
GET  /packages/:name/:version/artifact.wasm - Download component
GET  /api/v1/search?q=<query>            - Search packages
GET  /api/v1/stats                       - Registry stats
GET  /health                             - Health check
```

## Naming

Format: `<namespace>:<name>@<version>`

Examples:
- `wasi:http@0.2.0`
- `run:calc/calculator@0.1.0`

## Run

```bash
cargo run --release
```

Listens on `http://0.0.0.0:$PORT` (defaults to `8080` if `PORT` is not set)

## Configuration

Environment variables:

- `REGISTRY_URL` - Base URL used in metadata (default `http://localhost:8080`)
- `REGISTRY_DATA_DIR` - Data directory (default `./registry-data`)
- `PORT` - HTTP listen port (default `8080`)
- `REGISTRY_ADMIN_TOKEN` - Admin token (namespace `*`)
- `REGISTRY_TOKENS` - Comma-separated list `namespace:token`
- `REGISTRY_MAX_UPLOAD_MB` - Upload limit (default `50`)
- `REGISTRY_RATE_LIMIT_PER_MIN` - Global rate limit (default `120`)

## Publish a component

```bash
curl -X POST http://localhost:8080/api/v1/packages \
  -H "Authorization: Bearer <TOKEN>" \
  -F "name=run:example/hello" \
  -F "version=1.0.0" \
  -F "description=Hello world component" \
  -F "license=MIT" \
  -F "sha256=<sha256>" \
  -F "artifact=@hello.wasm"
```

## Install from registry

Update `run.toml`:

```toml
[registry]
url = "http://localhost:8080"  # local
# url = "https://registry.esubalew.dev"  # production
```

Then:

```bash
run v2 install run:example/hello@1.0.0
```

## Status

This is a minimal registry with persistence and auth. For production, add backups, monitoring, and object storage.

## Non-goals

This is NOT a full package registry. It's the minimum to make `run install` work reliably.
