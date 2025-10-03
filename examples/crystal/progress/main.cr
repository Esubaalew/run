# Expected output:
# [1] parsing CLI
# [2] compiling shards
# [3] shipping binary

steps = [
  "parsing CLI",
  "compiling shards",
  "shipping binary"
]

steps.each_with_index do |step, index|
  puts "[#{index + 1}] #{step}"
end
