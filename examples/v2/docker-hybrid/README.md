# Docker Hybrid Example

Configuration template for migrating from Docker Compose to Run 2.0.

## Files

- `run.toml` - Hybrid configuration (WASI components + Docker services)
- `docker-compose.yml` - Original Docker setup for reference

## Concept

Run 2.0 can run WASI components alongside Docker containers:
- Application logic runs as fast WASI components
- Databases and caches stay in Docker (via bridge)

## Usage

1. Analyze your existing `docker-compose.yml`
2. Migrate stateless services to WASI components
3. Keep stateful services (postgres, redis) in Docker bridge
4. Run with `run v2 dev`

## Migration Command

```bash
run v2 compose analyze docker-compose.yml
run v2 compose migrate docker-compose.yml run.toml
```
