# Modular Binary Distribution for PGQT: Feature Flags vs Runtime Plugins

## Executive Summary

**Yes, it's possible** to provide optional features as downloadable components in Rust, but the approach depends heavily on your priorities:

| Approach | User Experience | Binary Size | Complexity | Best For |
|----------|----------------|-------------|------------|----------|
| **Feature Flags (current)** | Download specific binary | Smallest total | Low | Most users |
| **Native Plugins (dylib)** | Download `.so`/`.dll` files | Medium total | High | Advanced users |
| **WebAssembly Plugins** | Download `.wasm` files | Large host, small plugins | Medium | Sandboxed extensions |
| **Sidecar Binaries** | Download separate executables | Medium | Low | CLI tools, converters |

**Recommendation:** For PGQT, **feature flags with multiple pre-built binaries** is the most practical approach. Runtime plugins add significant complexity for limited benefit in a database proxy context.

---

## 1. Approach 1: Feature Flags with Multiple Binaries (Recommended)

### How It Works
Build multiple variants of PGQT with different feature combinations, users download the one they need:

```bash
# pgqt-minimal (no optional features)
pgqt-v0.8.0-minimal-linux-x64.tar.gz      # ~8 MB

# pgqt-standard (default features)
pgqt-v0.8.0-standard-linux-x64.tar.gz     # ~10 MB

# pgqt-full (all features)
pgqt-v0.8.0-full-linux-x64.tar.gz         # ~15 MB
```

### Implementation

**Cargo.toml:**
```toml
[features]
default = ["tls", "plpgsql"]

# Core features
minimal = []  # No optional features
standard = ["tls", "plpgsql", "metrics", "web-config"]
full = ["tls", "plpgsql", "metrics", "web-config", "system-metrics", "tracing"]

# Individual features
tls = ["rustls", "tokio-rustls", "rcgen"]
plpgsql = ["pg_parse"]
metrics = ["prometheus-client", "tiny_http"]
web-config = ["metrics"]
system-metrics = ["sysinfo"]
tracing = ["tracing-subscriber"]
```

**Build script:**
```bash
#!/bin/bash
# build-variants.sh

VERSION=$(grep '^version' Cargo.toml | head -1 | cut -d'"' -f2)
TARGETS=("x86_64-unknown-linux-gnu" "aarch64-unknown-linux-gnu" "x86_64-apple-darwin")

for target in "${TARGETS[@]}"; do
    # Minimal
    cargo build --release --target $target --no-default-features
    tar czf "pgqt-v${VERSION}-minimal-${target}.tar.gz" -C target/$target/release pgqt
    
    # Standard
    cargo build --release --target $target --features standard
    tar czf "pgqt-v${VERSION}-standard-${target}.tar.gz" -C target/$target/release pgqt
    
    # Full
    cargo build --release --target $target --features full
    tar czf "pgqt-v${VERSION}-full-${target}.tar.gz" -C target/$target/release pgqt
done
```

### Distribution via Install Script

```bash
# install.sh - One-liner installation
curl -sSL https://pgqt.dev/install.sh | bash -s -- --variant standard

# Or manual selection
curl -sSL https://pgqt.dev/install.sh | bash -s -- --variant minimal --target aarch64-unknown-linux-gnu
```

**install.sh excerpt:**
```bash
#!/bin/bash
VARIANT=${1:-standard}
TARGET=${2:-$(detect_target)}
VERSION=$(curl -s https://api.github.com/repos/burggraf/pgqt/releases/latest | grep tag_name | cut -d'"' -f4)

URL="https://github.com/burggraf/pgqt/releases/download/${VERSION}/pgqt-${VERSION}-${VARIANT}-${TARGET}.tar.gz"

curl -L "$URL" | tar xz -C /usr/local/bin
chmod +x /usr/local/bin/pgqt
```

### Pros & Cons

| Pros | Cons |
|------|------|
| ✅ Zero runtime complexity | ❌ Users must choose at download time |
| ✅ Smallest possible binaries | ❌ Multiple builds in CI |
| ✅ No ABI compatibility issues | ❌ Can't add features without re-download |
| ✅ Full compiler optimizations (LTO) | |
| ✅ Simple distribution | |

---

## 2. Approach 2: Native Dynamic Libraries (Plugins)

### How It Works
Compile features as `.so`/`.dll`/`.dylib` files that are loaded at runtime:

```
pgqt-core (8 MB)
├── plugins/
│   ├── libpgqt_tls.so (2 MB)
│   ├── libpgqt_metrics.so (1.5 MB)
│   ├── libpgqt_plpgsql.so (500 KB)
│   └── libpgqt_webconfig.so (shared with metrics)
```

### Implementation with `abi_stable`

**Plugin Interface (shared between host and plugins):**

```rust
// pgqt-plugin-interface/src/lib.rs
use abi_stable::std_types::{RString, RVec, RBox};
use abi_stable::{StableAbi, sabi_trait};

#[sabi_trait]
pub trait PgqtFeature: Send + Sync {
    fn name(&self) -> RString;
    fn version(&self) -> RString;
    fn initialize(&self, context: &FeatureContext) -> RBox<dyn FeatureInstance>;
}

#[repr(C)]
#[derive(StableAbi)]
pub struct FeatureContext {
    pub config_path: RString,
    pub data_dir: RString,
}
```

**Host (plugin loader):**

```rust
// src/plugin_loader.rs
use abi_stable::library::RootModule;
use abi_stable::sabi_types::VersionStrings;

pub struct PluginLoader {
    plugins: Vec<Box<dyn PgqtFeature>>,
}

impl PluginLoader {
    pub fn load_plugin(&mut self, path: &Path) -> Result<()> {
        type PluginRef = abi_stable::library::lib_header::LibraryHeader;
        
        let library = unsafe { libloading::Library::new(path)? };
        let manifest: libloading::Symbol<unsafe extern "C" fn() -> *const PluginManifest> = 
            library.get(b"plugin_manifest")?;
        
        let manifest = unsafe { &*manifest() };
        
        if manifest.api_version != CURRENT_API_VERSION {
            return Err(anyhow!("Plugin API version mismatch"));
        }
        
        // Load and initialize
        let initializer: libloading::Symbol<unsafe extern "C" fn(&FeatureContext) -> *mut dyn PgqtFeature> = 
            library.get(b"plugin_init")?;
        
        let feature = unsafe { Box::from_raw(initializer(&self.context)) };
        self.plugins.push(feature);
        
        // Keep library loaded (leak intentionally or use Arc)
        std::mem::forget(library);
        
        Ok(())
    }
}
```

**Plugin implementation (example: metrics):**

```rust
// plugins/metrics/src/lib.rs
use pgqt_plugin_interface::{PgqtFeature, FeatureContext, FeatureInstance};
use abi_stable::{export_root_module, prefix_type::PrefixTypeTrait};

#[export_root_module]
static PLUGIN_HEADER: PluginHeader = PluginHeader::new(
    PluginVersion {
        major: 0,
        minor: 1,
        patch: 0,
    },
    PluginManifest {
        name: "metrics",
        description: "Prometheus metrics and web config",
    },
);

#[sabi_extern_fn]
pub fn plugin_init(context: &FeatureContext) -> RBox<dyn FeatureInstance> {
    let metrics = MetricsFeature::new(context);
    RBox::new(metrics)
}

struct MetricsFeature {
    server: Option<MetricsServer>,
}

impl FeatureInstance for MetricsFeature {
    fn start(&mut self) -> Result<()> {
        self.server = Some(MetricsServer::new()?);
        Ok(())
    }
    
    fn shutdown(&mut self) {
        self.server = None;
    }
}
```

### Critical Challenges

1. **Memory Allocation Mismatch**
   ```rust
   // WRONG: Allocating in plugin, freeing in host
   let data = plugin_alloc_string();  // Uses plugin's allocator
   drop(data);  // Uses host's allocator - CRASH!
   
   // CORRECT: Use shared allocator or provide destructor
   let data = plugin_alloc_string();
   plugin_free_string(data);  // Same allocator
   ```

2. **Panic Safety**
   ```rust
   #[no_mangle]
   pub extern "C" fn safe_plugin_call() -> i32 {
       match std::panic::catch_unwind(|| {
           // plugin logic
       }) {
           Ok(result) => result,
           Err(_) => -1,  // Panic caught, don't crash host
       }
   }
   ```

3. **Dependency Duplication**
   - Each plugin statically links its own copy of `std`, `serde`, etc.
   - 3 plugins × 2 MB each = 6 MB of duplicated code
   - Solution: Use `dlopen` with `RTLD_GLOBAL` (risky) or accept duplication

### Pros & Cons

| Pros | Cons |
|------|------|
| ✅ Download features after install | ❌ Complex FFI safety requirements |
| ✅ Core binary stays small | ❌ Dependency duplication across plugins |
| ✅ Can update plugins independently | ❌ Rust ABI instability (requires `abi_stable`) |
| | ❌ Platform-specific builds (.so/.dll/.dylib) |
| | ❌ Debugging is difficult across FFI boundary |

---

## 3. Approach 3: WebAssembly Plugins

### How It Works
Features compiled to `.wasm` modules, loaded by a Wasm runtime (Wasmtime/Wasmer):

```
pgqt-core (8 MB + 5-10 MB Wasmtime runtime)
├── wasm-plugins/
│   ├── tls.wasm (500 KB)
│   ├── metrics.wasm (400 KB)
│   ├── plpgsql.wasm (300 KB)
│   └── webconfig.wasm (200 KB)
```

### Implementation with Wasmtime

**Host (Wasm runtime integration):**

```rust
// src/wasm_host.rs
use wasmtime::{Engine, Module, Store, Instance, TypedFunc};

pub struct WasmPluginHost {
    engine: Engine,
    store: Store<HostState>,
}

impl WasmPluginHost {
    pub fn new() -> Self {
        let engine = Engine::default();
        let store = Store::new(&engine, HostState::default());
        Self { engine, store }
    }
    
    pub fn load_plugin(&mut self, wasm_path: &Path) -> Result<WasmPlugin> {
        let module = Module::from_file(&self.engine, wasm_path)?;
        
        // Create linker with host functions
        let mut linker = wasmtime::Linker::new(&self.engine);
        linker.func_wrap("host", "log", |msg: i32, len: i32| {
            // Safe logging from Wasm
        })?;
        
        let instance = linker.instantiate(&mut self.store, &module)?;
        
        // Get exported functions
        let init: TypedFunc<(), i32> = instance
            .get_typed_func(&mut self.store, "init")?;
        
        Ok(WasmPlugin { instance, init })
    }
}
```

**Plugin (compiled to Wasm):**

```rust
// wasm-plugins/metrics/src/lib.rs
// Compiled with: cargo build --target wasm32-wasi

#[no_mangle]
pub extern "C" fn init() -> i32 {
    match run_metrics_server() {
        Ok(_) => 0,
        Err(_) => -1,
    }
}

fn run_metrics_server() -> Result<()> {
    // Wasm-compatible metrics implementation
    // Limited to WASI capabilities (filesystem, stdout, networking via host)
}
```

### WASI Limitations for PGQT

| Feature | WASI Support | Workaround |
|---------|--------------|------------|
| Raw TCP sockets | ❌ Limited | Host provides socket functions |
| TLS/SSL | ❌ No | Host handles TLS, plugin uses plain text |
| File system | ✅ Yes | Standard WASI |
| Threading | ⚠️ Experimental | Single-threaded or async |
| SQLite access | ❌ No direct access | Host provides query interface |

### Pros & Cons

| Pros | Cons |
|------|------|
| ✅ Sandboxed (plugin crash doesn't kill host) | ❌ Large runtime overhead (5-10 MB) |
| ✅ Platform-independent (.wasm works everywhere) | ❌ Limited system access via WASI |
| ✅ Small plugin sizes (200-500 KB) | ❌ Complex host-plugin interface |
| ✅ Hot reloading supported | ❌ Performance overhead (1.2-2x) |
| | ❌ Not suitable for low-level features (TLS, SQLite) |

---

## 4. Approach 4: Sidecar Binaries

### How It Works
Separate executables that communicate via IPC (sockets, pipes, HTTP):

```
pgqt-core (8 MB)
pgqt-tls-proxy (2 MB)      # Handles TLS termination
pgqt-metrics (1.5 MB)      # Metrics server
pgqt-webconfig (1 MB)      # Web configuration UI
```

### Implementation

**Sidecar protocol (gRPC or simple HTTP):**

```rust
// TLS sidecar
#[tokio::main]
async fn main() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    
    // Tell parent process our port
    println!("TLS_SIDECAR_PORT={}", port);
    
    loop {
        let (stream, _) = listener.accept().await.unwrap();
        let tls_stream = tls_acceptor.accept(stream).await.unwrap();
        // Forward to pgqt-core via Unix socket/TCP
    }
}
```

**Core process spawns sidecars:**

```rust
// src/sidecar.rs
pub struct SidecarManager {
    tls_proxy: Option<Child>,
    metrics: Option<Child>,
}

impl SidecarManager {
    pub fn start_tls_sidecar(&mut self) -> Result<u16> {
        let mut child = Command::new("pgqt-tls-proxy")
            .arg("--parent-port")
            .arg(self.core_port.to_string())
            .stdout(Stdio::piped())
            .spawn()?;
        
        // Parse port from stdout
        let stdout = child.stdout.take().unwrap();
        let port = parse_port_from_output(stdout)?;
        
        self.tls_proxy = Some(child);
        Ok(port)
    }
}
```

### Pros & Cons

| Pros | Cons |
|------|------|
| ✅ Clean separation of concerns | ❌ Process management complexity |
| ✅ Each component can use optimal tech | ❌ IPC overhead |
| ✅ Can restart components independently | ❌ More moving parts to debug |
| ✅ Language-agnostic sidecars | ❌ Resource overhead (multiple processes) |

---

## 5. Comparison Matrix

| Criteria | Feature Flags | Native Plugins | WebAssembly | Sidecars |
|----------|---------------|----------------|-------------|----------|
| **Binary Size (Core)** | 8-15 MB | 8 MB | 13-18 MB | 8 MB |
| **Size per Feature** | Included | 1-3 MB | 200-500 KB | 1-3 MB |
| **Total Size (All Features)** | 15 MB | 20+ MB (duplication) | 16-20 MB | 15 MB |
| **Runtime Overhead** | Zero | Low (vtable) | High (VM) | Medium (IPC) |
| **Development Complexity** | Low | High | Medium | Medium |
| **Distribution Complexity** | Low | Medium | Low | Medium |
| **Security** | N/A | Low (native code) | High (sandboxed) | Medium |
| **Hot Reloading** | ❌ No | ✅ Yes | ✅ Yes | ✅ Yes |
| **Cross-Platform** | Build per target | Build per target | ✅ Universal | Build per target |

---

## 6. Recommendations for PGQT

### Primary Recommendation: Feature Flags with Multiple Binaries

**Why:**
1. **Simplicity:** No runtime complexity, no FFI, no sandboxing overhead
2. **Performance:** Zero overhead, full LTO optimizations
3. **Size:** Smallest total footprint when accounting for duplication
4. **Maintenance:** Standard Rust, no plugin architecture to maintain

**Implementation:**
```bash
# Provide 3 variants per platform
pgqt-minimal     # Core only (~8 MB)
pgqt-standard    # +TLS, +metrics, +web-config (~10 MB)
pgqt-full        # +PL/pgSQL, +system-metrics, +tracing (~15 MB)
```

### Secondary Option: WebAssembly for Extensions (Future)

If users need custom extensions (custom functions, middleware):

```rust
// Future: User-defined functions in Wasm
pgqt --load-wasm ./my_custom_logic.wasm
```

This keeps core features in the binary (performance-critical) while allowing safe extensions.

### What to Avoid

1. **Native plugins (`dylib`):** Too complex for the benefit, ABI headaches
2. **Sidecars:** Overkill for tightly-coupled features like TLS and metrics

---

## 7. Implementation Plan

### Phase 1: Feature Flag Organization (Immediate)
- [ ] Organize features in `Cargo.toml` with clear groups
- [ ] Create `minimal`, `standard`, `full` feature sets
- [ ] Document feature tradeoffs

### Phase 2: CI/CD for Multiple Binaries
- [ ] GitHub Actions matrix builds for all variants
- [ ] Automated release creation with all binaries
- [ ] Install script with variant selection

### Phase 3: Package Managers (Future)
- [ ] Homebrew formula with options: `brew install pgqt --with-full-features`
- [ ] APT/YUM repositories with multiple packages: `pgqt`, `pgqt-full`
- [ ] Docker images: `pgqt:minimal`, `pgqt:standard`, `pgqt:latest`

---

## 8. References

- [abi_stable crate](https://docs.rs/abi_stable/)
- [WebAssembly Component Model](https://component-model.bytecodealliance.org/)
- [Wasmtime Runtime](https://docs.rs/wasmtime/)
- [Rust Plugin Systems Reddit Discussion](https://www.reddit.com/r/rust/comments/o5124j/cglue_frictionless_abi_safety/)
- [Rust ABI Stability](https://users.rust-lang.org/t/dynamic-load-plugins-in-rust/74961)
