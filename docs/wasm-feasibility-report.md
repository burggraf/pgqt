# PGQT WASM Target Feasibility Report

## Executive Summary

**PGQT cannot run on `wasm32-unknown-unknown` without major re-architecture.**

This report analyzes the feasibility of adding WASM compilation targets to PGQT, identifying blockers and proposing alternative approaches.

---

## Dependency WASM Compatibility Analysis

| Crate | Version | WASM Status | Key Blocker |
|-------|---------|-------------|-------------|
| `tokio` | 1.0 (full features) | ❌ No | No TCP/networking/threads in wasm32-unknown-unknown |
| `pgwire` | 0.38 | ⚠️ Partial | Protocol library works; TCP server capability doesn't |
| `rusqlite` | 0.38 (bundled) | ✅ Yes | Works in-memory only (no filesystem access) |
| `pg_query` | 6.1 | ❌ No | `libpg_query` is C code, requires libc |
| `mlua` | 0.10 (luau) | ❌ No | C++ library, needs setjmp/longjmp |
| `dashmap` | 6.1.0 | ✅ Yes | None (minor: panics on deadlock) |
| `tokio-rustls` | 0.26 | ❌ No | No raw TCP sockets in WASM |
| `rcgen` | 0.13 | ✅ Yes | Needs JS RNG + time glue for wasm32-unknown-unknown |
| `serde` | 1.0 | ✅ Yes | Pure Rust, WASM-safe |
| `regex` | 1.10 | ✅ Yes | Pure Rust, WASM-safe |
| `chrono` | 0.4 | ✅ Yes | Pure Rust, WASM-safe |
| `uuid` | 1.7 | ✅ Yes | Pure Rust, WASM-safe |
| `anyhow` | 1.0 | ✅ Yes | Pure Rust, WASM-safe |
| `bytes` | 1.11 | ✅ Yes | Pure Rust, WASM-safe |
| `async-trait` | 0.1 | ✅ Yes | Pure Rust, WASM-safe |
| `futures` | 0.3 | ✅ Yes | Pure Rust, WASM-safe |
| `lazy_static` | 1.4 | ✅ Yes | Pure Rust, WASM-safe |
| `lru` | 0.12 | ✅ Yes | Pure Rust, WASM-safe |
| `hex` | 0.4 | ✅ Yes | Pure Rust, WASM-safe |
| `md5` | 0.7 | ✅ Yes | Pure Rust, WASM-safe |
| `encoding_rs` | 0.8 | ✅ Yes | Pure Rust, WASM-safe |
| `libc` | 0.2 | ❌ No | System C library bindings |

---

## Hard Blockers (Require Major Work)

### 1. pg_query - PostgreSQL Parser

**Current Implementation**: PGQT uses `pg_query` (v6.1) for parsing PostgreSQL SQL into AST.

**Problem**: 
- `pg_query` is a Rust wrapper around `libpg_query`
- `libpg_query` is extracted PostgreSQL C source code
- Requires libc and cannot compile to wasm32-unknown-unknown

**Potential Solutions**:

| Solution | Effort | Trade-offs |
|----------|--------|------------|
| Switch to `postgresql-cst-parser` | ~2-3 weeks | Pure Rust, PG 17 grammar; requires full transpiler AST refactor |
| Use `sqlparser` crate | ~3-4 weeks | Generic SQL parser; would lose PostgreSQL-specific syntax support |
| Emscripten build of `libpg_query` | ~1-2 weeks | Complex build process, large WASM binary (~5MB+), maintenance burden |
| Custom parser | ~2-3 months | Full control, but massive undertaking |

**Recommended**: `postgresql-cst-parser` if maintaining PostgreSQL compatibility is critical.

---

### 2. mlua/luau - PL/pgSQL Runtime

**Current Implementation**: PGQT uses `mlua` with Luau backend for executing PL/pgSQL functions transpiled to Lua.

**Problem**:
- Luau is a C++ library
- Requires `setjmp`/`longjmp` for error handling
- Cannot compile to wasm32-unknown-unknown without Emscripten

**Potential Solutions**:

| Solution | Effort | Trade-offs |
|----------|--------|------------|
| Switch to `rhai` | ~1-2 weeks | Pure Rust scripting; requires full PL/pgSQL runtime rewrite |
| Switch to `rune` | ~1-2 weeks | Pure Rust scripting; different syntax, rewrite needed |
| Use `wasm-bindgen` with JS Lua | ~1 week | Depends on JavaScript environment |
| Feature-gate PL/pgSQL | ~2-3 days | Disable for WASM builds; lose stored procedure support |

**Recommended**: Feature-gate PL/pgSQL for WASM builds as the immediate solution, with `rhai` as a long-term replacement.

---

### 3. tokio + pgwire - TCP Server Architecture

**Current Implementation**: PGQT is fundamentally a TCP server that accepts PostgreSQL wire protocol connections.

**Problem**:
- `tokio` with "full" features includes networking not available in WASM
- `pgwire` server implementation requires raw TCP sockets
- `wasm32-unknown-unknown` has no socket support

**Potential Solutions**:

| Solution | Effort | Trade-offs |
|----------|--------|------------|
| Target `wasm32-wasip2` | ~1 week | WASI Preview 2 has socket support; limited runtime support (Wasmtime) |
| Re-architect as library | ~3-4 weeks | Create "pgqt-core" without server; use WebSocket/JS glue |
| Use `wasm-bindgen` + WebSockets | ~2-3 weeks | Browser-compatible; requires JS shim for PostgreSQL protocol |

**Recommended**: `wasm32-wasip2` for server-side WASM, or library re-architecture for browser use.

---

## Recommended Approaches

### Option A: wasm32-wasip2 Target (Recommended for Server-Side)

**Use Case**: Running PGQT in server-side WASM runtimes like Wasmtime or WasmEdge.

```bash
rustup target add wasm32-wasip2
cargo build --target wasm32-wasip2
```

**Pros**:
- Minimal re-architecture needed
- Socket support via WASI Preview 2
- Can still act as a TCP server

**Cons**:
- Still requires solving `pg_query` and `mlua` blockers
- Limited runtime support (no browsers, few cloud platforms)
- WASI Preview 2 still evolving

**Effort**: ~3-4 weeks (mostly parser + runtime replacement)

---

### Option B: pgqt-core Library Crate (Recommended for Browser)

**Use Case**: Browser-based SQL transpilation, demo tools, or embedding in other applications.

**Architecture**:
```
pgqt-core/          # New crate: transpiler + SQLite only
├── src/
│   ├── lib.rs      # Library exports
│   ├── transpiler/ # Existing transpiler modules
│   ├── catalog/    # Catalog management
│   └── ...

pgqt-server/        # Existing binary crate
├── src/
│   ├── main.rs     # TCP server, tokio, pgwire
│   └── handler/    # Connection handling
```

**Changes Needed**:
1. Extract transpiler and catalog into `pgqt-core`
2. Remove tokio/pgwire dependencies from core
3. Add `wasm-bindgen` bindings
4. Create simple API: `transpile(sql) -> Result<String>`

**Pros**:
- Clean separation of concerns
- Browser-compatible
- Can still use in native applications

**Cons**:
- Significant refactoring
- No PostgreSQL wire protocol in WASM (use HTTP/WebSocket instead)
- Still requires parser replacement

**Effort**: ~4-6 weeks

---

### Option C: Full wasm32-unknown-unknown Port

**Use Case**: Maximum portability, including browsers and edge computing.

**Requirements**:
1. Replace `pg_query` with `postgresql-cst-parser`
2. Replace `mlua` with `rhai` or feature-gate it
3. Re-architect as library (Option B)
4. Add WebSocket or HTTP transport layer
5. Handle SQLite in-memory only (or use IndexedDB via `sql.js` approach)

**Pros**:
- Runs everywhere
- Maximum portability

**Cons**:
- Massive effort
- Lost features (PL/pgSQL, file-based databases)
- Complex build and testing

**Effort**: ~6-8 weeks

---

## Implementation Roadmap

### Phase 1: Core Extraction (2-3 weeks)
- Create `pgqt-core` crate
- Move transpiler, catalog, and SQLite handling
- Feature-gate server-specific code

### Phase 2: Parser Migration (2-3 weeks)
- Evaluate `postgresql-cst-parser` vs alternatives
- Migrate transpiler to new AST
- Comprehensive test suite

### Phase 3: Runtime Replacement (1-2 weeks)
- Feature-gate PL/pgSQL
- Evaluate `rhai` for future use
- Document limitations

### Phase 4: WASM Bindings (1 week)
- Add `wasm-bindgen` to `pgqt-core`
- Create JavaScript/TypeScript wrapper
- Build and publish to npm

---

## Quick Wins (Minimal Effort)

If full WASM support isn't required, these are easier alternatives:

1. **Feature-gate problematic modules**:
   ```toml
   [features]
   default = ["plpgsql", "server"]
   wasm = []  # Minimal feature set
   ```

2. **Use sql.js for browser demos**:
   - PostgreSQL syntax transpiled server-side
   - SQLite execution via sql.js in browser

3. **Docker WASM (wasmedge)**:
   - Use `wasm32-wasi` target
   - Run in containerized WASM runtime

---

## Conclusion

Adding WASM support to PGQT is **very difficult** due to three fundamental blockers:

1. **pg_query** (C-based PostgreSQL parser)
2. **mlua/luau** (C++ scripting runtime)  
3. **tokio/pgwire** (TCP server architecture)

The most practical paths are:
- **For server-side**: Target `wasm32-wasip2` (~3-4 weeks)
- **For browser/library use**: Create `pgqt-core` crate (~4-6 weeks)

Both approaches require replacing the parser and feature-gating or replacing the PL/pgSQL runtime.
