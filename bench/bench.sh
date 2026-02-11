#!/bin/bash
# Run 2.0 Benchmark Script
# Outputs CSV format for reproducibility

set -e

RUN_BIN="${RUN_BIN:-../target/release/run}"
EXAMPLE_DIR="${EXAMPLE_DIR:-../examples/hello-world}"

echo "# Run 2.0 Benchmarks"
echo "# Machine: $(uname -m)"
echo "# OS: $(uname -s)"
echo "# Date: $(date -Iseconds)"
echo ""
echo "metric,value_ms,notes"

# Reset persistent perf counters for a clean benchmark sample
$RUN_BIN --perf-reset > /dev/null 2>&1 || true

# Binary startup
start=$(python3 -c 'import time; print(int(time.time() * 1000))')
$RUN_BIN --version > /dev/null
end=$(python3 -c 'import time; print(int(time.time() * 1000))')
echo "binary_startup,$((end - start)),run --version"

# Dev server startup (measure from output)
cd "$EXAMPLE_DIR"
timeout 5s $RUN_BIN dev 2>&1 | grep -o 'started ([0-9.]*ms)' | head -1 | sed 's/started (\([0-9.]*\)ms)/component_load,\1,single component/' || true

echo ""
echo "# Perf counters"
$RUN_BIN --perf-report || true
echo ""
echo "# End of benchmarks"
