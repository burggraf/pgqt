# PGQT Build Options

This document describes the different build configurations available for PGQT and how to use them.

## Overview

PGQT supports multiple build configurations based on feature flags:

| Build | Features | Size | Use Case |
|-------|----------|------|----------|
| **Small** | plpgsql only | ~9.5MB | Local development |
| **Default** | plpgsql + tls | ~12MB | Production (TLS) |
| **Full** | plpgsql + tls + observability | ~14-15MB | Production with monitoring |

## Build Scripts

We provide four convenience scripts in the project root:

### `build-release.sh` - Standard Release Build

Builds PGQT with full TLS support (default configuration).

```bash
./build-release.sh
```

**Output:** `target/release/pgqt` (~12MB)

**Use this when:**
- Deploying to production
- Remote access is needed
- TLS/SSL encryption is required
- Running in cloud environments

### `build-full.sh` - Full Build with Observability

Builds PGQT with **all features** including TLS and observability (metrics, system metrics, web dashboard).

```bash
./build-full.sh
```

**Output:** `target/release/pgqt` (~14-15MB)

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

Builds PGQT without TLS support for a smaller binary size.

```bash
./build-release-small.sh
```

**Output:** `target/release/pgqt` (~9.5MB)

**Use this when:**
- Local development only
- Binary size matters
- TLS is not needed (localhost-only access)
- Running in resource-constrained environments

### `build-both.sh` - Build All Variants

Builds all three variants with a size comparison.

```bash
./build-both.sh
```

**Outputs:**
- `target/release/pgqt` - Default build (with TLS, ~12MB)
- `target/release/pgqt-tls` - Explicitly with TLS (~12MB)
- `target/release/pgqt-small` - Without TLS (~9.5MB)
- `target/release/pgqt-full` - With TLS + Observability (~14-15MB)

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
| `metrics` | Prometheus metrics endpoint | | ~1.5-2MB |
| `system-metrics` | CPU, memory, disk monitoring | | ~300-500KB |
| `web-config` | Web dashboard at `/` | | ~0KB |
| `observability` | All metrics features combined | | ~2-2.5MB |

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
| Small | plpgsql | ~9.5MB | Baseline |
| Default | plpgsql + tls | ~12MB | +2.5MB (+26%) |
| Full | plpgsql + tls + observability | ~14-15MB | +4.5-5.5MB (+47-58%) |

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
