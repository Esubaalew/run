# CI/CD Reproducibility Example

Configuration template for hermetic, reproducible builds.

## Files

- `run.toml` - Build configuration with reproducibility settings
- `wit/hello.wit` - Simple interface definition
- `verify.sh` - Script to verify build reproducibility

## Concept

Run 2.0 ensures reproducible builds:
- Toolchain versions locked in `run.lock.toml`
- Deterministic environment (SOURCE_DATE_EPOCH=0)
- Same hash on every machine

## Usage

```bash
# Build with reproducibility
run v2 build --reproducible

# Verify (build twice, compare hashes)
./verify.sh
```

## CI Integration

```yaml
# GitHub Actions
- name: Reproducible build
  run: run v2 build --reproducible

- name: Verify hash
  run: sha256sum target/wasm/*.wasm
```
