#!/bin/bash
# Build PGQT without TLS and Observability (smaller binary)
# Output: target/release/pgqt (~9-10MB)
# Includes: plpgsql
# Excludes: tls, observability (metrics, system-metrics, web-config)

set -e

echo "Building PGQT without TLS and Observability..."
echo "Features: plpgsql only"
echo ""

cargo build --release --no-default-features --features plpgsql

echo ""
echo "Build complete!"
ls -lh target/release/pgqt
echo ""
echo "Binary size:"
du -h target/release/pgqt | cut -f1
echo ""
echo "Features:"
echo "  ✓ plpgsql    - PL/pgSQL stored procedure support"
echo "  ✗ tls        - EXCLUDED (no encryption)"
echo "  ✗ metrics    - EXCLUDED"
echo "  ✗ system-metrics - EXCLUDED"
echo "  ✗ web-config - EXCLUDED"
echo ""
echo "Use this build when:"
echo "  - Local development only"
echo "  - You need stored procedures but not TLS"
echo "  - Binary size matters"
echo "  - No monitoring needed"
echo ""
echo "Note: To add features back, use:"
echo "      ./build-release.sh (all features)"
