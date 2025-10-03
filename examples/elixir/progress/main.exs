# Expected output:
# [1] connecting to node
# [2] starting supervision tree
# [3] processing messages

steps = [
  "connecting to node",
  "starting supervision tree",
  "processing messages"
]

steps
|> Enum.with_index(1)
|> Enum.each(fn {label, index} ->
  IO.puts("[#{index}] #{label}")
end)
