# Developer Experience Example

Configuration template for `run v2 dev` workflow.

## Files

- `run.toml` - Development configuration with hot reload
- `wit/` - Interface definitions for api, web, worker components

## Features

The `run v2 dev` command provides:
- Hot reload on file changes
- Unified logs from all components
- Automatic restart on crash
- Multi-component orchestration

## Configuration

```toml
[dev]
watch = ["src/**/*.rs"]
reload_delay_ms = 100
auto_restart = true
log_level = "info"
```

## Usage

1. Build your components
2. Update paths in `run.toml`
3. Run `run v2 dev`

## Commands

```bash
run v2 dev              # Start with hot reload
run v2 dev --verbose    # Detailed output
run v2 dev --filter api # Filter logs
```
