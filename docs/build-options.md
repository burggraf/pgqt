# PGQT Build Options

This document describes the different build configurations available for PGQT and how to use them.

## Overview

PGQT supports two main build configurations:

1. **Full Build (with TLS)** - Default, includes TLS/SSL support (~12MB)
2. **Small Build (without TLS)** - Smaller binary for local development (~9.5MB)

## Build Scripts

We provide three convenience scripts in the project root:

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

### `build-both.sh` - Build Both Variants

Builds both variants and creates three binaries with a size comparison.

```bash
./build-both.sh
```

**Outputs:**
- `target/release/pgqt` - Default build (with TLS, ~12MB)
- `target/release/pgqt-tls` - Explicitly with TLS (~12MB)
- `target/release/pgqt-small` - Without TLS (~9.5MB)

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

### Feature Flags

| Feature | Description | Default |
|---------|-------------|---------|
| `tls` | TLS/SSL support via rustls | ✓ |
| `plpgsql` | PL/pgSQL stored procedure support | ✓ |

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

| Build | Size | Difference |
|-------|------|------------|
| Without TLS | ~9.5MB | Baseline |
| With TLS | ~12MB | +2.5MB (+26%) |

## When to Use Each Build

### Use Full Build (with TLS) When:

- **Production deployments** - Security is critical
- **Remote access** - Clients connect over network
- **Cloud hosting** - AWS, GCP, Azure, etc.
- **Docker containers** - Running in containerized environments
- **CI/CD pipelines** - Testing against production-like configs
- **Shared development servers** - Multiple developers connect remotely

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
