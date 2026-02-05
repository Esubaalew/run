# Run 2.0 Benchmarks

Reproducible performance measurements.

## Machine Spec

Record your machine spec when running benchmarks:

```bash
./bench.sh | tee results-$(hostname)-$(date +%Y%m%d).csv
```

## Measurements

| Metric | Command | What it measures |
|--------|---------|------------------|
| Binary startup | `time run --version` | Cold CLI startup |
| Dev server ready | `run dev` (parse output) | Time to first component ready |
| Component load | From dev server output | Single component instantiation |
| Verify | `run verify` | Lockfile + hash verification |

## Running Benchmarks

```bash
cd bench/
./bench.sh
```

Output is CSV format for easy comparison.

## Baseline (Reference Machine)

Machine: macOS, Apple Silicon M1
Date: 2026-02-03

```
metric,value_ms,notes
binary_startup,4,run --version
dev_ready_1_component,10.5,run dev with hello-world
component_load,10.4,single WASM component
```

## Regression Guard

If any metric is > 2x baseline, investigate before release.
