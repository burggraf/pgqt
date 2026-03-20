#!/bin/bash
# Build all variants of PGQT release binaries
# - pgqt (default: with TLS, ~12MB)
# - pgqt-small (without TLS, ~9.5MB)
# - pgqt-full (with TLS + Observability, ~14-15MB)

set -e

echo "=========================================="
echo "Building PGQT - All Variants"
echo "=========================================="
echo ""

# Build with ALL features (default)
echo "Building with ALL features (TLS + Observability, default)..."
cargo build --release
cp target/release/pgqt target/release/pgqt-full
echo "✓ Built: target/release/pgqt-tls"
ls -lh target/release/pgqt-tls | awk '{print "  Size: " $5}'
echo ""

# Build without TLS (small)
echo "Building WITHOUT TLS support (small)..."
cargo build --release --no-default-features --features plpgsql
cp target/release/pgqt target/release/pgqt-small
echo "✓ Built: target/release/pgqt-small"
ls -lh target/release/pgqt-small | awk '{print "  Size: " $5}'
echo ""

# Build with ALL features (TLS + Observability)
echo "Building with ALL features (TLS + Observability)..."
cargo build --release --features "plpgsql,tls,observability"
cp target/release/pgqt target/release/pgqt-full
echo "✓ Built: target/release/pgqt-full"
ls -lh target/release/pgqt-full | awk '{print "  Size: " $5}'
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
echo "target/release/pgqt-full  - With TLS + Observability"
echo ""
echo "Binary sizes:"
ls -lh target/release/pgqt* | grep -v ".d$" | awk '{printf "  %-25s %s\n", $9, $5}'
echo ""
echo "Size comparison:"
TLS_SIZE=$(stat -f%z target/release/pgqt-tls 2>/dev/null || stat -c%s target/release/pgqt-tls)
SMALL_SIZE=$(stat -f%z target/release/pgqt-small 2>/dev/null || stat -c%s target/release/pgqt-small)
FULL_SIZE=$(stat -f%z target/release/pgqt-full 2>/dev/null || stat -c%s target/release/pgqt-full)
DIFF_TLS=$((TLS_SIZE - SMALL_SIZE))
DIFF_FULL=$((FULL_SIZE - TLS_SIZE))
DIFF_TLS_MB=$(echo "scale=2; $DIFF_TLS / 1024 / 1024" | bc)
DIFF_FULL_MB=$(echo "scale=2; $DIFF_FULL / 1024 / 1024" | bc)
echo "  TLS adds ${DIFF_TLS_MB}MB to binary size"
echo "  Observability adds ${DIFF_FULL_MB}MB to binary size"
echo ""
echo "Usage examples:"
echo "  ./pgqt-small              # Minimal build, local dev"
echo "  ./pgqt-tls                # Production with TLS"
echo "  ./pgqt-full --metrics-enabled  # Production with TLS + Monitoring"
