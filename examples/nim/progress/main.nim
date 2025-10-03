# Expected output:
# [1] bootstrapping runtime
# [2] compiling modules
# [3] serving traffic

let steps = @[
  "bootstrapping runtime",
  "compiling modules",
  "serving traffic"
]

for idx, step in steps.pairs():
  echo "[" & $(idx + 1) & "] " & step
