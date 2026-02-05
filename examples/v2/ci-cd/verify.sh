#!/bin/bash
set -e

echo "🔒 Verifying reproducible builds..."

# First build
echo "📦 Build 1..."
run build --reproducible
mkdir -p /tmp/run-build-1
cp target/wasm/*.wasm /tmp/run-build-1/

# Clean
echo "🧹 Cleaning..."
cargo clean

# Second build
echo "📦 Build 2..."
run build --reproducible
mkdir -p /tmp/run-build-2
cp target/wasm/*.wasm /tmp/run-build-2/

# Compare
echo "🔍 Comparing hashes..."
cd /tmp/run-build-1
sha256sum *.wasm > ../hashes1.txt

cd /tmp/run-build-2
sha256sum *.wasm > ../hashes2.txt

cd /tmp

if diff hashes1.txt hashes2.txt; then
    echo "✅ Builds are reproducible!"
    echo ""
    echo "Hashes:"
    cat hashes1.txt
    exit 0
else
    echo "❌ Builds are NOT reproducible!"
    echo ""
    echo "Build 1:"
    cat hashes1.txt
    echo ""
    echo "Build 2:"
    cat hashes2.txt
    exit 1
fi
