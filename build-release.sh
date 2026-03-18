#!/bin/bash
# Build PGQT release binary with TLS support (default)
# Output: target/release/pgqt (~12MB)

set -e

echo "Building PGQT release with TLS support..."
cargo build --release

echo ""
echo "Build complete!"
ls -lh target/release/pgqt
echo ""
echo "Binary size:"
du -h target/release/pgqt | cut -f1
