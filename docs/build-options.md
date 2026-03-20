# PGQT Build Options

This document describes the different build configurations available for PGQT and how to use them.

## Overview

PGQT supports multiple build configurations based on feature flags:

| Build | Features | Size | Use Case |
|-------|----------|------|----------|
| **Minimal** | Core only | ~8-9MB | Ultra-small, no extras |
| **Small** | plpgsql only | ~9-10MB | Local development |
| **Default** | plpgsql + tls + observability | ~12MB | Production with monitoring |
| **Custom** | You choose | Varies | Specific needs |

**Note:** As of v0.8.0, observability features are **enabled by default**. This includes Prometheus metrics, system monitoring, and web dashboard.

## Build Scripts

We provide six convenience scripts in the project root:

| Script | Features | Size | Use Case |
|--------|----------|------|----------|
| `build-release.sh` | **All (default)** | ~12MB | Production |
| `build-full.sh` | **All** | ~12MB | Production (explicit) |
| `build-release-small.sh` | plpgsql only | ~9-10MB | Dev (no TLS/metrics) |
| `build-minimal.sh` | Core only | ~8-9MB | Ultra-small |
| `build-custom.sh` | Interactive | Varies | Custom selection |
| `build-both.sh` | All variants | - | Build all versions |

### `build-release.sh` - Standard Release Build (Default)

Builds PGQT with **all features** including TLS and observability (metrics, system monitoring, web dashboard). This is the **default** build.

```bash
./build-release.sh
```

**Output:** `target/release/pgqt` (~12MB)

**Features:**
- ✓ plpgsql - PL/pgSQL stored procedure support
- ✓ tls - TLS/SSL encryption
- ✓ metrics - Prometheus metrics endpoint
- ✓ system-metrics - CPU, memory, disk monitoring
- ✓ web-config - Web dashboard at `/`

**Use this when:**
- Deploying to production
- Remote access is needed
- TLS/SSL encryption is required
- Monitoring and observability needed
- Running in cloud environments

**Runtime options:**
```bash
# Metrics enabled by default on port 9090
./target/release/pgqt --database app.db

# Disable metrics
./target/release/pgqt --database app.db --metrics-disabled

# Custom metrics port
./target/release/pgqt --database app.db --metrics-port 8080
```

### `build-full.sh` - Full Build with Observability

Builds PGQT with **all features** including TLS and observability (metrics, system metrics, web dashboard).

```bash
./build-full.sh
```

**Output:** `target/release/pgqt` (~12MB)

**Features included:**
- ✓ plpgsql - PL/pgSQL stored procedure support
- ✓ tls - TLS/SSL encryption
- ✓ metrics - Prometheus metrics endpoint
- ✓ system-metrics - CPU, memory, disk monitoring
- ✓ web-config - Web dashboard at `/`

**Use this when:**
- Production deployments requiring monitoring
- You need Prometheus metrics scraping
- You want the built-in web dashboard
- System resource monitoring is required

**Running with observability:**
```bash
./target/release/pgqt --metrics-enabled --metrics-port 9090
```

**Endpoints:**
- PostgreSQL: `localhost:5432`
- Metrics: `http://localhost:9090/metrics`
- Health: `http://localhost:9090/health`
- Dashboard: `http://localhost:9090/`

### `build-release-small.sh` - Small Release Build

Builds PGQT with **plpgsql only** (no TLS, no observability).

```bash
./build-release-small.sh
```

**Output:** `target/release/pgqt` (~9-10MB)

**Features:**
- ✓ plpgsql - PL/pgSQL stored procedure support
- ✗ tls - EXCLUDED (no encryption)
- ✗ metrics - EXCLUDED
- ✗ system-metrics - EXCLUDED
- ✗ web-config - EXCLUDED

**Use this when:**
- Local development only
- You need stored procedures but not TLS
- Binary size matters
- No monitoring needed

### `build-minimal.sh` - Minimal Build

Builds PGQT with **core only** (no plpgsql, no TLS, no observability). Smallest possible binary.

```bash
./build-minimal.sh
```

**Output:** `target/release/pgqt` (~8-9MB)

**Features:**
- ✗ plpgsql - EXCLUDED
- ✗ tls - EXCLUDED
- ✗ metrics - EXCLUDED
- ✗ system-metrics - EXCLUDED
- ✗ web-config - EXCLUDED

**Use this when:**
- You need the smallest possible binary
- You don't need stored procedures
- You don't need TLS/encryption
- You don't need monitoring
- Running purely locally with no external access

### `build-custom.sh` - Custom Build

Interactive script that lets you choose which features to include.

```bash
./build-custom.sh
```

**Example interaction:**
```
Include plpgsql (stored procedures)? [Y/n]: y
Include TLS (encryption)? [Y/n]: n
Include metrics (Prometheus endpoint)? [Y/n]: y
Include system-metrics (CPU/memory/disk)? [Y/n]: y
Include web-config (dashboard)? [Y/n]: n
```

**Use this when:**
- You need specific feature combinations
- You want to optimize binary size precisely
- You're experimenting with different configurations

### `build-both.sh` - Build All Variants

Builds all variants with a size comparison.

```bash
./build-both.sh
```

**Outputs:**
- `target/release/pgqt` - Default build (all features, ~12MB)
- `target/release/pgqt-full` - All features (~12MB)
- `target/release/pgqt-small` - plpgsql only (~9-10MB)
- `target/release/pgqt-tls` - Legacy name (same as full)

## Manual Build Commands

You can also build directly with cargo:

### With TLS (Default)
```bash
cargo build --release
```

### Without TLS
```bash
cargo build --release --no-default-features --features plpgsql
```

### With Observability (All Features)
```bash
cargo build --release --features "plpgsql,tls,observability"
```

Or build specific observability features:
```bash
# Metrics only (no system metrics or web UI)
cargo build --release --features "plpgsql,tls,metrics"

# Metrics + System metrics (no web UI)
cargo build --release --features "plpgsql,tls,system-metrics"

# Metrics + Web UI (no system metrics)
cargo build --release --features "plpgsql,tls,web-config"
```

### Feature Flags

| Feature | Description | Default | Size Impact |
|---------|-------------|---------|-------------|
| `tls` | TLS/SSL support via rustls | ✓ | ~2.5MB |
| `plpgsql` | PL/pgSQL stored procedure support | ✓ | ~500KB |
| `metrics` | Prometheus metrics endpoint | ✓ | ~1.5-2MB |
| `system-metrics` | CPU, memory, disk monitoring | ✓ | ~300-500KB |
| `web-config` | Web dashboard at `/` | ✓ | ~0KB |
| `observability` | All metrics features combined | ✓ | ~2-2.5MB |

**Note:** All features are enabled by default as of v0.8.0. Use `--no-default-features` to exclude them.

### Custom Feature Combinations

```bash
# TLS only, no PL/pgSQL
cargo build --release --no-default-features --features tls

# PL/pgSQL only, no TLS
cargo build --release --no-default-features --features plpgsql

# Minimal build (no optional features)
cargo build --release --no-default-features
```

## TLS Support Details

### What TLS Provides

When the `tls` feature is enabled, PGQT can:

- Accept encrypted connections from PostgreSQL clients
- Generate self-signed certificates automatically (`--ssl-ephemeral`)
- Use custom TLS certificates (`--ssl-cert`, `--ssl-key`)
- Verify client certificates (`--ssl-ca`)

### TLS Dependencies

The `tls` feature adds these dependencies:

| Crate | Purpose | Size Impact |
|-------|---------|-------------|
| `rustls` | TLS implementation | ~1.5MB |
| `tokio-rustls` | Async TLS integration | ~300KB |
| `rcgen` | Certificate generation | ~200KB |
| **Total** | | **~2MB** |

### Without TLS

If you try to use TLS flags when running a binary built without TLS support:

```bash
./pgqt --ssl --ssl-ephemeral
```

You'll see:
```
Warning: TLS requested but not compiled in. Rebuild with --features tls
```

The server will start but without TLS encryption.

## Size Comparison

| Build | Features | Size | Difference |
|-------|----------|------|------------|
| Minimal | Core only | ~8-9MB | Baseline |
| Small | plpgsql | ~9-10MB | +1MB (+11%) |
| Default | plpgsql + tls + observability | ~12MB | +3-4MB (+33-44%) |

## When to Use Each Build

### Use Full Build (with TLS) When:

- **Production deployments** - Security is critical
- **Remote access** - Clients connect over network
- **Cloud hosting** - AWS, GCP, Azure, etc.
- **Docker containers** - Running in containerized environments
- **CI/CD pipelines** - Testing against production-like configs
- **Shared development servers** - Multiple developers connect remotely

### Use Full Build with Observability When:

- **Production monitoring required** - You need Prometheus metrics
- **Performance analysis** - Query latency tracking and profiling
- **Resource monitoring** - CPU, memory, disk usage tracking
- **Operational visibility** - Health checks and dashboards
- **Long-running services** - Services that need continuous monitoring

### Use Small Build (without TLS) When:

- **Local development** - Only localhost connections
- **Embedded systems** - Limited storage space
- **Quick testing** - Faster build times
- **CI for non-TLS tests** - When testing non-network features
- **Distribution size matters** - Smaller downloads

## Examples

### Local Development Workflow

```bash
# Build small for daily development
./build-release-small.sh

# Run locally (no TLS needed)
./target/release/pgqt --database dev.db
```

### Production Deployment

```bash
# Build full version for production
./build-release.sh

# Deploy with TLS
./target/release/pgqt --database prod.db --ssl --ssl-ephemeral
```

### Building Both for Distribution

```bash
# Create both variants
./build-both.sh

# Distribute appropriate version
# - Developers get pgqt-small
# - Servers get pgqt-tls
# - Monitored servers get pgqt-full
```

### Production Deployment with Observability

```bash
# Build with observability
./build-full.sh

# Deploy with TLS and metrics
./target/release/pgqt \
  --database prod.db \
  --ssl --ssl-ephemeral \
  --metrics-enabled \
  --metrics-port 9090

# Access endpoints:
# - PostgreSQL: localhost:5432
# - Metrics:    http://localhost:9090/metrics
# - Health:     http://localhost:9090/health
# - Dashboard:  http://localhost:9090/
```

## Troubleshooting

### "TLS requested but not compiled in"

You're running a binary built without TLS support but using TLS flags:

```bash
# Wrong - built without TLS, running with TLS flags
./build-release-small.sh
./target/release/pgqt --ssl
# Warning: TLS requested but not compiled in

# Correct - use full build
./build-release.sh
./target/release/pgqt --ssl --ssl-ephemeral
```

### Binary Still Large Without TLS

If your binary is still large after building without TLS, check:

1. **Clean build:** `cargo clean` then rebuild
2. **Profile settings:** Ensure `[profile.release]` has:
   ```toml
   strip = true
   lto = true
   codegen-units = 1
   panic = "abort"
   ```
3. **Dependencies:** Some dependencies (like `mlua`, `pg_query`) are inherently large

### Feature Not Found

If you get errors about missing features:

```bash
# Check available features
cargo metadata --format-version 1 | jq '.packages[] | select(.name == "pgqt") | .features'

# Verify Cargo.toml has the features defined
```
