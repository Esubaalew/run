-- Expected output:
-- Step 1: Parsing configuration
-- Step 2: Loading modules
-- Step 3: Launching runtime

module Main where

import Text.Printf (printf)

steps :: [(Int, String)]
steps = zip [1 ..] ["Parsing configuration", "Loading modules", "Launching runtime"]

main :: IO ()
main = mapM_ printStep steps
  where
    printStep (index, label) = printf "Step %d: %s\n" index label
