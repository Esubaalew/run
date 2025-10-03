#!/usr/bin/env bash
# Expected output:
# step 1
# step 2
# step 3
set -euo pipefail

for step in 1 2 3; do
  echo "step $step"
done
