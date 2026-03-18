#!/bin/bash
# Build both variants of PGQT release binaries
# - pgqt (with TLS, ~12MB)
# - pgqt-small (without TLS, ~9.5MB)

set -e

echo "=========================================="
echo "Building PGQT - Both Variants"
echo "=========================================="
echo ""

# Build with TLS
echo "Building with TLS support..."
cargo build --release
cp target/release/pgqt target/release/pgqt-tls
echo "✓ Built: target/release/pgqt-tls"
ls -lh target/release/pgqt-tls | awk '{print "  Size: " $5}'
echo ""

# Build without TLS
echo "Building WITHOUT TLS support (small)..."
cargo build --release --no-default-features --features plpgsql
cp target/release/pgqt target/release/pgqt-small
echo "✓ Built: target/release/pgqt-small"
ls -lh target/release/pgqt-small | awk '{print "  Size: " $5}'
echo ""

# Restore default build
echo "Restoring default build (with TLS)..."
cargo build --release
echo "✓ Default binary: target/release/pgqt (with TLS)"
echo ""

echo "=========================================="
echo "Build Summary"
echo "=========================================="
echo ""
echo "target/release/pgqt       - Default build (with TLS)"
echo "target/release/pgqt-tls   - With TLS support"
echo "target/release/pgqt-small - Without TLS (smaller)"
echo ""
echo "Binary sizes:"
ls -lh target/release/pgqt* | grep -v ".d$" | awk '{printf "  %-25s %s\n", $9, $5}'
echo ""
echo "Size difference:"
TLS_SIZE=$(stat -f%z target/release/pgqt-tls 2>/dev/null || stat -c%s target/release/pgqt-tls)
SMALL_SIZE=$(stat -f%z target/release/pgqt-small 2>/dev/null || stat -c%s target/release/pgqt-small)
DIFF=$((TLS_SIZE - SMALL_SIZE))
DIFF_MB=$(echo "scale=2; $DIFF / 1024 / 1024" | bc)
echo "  TLS adds ${DIFF_MB}MB to binary size"
