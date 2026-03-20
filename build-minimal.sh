#!/bin/bash
# Build PGQT with MINIMAL features (core only, no plpgsql, no TLS, no observability)
# Output: target/release/pgqt (~8-9MB)
# Smallest possible binary

set -e

echo "Building PGQT with MINIMAL features (core only)..."
echo "Features EXCLUDED: plpgsql, tls, observability"
echo ""

cargo build --release --no-default-features

echo ""
echo "Build complete!"
ls -lh target/release/pgqt
echo ""
echo "Binary size:"
du -h target/release/pgqt | cut -f1
echo ""
echo "Features:"
echo "  ✗ plpgsql    - EXCLUDED"
echo "  ✗ tls        - EXCLUDED"
echo "  ✗ metrics    - EXCLUDED"
echo "  ✗ system-metrics - EXCLUDED"
echo "  ✗ web-config - EXCLUDED"
echo ""
echo "Use this build when:"
echo "  - You need the smallest possible binary"
echo "  - You don't need stored procedures"
echo "  - You don't need TLS/encryption"
echo "  - You don't need monitoring"
echo "  - Running purely locally with no external access"
