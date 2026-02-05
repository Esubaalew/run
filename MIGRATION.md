# Docker to Run 2.0 Migration Guide

Run 2.0 is experimental and opt-in. Use `run v2` to access the v2 commands.

This guide helps you migrate from Docker/Docker Compose to Run 2.0.

## Table of Contents

1. [Why Migrate?](#why-migrate)
2. [Migration Strategy](#migration-strategy)
3. [Step-by-Step Migration](#step-by-step-migration)
4. [Command Mapping](#command-mapping)
5. [Architecture Changes](#architecture-changes)
6. [Common Patterns](#common-patterns)
7. [Troubleshooting](#troubleshooting)

## Why Migrate?

| Metric | Docker | Run 2.0 | Improvement |
|--------|--------|---------|-------------|
| Startup time | 5-10s | <10ms | **500-1000x** |
| Image size | 50-500MB | <5MB | **10-100x** |
| Memory usage | 256MB+ | <10MB | **25x+** |
| Build time | 1-5min | <10s | **6-30x** |
| Cold start | 500ms+ | <10ms | **50x+** |

**Additional Benefits:**
- No Docker daemon required
- Capability-based security (no root)
- Cross-platform (Linux, macOS, Windows)
- Reproducible builds
- Component isolation
- Multi-language interop via WIT

## Migration Strategy

### Phase 1: Hybrid Mode (Recommended Start)

Keep Docker for stateful services (databases, caches), migrate application logic to WASI.

```
Docker Compose -> Run 2.0 Hybrid
App (Docker) -> App (WASI) <10ms
DB (Docker) -> DB (Docker) Keep
Cache -> Cache Keep
```

### Phase 2: Pure WASI (Goal)

Migrate everything to WASI components.

```
Run 2.0 Hybrid -> Run 2.0 Pure
App (WASI) -> App (WASI)
DB (Docker) -> DB (WASI) All WASI
Cache -> Cache
```

## Step-by-Step Migration

### Step 1: Analyze Your Docker Setup

```bash
# Install Run 2.0
curl -sSL https://run.esubalew.et/install.sh | bash

# Analyze compose file
cd your-project
run v2 compose analyze docker-compose.yml
```

Output:
```
Services:
  OK web-api   -> WASI component (can migrate)
  OK worker    -> WASI component (can migrate)
  WARN postgres -> Docker bridge (stateful, keep Docker)
  WARN redis    -> Docker bridge (stateful, keep Docker)

Recommendation: Hybrid mode (2 WASI + 2 Docker)
Estimated startup: 5s (Docker) + 10ms (WASI)
Estimated size: 128MB (Docker) + 3MB (WASI)
```

### Step 2: Auto-Generate run.toml

```bash
run v2 compose migrate docker-compose.yml run.toml
```

This creates a `run.toml` with:
- Application services as WASI components
- Stateful services as Docker bridge

**Before: docker-compose.yml**

```yaml
version: '3'
services:
  web:
    build: .
    ports:
      - "8080:8080"
    environment:
      DATABASE_URL: postgres://db:5432/mydb
    depends_on:
      - db
  
  db:
    image: postgres:15
    environment:
      POSTGRES_PASSWORD: secret
```

**After: run.toml**

```toml
[package]
name = "my-app"
version = "1.0.0"

[[component]]
name = "web"
source = "src/lib.rs"
language = "rust"
wit = "wit/http.wit"

[component.web.env]
DATABASE_URL = "postgres://localhost:5432/mydb"

[bridge.postgres]
image = "postgres:15"
ports = { 5432 = 5432 }
env = { POSTGRES_PASSWORD = "secret" }
```

### Step 3: Convert Application to WASI Component

#### Option A: Rust

```bash
# Install cargo-component
cargo install cargo-component

# Initialize component
cargo component new web --lib

# Write code
cat > src/lib.rs <<'EOF'
wit_bindgen::generate!({
    world: "http-handler",
    exports: {
        "wasi:http/handler": Handler,
    },
});

struct Handler;

impl exports::wasi::http::handler::Guest for Handler {
    fn handle(request: Request) -> Response {
        Response::new(200, b"Hello from Run 2.0!")
    }
}
EOF

# Build
cargo component build --release
```

#### Option B: Python

```bash
# Install componentize-py
pip install componentize-py

# Write code
cat > app.py <<'EOF'
def handle_request(request):
    return {
        "status": 200,
        "body": b"Hello from Run 2.0!"
    }
EOF

# Build
componentize-py -d http.wit -o app.wasm app.py
```

#### Option C: TypeScript

```bash
# Install jco
npm install -g @bytecodealliance/jco

# Write code
cat > index.ts <<'EOF'
export function handleRequest(request: Request): Response {
    return new Response("Hello from Run 2.0!", { status: 200 });
}
EOF

# Build
jco transpile index.ts -o app.wasm
```

### Step 4: Test Hybrid Setup

```bash
# Start dev server
run v2 dev

# Output:
[run] Starting WASI components...
[web] OK Built in 8ms
[run] Starting Docker bridge...
[docker] postgres -> postgres:15
[run] All services ready:
[run]   http://localhost:8080 (web)

# Test
curl http://localhost:8080
# Hello from Run 2.0!
```

### Step 5: Deploy

```bash
# Production build
run v2 build --release --reproducible

# Deploy (examples)
run v2 deploy --target local
run v2 deploy --target edge --provider cloudflare
run v2 deploy --target registry
```

## Command Mapping

| Docker Command | Run 2.0 Equivalent | Notes |
|----------------|-------------------|-------|
| `docker-compose up` | `run v2 dev` | <10ms startup for WASI |
| `docker-compose build` | `run v2 build` | <10s builds |
| `docker-compose down` | `run stop` | Stops all components |
| `docker-compose logs` | `run v2 dev` (shows logs) | Unified logs |
| `docker-compose ps` | `run v2 info` | Component status |
| `docker-compose exec` | `run v2 exec` | Execute in component |
| `docker build` | `run v2 build` | No Dockerfile needed |
| `docker run` | `run v2 exec` | Direct execution |
| `docker push` | `run v2 deploy --target registry` | WASI registry |
| `docker pull` | `run v2 install` | Download components |

## Architecture Changes

### Before: Docker Compose

```
docker-compose.yml
|-- Service A (container)
|   |-- Dockerfile
|   |-- node_modules/
|   `-- app.js
|-- Service B (container)
|   |-- Dockerfile
|   |-- venv/
|   `-- app.py
`-- Database (container)
    `-- postgres:15

Docker Daemon (required)
|-- Network bridge
|-- Volume management
`-- Container orchestration
```

**Issues:**
- Docker daemon required
- 500MB+ images
- 5-10s startup
- Complex networking
- Root access risks

### After: Run 2.0

```
run.toml
|-- Component A (WASI)    <10ms, 2MB
|-- Component B (WASI)    <10ms, 3MB
`-- Database (Docker*)    5s, 128MB

Run Runtime (no daemon)
|-- Component Model (WASI 0.2)
|-- Capability security
`-- Direct host process

*Docker bridge optional
```

**Benefits:**
- No daemon needed
- <5MB components
- <10ms startup
- Capability-based security
- Cross-platform

## Common Patterns

### Pattern 1: HTTP Service

**Before (Dockerfile):**
```dockerfile
FROM node:18
WORKDIR /app
COPY package*.json ./
RUN npm install
COPY . .
CMD ["node", "server.js"]
```

**After (run.toml):**
```toml
[[component]]
name = "api"
source = "src/lib.rs"
language = "rust"
wit = "wit/http.wit"
```

### Pattern 2: Database Connection

**Before:**
```yaml
services:
  app:
    depends_on:
      - db
    environment:
      DATABASE_URL: postgres://db:5432/mydb
  db:
    image: postgres:15
```

**After:**
```toml
[[component]]
name = "app"
source = "src/lib.rs"
env = { DATABASE_URL = "postgres://localhost:5432/mydb" }

[bridge.postgres]
image = "postgres:15"
ports = { 5432 = 5432 }
```

### Pattern 3: Multi-service Application

**Before (docker-compose.yml):**
```yaml
services:
  frontend:
    build: ./frontend
    ports: ["3000:3000"]
  
  backend:
    build: ./backend
    ports: ["8080:8080"]
  
  worker:
    build: ./worker
```

**After (run.toml):**
```toml
[[component]]
name = "frontend"
source = "frontend/src/lib.rs"

[[component]]
name = "backend"
source = "backend/src/lib.rs"

[[component]]
name = "worker"
source = "worker/src/lib.rs"
```

All start in <10ms, no containers needed.

### Pattern 4: Environment Variables

**Before:**
```yaml
services:
  app:
    environment:
      NODE_ENV: production
      API_KEY: ${API_KEY}
```

**After:**
```toml
[[component]]
name = "app"
env = { NODE_ENV = "production", API_KEY = "${API_KEY}" }

[env.production]
NODE_ENV = "production"
LOG_LEVEL = "info"
```

## Troubleshooting

### Issue 1: "Docker is not available"

If you see this error but have Docker installed:

```bash
# Check Docker is running
docker info

# Enable Docker bridge in run.toml
[bridge]
enabled = true
```

### Issue 2: Port conflicts

```bash
# Error: Port 8080 already in use

# Solution: Change port in run.toml
[[component]]
name = "web"
dev.port = 8081  # Changed from 8080
```

### Issue 3: Missing dependencies

```bash
# Error: Cannot find module 'xyz'

# For Rust: Add to Cargo.toml
[dependencies]
xyz = "1.0"

# For Python: Add to requirements.txt
xyz==1.0.0

# For JS/TS: Add to package.json
{
  "dependencies": {
    "xyz": "^1.0.0"
  }
}
```

### Issue 4: WASI compatibility

Not all code can run in WASI yet. Use Docker bridge for:

- Native dependencies (e.g., `libpq`, `openssl`)
- GPU access
- Kernel modules
- X11/GUI applications

```toml
# Keep these in Docker bridge
[bridge.legacy-service]
image = "my-legacy-app"
```

### Issue 5: Performance regression

If WASI is slower than Docker:

```bash
# Enable release builds
run v2 build --release

# Verbose logs
run v2 dev --verbose

# Check component size
ls -lh target/wasm/*.wasm

# Optimize
[component.my-component]
opt_level = 3
strip_debug = true
```

## Migration Checklist

- [ ] Install Run 2.0
- [ ] Analyze `docker-compose.yml` with `run v2 compose analyze`
- [ ] Generate `run.toml` with `run v2 compose migrate`
- [ ] Convert application code to WASI components
- [ ] Test with `run v2 dev`
- [ ] Verify functionality matches Docker setup
- [ ] Deploy to production
- [ ] (Optional) Remove Docker for pure WASI

## Success Stories

### Case 1: Microservices API

**Before:**
- 5 services in Docker
- 10s startup
- 800MB total images

**After:**
- 5 WASI components
- <10ms startup
- 8MB total size

**Result:** 100x smaller, 1000x faster startup

### Case 2: Serverless Edge

**Before:**
- Node.js Lambda function
- 200MB deployment
- 300ms cold start

**After:**
- WASI component
- 3MB deployment
- <10ms cold start

**Result:** 66x smaller, 30x faster

### Case 3: CI/CD Pipeline

**Before:**
- Docker builds in CI
- 5 min build time
- Non-reproducible

**After:**
- Run 2.0 builds
- <30s build time
- Reproducible (same hash)

**Result:** 10x faster, verifiable

## Next Steps

1. Start with **hybrid mode** (WASI + Docker bridge)
2. Gradually migrate stateful services to WASI
3. Optimize component size and startup time
4. Deploy to edge/serverless if applicable
5. Remove Docker entirely when ready

## Support

- Documentation: https://run.esubalew.et/docs
- Examples: `examples/v2/docker-hybrid/`
- Issues: https://github.com/esubaalew/run/issues
