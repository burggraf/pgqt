#!/bin/bash
# Build PGQT release binary WITHOUT TLS support (smaller binary)
# Output: target/release/pgqt (~9.5MB)
# Use this for local development when TLS is not needed

set -e

echo "Building PGQT release WITHOUT TLS support (smaller binary)..."
cargo build --release --no-default-features --features plpgsql

echo ""
echo "Build complete!"
ls -lh target/release/pgqt
echo ""
echo "Binary size:"
du -h target/release/pgqt | cut -f1
echo ""
echo "Note: TLS features are not available in this build."
echo "      Use ./build-release.sh if you need TLS/SSL support."
