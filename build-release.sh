#!/bin/bash
# Build PGQT release binary with ALL features (default)
# Output: target/release/pgqt (~14-15MB)

set -e

echo "Building PGQT release with ALL features (TLS + Observability)..."
cargo build --release

echo ""
echo "Build complete!"
ls -lh target/release/pgqt
echo ""
echo "Binary size:"
du -h target/release/pgqt | cut -f1
echo ""
echo "Features enabled:"
echo "  ✓ plpgsql    - PL/pgSQL stored procedure support"
echo "  ✓ tls        - TLS/SSL encryption"
echo "  ✓ metrics    - Prometheus metrics endpoint (/metrics)"
echo "  ✓ system-metrics - CPU, memory, disk monitoring"
echo "  ✓ web-config - Web dashboard at /"
echo ""
echo "Usage:"
echo "  ./pgqt                    # Metrics enabled by default on port 9090"
echo "  ./pgqt --metrics-disabled # Disable metrics"
echo "  ./pgqt --metrics-port 8080 # Use custom port"
echo ""
echo "Endpoints:"
echo "  PostgreSQL: localhost:5432"
echo "  Metrics:    http://localhost:9090/metrics"
echo "  Health:     http://localhost:9090/health"
echo "  Dashboard:  http://localhost:9090/"
