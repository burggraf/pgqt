#!/bin/bash
# Build PGQT release binary with ALL features (TLS + Observability)
# Output: target/release/pgqt (~14-15MB)
# This includes: plpgsql, tls, metrics, system-metrics, web-config

set -e

echo "Building PGQT release with ALL features..."
echo "Features included: plpgsql, tls, metrics, system-metrics, web-config"
echo ""

cargo build --release --features "plpgsql,tls,observability"

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
echo "  ./pgqt --metrics-enabled --metrics-port 9090"
echo ""
echo "Endpoints:"
echo "  PostgreSQL: localhost:5432"
echo "  Metrics:    http://localhost:9090/metrics"
echo "  Health:     http://localhost:9090/health"
echo "  Dashboard:  http://localhost:9090/"
