#!/bin/bash
# Interactive build script - select which features to include

set -e

echo "========================================"
echo "PGQT Custom Build"
echo "========================================"
echo ""
echo "Select which features to include:"
echo ""

# Default answers
INCLUDE_PLPGSQL="y"
INCLUDE_TLS="y"
INCLUDE_METRICS="y"
INCLUDE_SYSTEM_METRICS="y"
INCLUDE_WEB_CONFIG="y"

# Ask for each feature
read -p "Include plpgsql (stored procedures)? [Y/n]: " answer
if [[ "$answer" =~ ^[Nn]$ ]]; then
    INCLUDE_PLPGSQL="n"
fi

read -p "Include TLS (encryption)? [Y/n]: " answer
if [[ "$answer" =~ ^[Nn]$ ]]; then
    INCLUDE_TLS="n"
fi

read -p "Include metrics (Prometheus endpoint)? [Y/n]: " answer
if [[ "$answer" =~ ^[Nn]$ ]]; then
    INCLUDE_METRICS="n"
fi

if [ "$INCLUDE_METRICS" = "y" ]; then
    read -p "Include system-metrics (CPU/memory/disk)? [Y/n]: " answer
    if [[ "$answer" =~ ^[Nn]$ ]]; then
        INCLUDE_SYSTEM_METRICS="n"
    fi

    read -p "Include web-config (dashboard)? [Y/n]: " answer
    if [[ "$answer" =~ ^[Nn]$ ]]; then
        INCLUDE_WEB_CONFIG="n"
    fi
else
    INCLUDE_SYSTEM_METRICS="n"
    INCLUDE_WEB_CONFIG="n"
fi

echo ""
echo "========================================"
echo "Building with selected features:"
echo "========================================"
echo ""

# Build features list
FEATURES=""

if [ "$INCLUDE_PLPGSQL" = "y" ]; then
    FEATURES="$FEATURES plpgsql"
    echo "  ✓ plpgsql"
else
    echo "  ✗ plpgsql"
fi

if [ "$INCLUDE_TLS" = "y" ]; then
    FEATURES="$FEATURES tls"
    echo "  ✓ tls"
else
    echo "  ✗ tls"
fi

if [ "$INCLUDE_METRICS" = "y" ]; then
    FEATURES="$FEATURES metrics"
    echo "  ✓ metrics"
else
    echo "  ✗ metrics"
fi

if [ "$INCLUDE_SYSTEM_METRICS" = "y" ]; then
    FEATURES="$FEATURES system-metrics"
    echo "  ✓ system-metrics"
else
    echo "  ✗ system-metrics"
fi

if [ "$INCLUDE_WEB_CONFIG" = "y" ]; then
    FEATURES="$FEATURES web-config"
    echo "  ✓ web-config"
else
    echo "  ✗ web-config"
fi

echo ""

# Trim leading space
FEATURES=$(echo "$FEATURES" | sed 's/^ *//')

if [ -z "$FEATURES" ]; then
    echo "Building with NO features (minimal build)..."
    cargo build --release --no-default-features
else
    echo "Building with features: $FEATURES"
    cargo build --release --no-default-features --features "$FEATURES"
fi

echo ""
echo "========================================"
echo "Build complete!"
echo "========================================"
ls -lh target/release/pgqt
echo ""
echo "Binary size:"
du -h target/release/pgqt | cut -f1
echo ""
