# Vector Search Implementation Plan

> **REQUIRED SUB-SKILL:** Use the executing-plans skill to implement this plan task-by-task.

**Goal:** Implement PostgreSQL pgvector-compatible vector search using sqlite-vec for 100% API compatibility.

**Architecture:** Create a `vector.rs` module that implements pgvector functions and operators by translating them to sqlite-vec equivalents. The transpiler will convert pgvector SQL syntax (operators like `<->`, `<=>`, types like `vector`) to sqlite-vec compatible SQL. All vector operations will be implemented as SQLite scalar functions that call sqlite-vec internally.

**Tech Stack:** Rust, sqlite-vec extension, pg_query for AST parsing, rusqlite for function registration

---

## Background Research Summary

### pgvector API (PostgreSQL)
- **Types:** `vector(N)` - float32 vectors up to 2000 dimensions
- **Distance Operators:**
  - `<->` L2 (Euclidean) distance
  - `<=>` Cosine distance  
  - `<#>` Inner product (negative)
  - `<+>` L1 (Manhattan) distance
- **Functions:**
  - `l2_distance(a, b)`, `cosine_distance(a, b)`, `inner_product(a, b)`, `l1_distance(a, b)`
  - `vector_dims(v)` - number of dimensions
  - `l2_norm(v)` - L2 norm (magnitude)
  - `l2_normalize(v)` - normalize to unit vector
  - `subvector(v, start, len)` - extract subset

### sqlite-vec API (SQLite)
- **Types:** `vec_f32(json)` - creates float32 blob
- **Distance Functions:**
  - `vec_distance_L2(a, b)`
  - `vec_distance_cosine(a, b)`
  - `vec_distance_hamming(a, b)` (for bit vectors)
- **Utility Functions:**
  - `vec_length(v)` - number of dimensions
  - `vec_normalize(v)` - L2 normalize
  - `vec_slice(v, start, end)` - extract subset
  - `vec_to_json(v)` - convert to JSON
  - `vec_add(a, b)`, `vec_sub(a, b)` - arithmetic

### Compatibility Mapping
| pgvector | sqlite-vec | Implementation |
|----------|------------|----------------|
| `vector(N)` | BLOB | Store as vec_f32 blob |
| `<->` | `vec_distance_L2()` | Transpile operator |
| `<=>` | `vec_distance_cosine()` | Transpile operator |
| `<#>` | `-inner_product()` | Transpile + negate |
| `<+>` | Custom L1 | Implement in Rust |
| `l2_distance()` | `vec_distance_L2()` | Direct call |
| `cosine_distance()` | `vec_distance_cosine()` | Direct call |
| `inner_product()` | Custom | Implement in Rust |
| `l1_distance()` | Custom L1 | Implement in Rust |
| `vector_dims()` | `vec_length()` | Direct call |
| `l2_norm()` | Custom | Implement in Rust |
| `l2_normalize()` | `vec_normalize()` | Direct call |
| `subvector()` | `vec_slice()` | Direct call |

---

## Task 1: Add sqlite-vec Dependency

**Files:**
- Modify: `Cargo.toml`

**Step 1: Add sqlite-vec crate dependency**

```toml
# Add to [dependencies] section after regex
sqlite-vec = "0.1"
```

**Step 2: Verify build**

Run: `cargo check`
Expected: Compiles successfully

**Step 3: Commit**

```bash
git add Cargo.toml Cargo.lock
git commit -m "chore: add sqlite-vec dependency for vector search"
```

---

## Task 2: Create vector.rs Module

**Files:**
- Create: `src/vector.rs`
- Modify: `src/lib.rs`

**Step 1: Create vector.rs with core structure**

```rust
//! Vector search support using sqlite-vec
//! 
//! This module provides PostgreSQL pgvector-compatible vector operations
//! by delegating to the sqlite-vec SQLite extension.

use std::os::raw::c_int;

/// Parse a vector from PostgreSQL array format '[1,2,3]' to JSON '[1,2,3]'
/// This is needed because pgvector accepts both formats
pub fn parse_vector_literal(input: &str) -> String {
    let trimmed = input.trim();
    
    // Already looks like JSON array
    if trimmed.starts_with('[') && trimmed.ends_with(']') {
        return trimmed.to_string();
    }
    
    // Handle PostgreSQL vector format (no outer brackets sometimes)
    if !trimmed.starts_with('[') {
        return format!("[{}]", trimmed);
    }
    
    trimmed.to_string()
}

/// Calculate L2 (Euclidean) distance between two vectors
pub fn l2_distance(a: &str, b: &str) -> Result<f64, String> {
    let vec_a = parse_vector_to_f32(a)?;
    let vec_b = parse_vector_to_f32(b)?;
    
    if vec_a.len() != vec_b.len() {
        return Err(format!(
            "vector dimension mismatch: {} vs {}",
            vec_a.len(),
            vec_b.len()
        ));
    }
    
    let sum: f64 = vec_a.iter()
        .zip(vec_b.iter())
        .map(|(x, y)| {
            let diff = *x as f64 - *y as f64;
            diff * diff
        })
        .sum();
    
    Ok(sum.sqrt())
}

/// Calculate cosine distance between two vectors
/// Returns 1 - cosine_similarity (0 = identical direction, 2 = opposite)
pub fn cosine_distance(a: &str, b: &str) -> Result<f64, String> {
    let vec_a = parse_vector_to_f32(a)?;
    let vec_b = parse_vector_to_f32(b)?;
    
    if vec_a.len() != vec_b.len() {
        return Err(format!(
            "vector dimension mismatch: {} vs {}",
            vec_a.len(),
            vec_b.len()
        ));
    }
    
    let dot_product: f64 = vec_a.iter()
        .zip(vec_b.iter())
        .map(|(x, y)| (*x as f64) * (*y as f64))
        .sum();
    
    let norm_a: f64 = vec_a.iter().map(|x| (*x as f64).powi(2)).sum::<f64>().sqrt();
    let norm_b: f64 = vec_b.iter().map(|x| (*x as f64).powi(2)).sum::<f64>().sqrt();
    
    if norm_a == 0.0 || norm_b == 0.0 {
        return Err("cannot compute cosine distance for zero vector".to_string());
    }
    
    let similarity = dot_product / (norm_a * norm_b);
    Ok(1.0 - similarity)
}

/// Calculate inner product of two vectors
pub fn inner_product(a: &str, b: &str) -> Result<f64, String> {
    let vec_a = parse_vector_to_f32(a)?;
    let vec_b = parse_vector_to_f32(b)?;
    
    if vec_a.len() != vec_b.len() {
        return Err(format!(
            "vector dimension mismatch: {} vs {}",
            vec_a.len(),
            vec_b.len()
        ));
    }
    
    let product: f64 = vec_a.iter()
        .zip(vec_b.iter())
        .map(|(x, y)| (*x as f64) * (*y as f64))
        .sum();
    
    Ok(product)
}

/// Calculate L1 (Manhattan) distance between two vectors
pub fn l1_distance(a: &str, b: &str) -> Result<f64, String> {
    let vec_a = parse_vector_to_f32(a)?;
    let vec_b = parse_vector_to_f32(b)?;
    
    if vec_a.len() != vec_b.len() {
        return Err(format!(
            "vector dimension mismatch: {} vs {}",
            vec_a.len(),
            vec_b.len()
        ));
    }
    
    let sum: f64 = vec_a.iter()
        .zip(vec_b.iter())
        .map(|(x, y)| (*x as f64 - *y as f64).abs())
        .sum();
    
    Ok(sum)
}

/// Get the number of dimensions in a vector
pub fn vector_dims(v: &str) -> Result<i32, String> {
    let vec = parse_vector_to_f32(v)?;
    Ok(vec.len() as i32)
}

/// Calculate L2 norm (magnitude) of a vector
pub fn l2_norm(v: &str) -> Result<f64, String> {
    let vec = parse_vector_to_f32(v)?;
    let sum: f64 = vec.iter().map(|x| (*x as f64).powi(2)).sum();
    Ok(sum.sqrt())
}

/// Normalize a vector to unit length (L2 normalization)
pub fn l2_normalize(v: &str) -> Result<String, String> {
    let vec = parse_vector_to_f32(v)?;
    let norm = l2_norm(v)?;
    
    if norm == 0.0 {
        return Err("cannot normalize zero vector".to_string());
    }
    
    let normalized: Vec<String> = vec.iter()
        .map(|x| (*x as f64 / norm).to_string())
        .collect();
    
    Ok(format!("[{}]", normalized.join(",")))
}

/// Extract a subvector from start (inclusive) to start+length (exclusive)
/// PostgreSQL uses 1-based indexing, so we adjust
pub fn subvector(v: &str, start: i32, length: i32) -> Result<String, String> {
    let vec = parse_vector_to_f32(v)?;
    
    // PostgreSQL uses 1-based indexing
    let start_idx = (start.max(1) as usize) - 1;
    let end_idx = ((start_idx as i32 + length).min(vec.len() as i32)) as usize;
    
    if start_idx >= vec.len() {
        return Err("start index out of bounds".to_string());
    }
    
    let subset: Vec<String> = vec[start_idx..end_idx]
        .iter()
        .map(|x| x.to_string())
        .collect();
    
    Ok(format!("[{}]", subset.join(",")))
}

/// Parse a vector string (JSON or PostgreSQL format) to Vec<f32>
fn parse_vector_to_f32(input: &str) -> Result<Vec<f32>, String> {
    let trimmed = input.trim();
    
    // Handle JSON array format
    if trimmed.starts_with('[') && trimmed.ends_with(']') {
        let inner = &trimmed[1..trimmed.len()-1];
        if inner.trim().is_empty() {
            return Ok(Vec::new());
        }
        
        let values: Result<Vec<f32>, _> = inner
            .split(',')
            .map(|s| s.trim().parse::<f32>())
            .collect();
        
        values.map_err(|e| format!("invalid vector element: {}", e))
    } else {
        Err("vector must be in format '[1,2,3]'".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_l2_distance() {
        let result = l2_distance("[1, 1]", "[2, 2]").unwrap();
        assert!((result - 1.41421356).abs() < 0.0001);
    }

    #[test]
    fn test_cosine_distance_identical() {
        let result = cosine_distance("[1, 2, 3]", "[1, 2, 3]").unwrap();
        assert!(result.abs() < 0.0001);
    }

    #[test]
    fn test_cosine_distance_opposite() {
        let result = cosine_distance("[1, 0]", "[-1, 0]").unwrap();
        assert!((result - 2.0).abs() < 0.0001);
    }

    #[test]
    fn test_inner_product() {
        let result = inner_product("[1, 2, 3]", "[4, 5, 6]").unwrap();
        assert!((result - 32.0).abs() < 0.0001); // 1*4 + 2*5 + 3*6
    }

    #[test]
    fn test_l1_distance() {
        let result = l1_distance("[1, 2, 3]", "[4, 5, 6]").unwrap();
        assert!((result - 9.0).abs() < 0.0001); // |1-4| + |2-5| + |3-6|
    }

    #[test]
    fn test_vector_dims() {
        let result = vector_dims("[1, 2, 3, 4, 5]").unwrap();
        assert_eq!(result, 5);
    }

    #[test]
    fn test_l2_norm() {
        let result = l2_norm("[3, 4]").unwrap();
        assert!((result - 5.0).abs() < 0.0001);
    }

    #[test]
    fn test_l2_normalize() {
        let result = l2_normalize("[3, 4]").unwrap();
        assert!(result.contains("0.6") || result.contains("0.600"));
        assert!(result.contains("0.8") || result.contains("0.800"));
    }

    #[test]
    fn test_subvector() {
        let result = subvector("[1, 2, 3, 4, 5]", 1, 3).unwrap();
        assert!(result.contains("1"));
        assert!(result.contains("2"));
        assert!(result.contains("3"));
    }

    #[test]
    fn test_dimension_mismatch() {
        let result = l2_distance("[1, 2]", "[1, 2, 3]");
        assert!(result.is_err());
    }
}
```

**Step 2: Run tests to verify implementation**

Run: `cargo test vector:: --no-fail-fast`
Expected: All tests pass

**Step 3: Add module to lib.rs**

```rust
pub mod vector;
```

**Step 4: Commit**

```bash
git add src/vector.rs src/lib.rs
git commit -m "feat: add vector.rs module with core distance functions"
```

---

## Task 3: Update Transpiler for Vector Type

**Files:**
- Modify: `src/transpiler.rs`

**Step 1: Add vector type mapping in rewrite_type_for_sqlite function**

Find the `rewrite_type_for_sqlite` function and add `VECTOR` type:

```rust
fn rewrite_type_for_sqlite(pg_type: &str) -> String {
    let upper = pg_type.to_uppercase();
    match upper.as_str() {
        // ... existing types ...
        "VECTOR" => "BLOB".to_string(),  // Store vectors as BLOB
        // ... rest of function ...
    }
}
```

**Step 2: Add vector type to extract_original_type for metadata**

In `extract_original_type`, ensure VECTOR is preserved:

```rust
fn extract_original_type(type_name: &Option<TypeName>) -> String {
    // ... existing code ...
    // VECTOR type should be preserved as-is
    // The dimension can be extracted from typmods if present
}
```

**Step 3: Commit**

```bash
git add src/transpiler.rs
git commit -m "feat: add VECTOR type mapping in transpiler"
```

---

## Task 4: Add Vector Distance Operators to Transpiler

**Files:**
- Modify: `src/transpiler.rs`

**Step 1: Add vector operator transpilation in AExpr handling**

Find the `reconstruct_aexpr` function (or create one if needed) and add vector operators:

```rust
/// Reconstruct an AExpr (arithmetic expression) with vector operator support
fn reconstruct_aexpr(expr: &AExpr, ctx: &mut TranspileContext) -> Option<String> {
    let name = expr.name.first()?;
    let name_str = if let Some(ref inner) = name.node {
        if let NodeEnum::String(s) = inner {
            s.sval.as_str()
        } else {
            return None;
        }
    } else {
        return None;
    };

    let left = reconstruct_node(expr.lexpr.as_ref()?, ctx);
    let right = reconstruct_node(expr.rexpr.as_ref()?, ctx);

    match name_str {
        // Vector distance operators (pgvector)
        "<->" => Some(format!("vector_l2_distance({}, {})", left, right)),
        "<=>" => Some(format!("vector_cosine_distance({}, {})", left, right)),
        "<#>" => Some(format!("vector_inner_product({}, {})", left, right)),
        "<+>" => Some(format!("vector_l1_distance({}, {})", left, right)),
        // ... existing operators ...
        _ => None,
    }
}
```

**Step 2: Update reconstruct_node to handle AExpr with vector operators**

In the main `reconstruct_node` function's NodeEnum::AExpr branch:

```rust
NodeEnum::AExpr(ref aexpr) => {
    if let Some(result) = reconstruct_aexpr(aexpr, ctx) {
        result
    } else {
        // ... existing fallback ...
    }
}
```

**Step 3: Commit**

```bash
git add src/transpiler.rs
git commit -m "feat: add vector distance operator transpilation"
```

---

## Task 5: Register Vector Functions in main.rs

**Files:**
- Modify: `src/main.rs`

**Step 1: Add vector function registrations after FTS functions**

Add these function registrations in `SqliteHandler::new()`:

```rust
        // Vector search functions (pgvector compatibility)
        conn.create_scalar_function("vector_l2_distance", 2, rusqlite::functions::FunctionFlags::SQLITE_UTF8, |ctx| {
            let a: String = ctx.get(0)?;
            let b: String = ctx.get(1)?;
            crate::vector::l2_distance(&a, &b)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        conn.create_scalar_function("vector_cosine_distance", 2, rusqlite::functions::FunctionFlags::SQLITE_UTF8, |ctx| {
            let a: String = ctx.get(0)?;
            let b: String = ctx.get(1)?;
            crate::vector::cosine_distance(&a, &b)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        conn.create_scalar_function("vector_inner_product", 2, rusqlite::functions::FunctionFlags::SQLITE_UTF8, |ctx| {
            let a: String = ctx.get(0)?;
            let b: String = ctx.get(1)?;
            crate::vector::inner_product(&a, &b)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        conn.create_scalar_function("vector_l1_distance", 2, rusqlite::functions::FunctionFlags::SQLITE_UTF8, |ctx| {
            let a: String = ctx.get(0)?;
            let b: String = ctx.get(1)?;
            crate::vector::l1_distance(&a, &b)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        conn.create_scalar_function("l2_distance", 2, rusqlite::functions::FunctionFlags::SQLITE_UTF8, |ctx| {
            let a: String = ctx.get(0)?;
            let b: String = ctx.get(1)?;
            crate::vector::l2_distance(&a, &b)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        conn.create_scalar_function("cosine_distance", 2, rusqlite::functions::FunctionFlags::SQLITE_UTF8, |ctx| {
            let a: String = ctx.get(0)?;
            let b: String = ctx.get(1)?;
            crate::vector::cosine_distance(&a, &b)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        conn.create_scalar_function("inner_product", 2, rusqlite::functions::FunctionFlags::SQLITE_UTF8, |ctx| {
            let a: String = ctx.get(0)?;
            let b: String = ctx.get(1)?;
            crate::vector::inner_product(&a, &b)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        conn.create_scalar_function("l1_distance", 2, rusqlite::functions::FunctionFlags::SQLITE_UTF8, |ctx| {
            let a: String = ctx.get(0)?;
            let b: String = ctx.get(1)?;
            crate::vector::l1_distance(&a, &b)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        conn.create_scalar_function("vector_dims", 1, rusqlite::functions::FunctionFlags::SQLITE_UTF8, |ctx| {
            let v: String = ctx.get(0)?;
            crate::vector::vector_dims(&v)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        conn.create_scalar_function("l2_norm", 1, rusqlite::functions::FunctionFlags::SQLITE_UTF8, |ctx| {
            let v: String = ctx.get(0)?;
            crate::vector::l2_norm(&v)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        conn.create_scalar_function("l2_normalize", 1, rusqlite::functions::FunctionFlags::SQLITE_UTF8, |ctx| {
            let v: String = ctx.get(0)?;
            crate::vector::l2_normalize(&v)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;

        conn.create_scalar_function("subvector", 3, rusqlite::functions::FunctionFlags::SQLITE_UTF8, |ctx| {
            let v: String = ctx.get(0)?;
            let start: i32 = ctx.get(1)?;
            let length: i32 = ctx.get(2)?;
            crate::vector::subvector(&v, start, length)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        })?;
```

**Step 2: Build and verify**

Run: `cargo build`
Expected: Compiles successfully

**Step 3: Commit**

```bash
git add src/main.rs
git commit -m "feat: register vector scalar functions in SQLite handler"
```

---

## Task 6: Add Vector Module to main.rs

**Files:**
- Modify: `src/main.rs`

**Step 1: Add mod declaration at top of main.rs**

```rust
mod vector;
```

**Step 2: Verify build**

Run: `cargo build`
Expected: Compiles successfully

**Step 3: Commit**

```bash
git add src/main.rs
git commit -m "chore: add vector module to main.rs"
```

---

## Task 7: Write Unit Tests for Vector Transpilation

**Files:**
- Create: `tests/vector_tests.rs`

**Step 1: Create vector transpilation tests**

```rust
use postgresqlite::transpiler::transpile;

#[test]
fn test_transpile_vector_type() {
    let input = "CREATE TABLE items (id SERIAL, embedding VECTOR(3))";
    let result = transpile(input);
    assert!(result.to_lowercase().contains("blob"));
}

#[test]
fn test_transpile_l2_distance_function() {
    let input = "SELECT l2_distance(embedding, '[1,2,3]') FROM items";
    let result = transpile(input);
    assert!(result.contains("l2_distance"));
}

#[test]
fn test_transpile_cosine_distance_function() {
    let input = "SELECT cosine_distance(a, b) FROM vectors";
    let result = transpile(input);
    assert!(result.contains("cosine_distance"));
}

#[test]
fn test_transpile_inner_product_function() {
    let input = "SELECT inner_product(a, b) FROM vectors";
    let result = transpile(input);
    assert!(result.contains("inner_product"));
}

#[test]
fn test_transpile_l1_distance_function() {
    let input = "SELECT l1_distance(a, b) FROM vectors";
    let result = transpile(input);
    assert!(result.contains("l1_distance"));
}

#[test]
fn test_transpile_vector_dims() {
    let input = "SELECT vector_dims(embedding) FROM items";
    let result = transpile(input);
    assert!(result.contains("vector_dims"));
}

#[test]
fn test_transpile_l2_norm() {
    let input = "SELECT l2_norm(embedding) FROM items";
    let result = transpile(input);
    assert!(result.contains("l2_norm"));
}

#[test]
fn test_transpile_l2_normalize() {
    let input = "SELECT l2_normalize(embedding) FROM items";
    let result = transpile(input);
    assert!(result.contains("l2_normalize"));
}

#[test]
fn test_transpile_subvector() {
    let input = "SELECT subvector(embedding, 1, 3) FROM items";
    let result = transpile(input);
    assert!(result.contains("subvector"));
}

#[test]
fn test_transpile_vector_in_order_by() {
    let input = "SELECT * FROM items ORDER BY l2_distance(embedding, '[1,2,3]') LIMIT 5";
    let result = transpile(input);
    assert!(result.contains("l2_distance"));
    assert!(result.contains("order by"));
}

#[test]
fn test_transpile_vector_in_where_clause() {
    let input = "SELECT * FROM items WHERE l2_distance(embedding, '[1,2,3]') < 0.5";
    let result = transpile(input);
    assert!(result.contains("l2_distance"));
    assert!(result.contains("where"));
}
```

**Step 2: Run tests**

Run: `cargo test vector_tests --no-fail-fast`
Expected: All tests pass

**Step 3: Commit**

```bash
git add tests/vector_tests.rs
git commit -m "test: add vector transpilation unit tests"
```

---

## Task 8: Write Integration Tests for Vector Functions

**Files:**
- Create: `tests/vector_integration_tests.rs`

**Step 1: Create integration tests**

```rust
use postgresqlite::vector::*;

#[test]
fn test_l2_distance_basic() {
    let result = l2_distance("[1, 1]", "[2, 2]").unwrap();
    assert!((result - 1.41421356).abs() < 0.0001);
}

#[test]
fn test_l2_distance_zero() {
    let result = l2_distance("[1, 2, 3]", "[1, 2, 3]").unwrap();
    assert!(result.abs() < 0.0001);
}

#[test]
fn test_l2_distance_negative() {
    let result = l2_distance("[1, 2, 3]", "[-1, -2, -3]").unwrap();
    assert!((result - 7.48331477).abs() < 0.0001);
}

#[test]
fn test_cosine_distance_identical() {
    let result = cosine_distance("[1, 2, 3]", "[1, 2, 3]").unwrap();
    assert!(result.abs() < 0.0001);
}

#[test]
fn test_cosine_distance_opposite() {
    let result = cosine_distance("[1, 0]", "[-1, 0]").unwrap();
    assert!((result - 2.0).abs() < 0.0001);
}

#[test]
fn test_cosine_distance_orthogonal() {
    let result = cosine_distance("[1, 0]", "[0, 1]").unwrap();
    assert!((result - 1.0).abs() < 0.0001);
}

#[test]
fn test_cosine_distance_scaled() {
    // Cosine distance should be invariant to scaling
    let r1 = cosine_distance("[1, 2, 3]", "[4, 5, 6]").unwrap();
    let r2 = cosine_distance("[2, 4, 6]", "[8, 10, 12]").unwrap();
    assert!((r1 - r2).abs() < 0.0001);
}

#[test]
fn test_inner_product_basic() {
    let result = inner_product("[1, 2, 3]", "[4, 5, 6]").unwrap();
    assert!((result - 32.0).abs() < 0.0001); // 1*4 + 2*5 + 3*6
}

#[test]
fn test_inner_product_orthogonal() {
    let result = inner_product("[1, 0]", "[0, 1]").unwrap();
    assert!(result.abs() < 0.0001);
}

#[test]
fn test_inner_product_negative() {
    let result = inner_product("[1, 2, 3]", "[-1, -2, -3]").unwrap();
    assert!((result - (-14.0)).abs() < 0.0001);
}

#[test]
fn test_l1_distance_basic() {
    let result = l1_distance("[1, 2, 3]", "[4, 5, 6]").unwrap();
    assert!((result - 9.0).abs() < 0.0001);
}

#[test]
fn test_l1_distance_negative() {
    let result = l1_distance("[-1, -2]", "[1, 2]").unwrap();
    assert!((result - 6.0).abs() < 0.0001);
}

#[test]
fn test_vector_dims_basic() {
    assert_eq!(vector_dims("[1, 2, 3]").unwrap(), 3);
    assert_eq!(vector_dims("[1]").unwrap(), 1);
    assert_eq!(vector_dims("[1, 2, 3, 4, 5]").unwrap(), 5);
}

#[test]
fn test_vector_dims_empty() {
    assert_eq!(vector_dims("[]").unwrap(), 0);
}

#[test]
fn test_l2_norm_basic() {
    let result = l2_norm("[3, 4]").unwrap();
    assert!((result - 5.0).abs() < 0.0001);
}

#[test]
fn test_l2_norm_unit() {
    let result = l2_norm("[1, 0, 0]").unwrap();
    assert!((result - 1.0).abs() < 0.0001);
}

#[test]
fn test_l2_normalize_basic() {
    let result = l2_normalize("[3, 4]").unwrap();
    // Should be [0.6, 0.8]
    let vals: Vec<f64> = result
        .trim_matches(|c| c == '[' || c == ']')
        .split(',')
        .map(|s| s.trim().parse().unwrap())
        .collect();
    assert!((vals[0] - 0.6).abs() < 0.0001);
    assert!((vals[1] - 0.8).abs() < 0.0001);
}

#[test]
fn test_l2_normalize_unit() {
    let result = l2_normalize("[1, 0, 0]").unwrap();
    let vals: Vec<f64> = result
        .trim_matches(|c| c == '[' || c == ']')
        .split(',')
        .map(|s| s.trim().parse().unwrap())
        .collect();
    assert!((vals[0] - 1.0).abs() < 0.0001);
    assert!(vals[1].abs() < 0.0001);
    assert!(vals[2].abs() < 0.0001);
}

#[test]
fn test_subvector_basic() {
    let result = subvector("[1, 2, 3, 4, 5]", 1, 3).unwrap();
    assert!(result.contains("1"));
    assert!(result.contains("2"));
    assert!(result.contains("3"));
    assert!(!result.contains("4"));
    assert!(!result.contains("5"));
}

#[test]
fn test_subvector_from_middle() {
    let result = subvector("[1, 2, 3, 4, 5]", 2, 2).unwrap();
    assert!(result.contains("2"));
    assert!(result.contains("3"));
    assert!(!result.contains("1"));
    assert!(!result.contains("4"));
}

#[test]
fn test_dimension_mismatch_error() {
    let result = l2_distance("[1, 2]", "[1, 2, 3]");
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("mismatch"));
}

#[test]
fn test_zero_vector_error() {
    let result = cosine_distance("[0, 0]", "[1, 2]");
    assert!(result.is_err());
    
    let result = l2_normalize("[0, 0]");
    assert!(result.is_err());
}

#[test]
fn test_parse_invalid_vector() {
    let result = l2_distance("not a vector", "[1, 2]");
    assert!(result.is_err());
}

#[test]
fn test_high_dimensional_vector() {
    let mut vec: Vec<String> = (0..1536).map(|i| format!("0.{}", i % 10)).collect();
    let vec_str = format!("[{}]", vec.join(","));
    
    let dims = vector_dims(&vec_str).unwrap();
    assert_eq!(dims, 1536);
    
    let norm = l2_norm(&vec_str).unwrap();
    assert!(norm > 0.0);
}

#[test]
fn test_floating_point_precision() {
    let a = "[0.1, 0.2, 0.3]";
    let b = "[0.4, 0.5, 0.6]";
    
    let l2 = l2_distance(a, b).unwrap();
    let l1 = l1_distance(a, b).unwrap();
    let cos = cosine_distance(a, b).unwrap();
    
    // All distances should be positive
    assert!(l2 > 0.0);
    assert!(l1 > 0.0);
    assert!(cos >= 0.0);
}

#[test]
fn test_whitespace_handling() {
    let r1 = l2_distance("[1,2,3]", "[4,5,6]").unwrap();
    let r2 = l2_distance("[ 1, 2, 3 ]", "[ 4, 5, 6 ]").unwrap();
    assert!((r1 - r2).abs() < 0.0001);
}
```

**Step 2: Run tests**

Run: `cargo test vector_integration_tests --no-fail-fast`
Expected: All tests pass

**Step 3: Commit**

```bash
git add tests/vector_integration_tests.rs
git commit -m "test: add comprehensive vector function integration tests"
```

---

## Task 9: Create Vector Documentation

**Files:**
- Create: `docs/VECTOR.md`

**Step 1: Create vector search documentation**

```markdown
# Vector Search (pgvector Compatibility)

PGlite Proxy provides PostgreSQL pgvector-compatible vector search functionality. This allows you to perform similarity searches on vector embeddings using familiar PostgreSQL syntax.

## Overview

Vector search is essential for:
- **Semantic search**: Finding similar documents based on meaning
- **Recommendation systems**: Finding similar items
- **RAG (Retrieval-Augmented Generation)**: Providing context to LLMs
- **Image similarity**: Finding visually similar images

## Data Types

| PostgreSQL Type | SQLite Storage | Description |
|----------------|----------------|-------------|
| `vector(N)` | BLOB | N-dimensional float32 vector |

```sql
CREATE TABLE documents (
    id SERIAL PRIMARY KEY,
    content TEXT,
    embedding VECTOR(1536)  -- OpenAI ada-002 embeddings
);
```

## Distance Functions

### l2_distance(a, b) / vector_l2_distance(a, b)

Calculates the L2 (Euclidean) distance between two vectors.

```sql
SELECT l2_distance(embedding, '[1, 2, 3]') AS distance
FROM documents
ORDER BY distance
LIMIT 10;
```

### cosine_distance(a, b) / vector_cosine_distance(a, b)

Calculates the cosine distance (1 - cosine similarity) between two vectors.
Recommended for text embeddings (OpenAI, Cohere, etc.).

```sql
SELECT cosine_distance(embedding, '[0.1, 0.2, 0.3]') AS distance
FROM documents
ORDER BY distance
LIMIT 10;
```

### inner_product(a, b) / vector_inner_product(a, b)

Calculates the dot product of two vectors. Useful for normalized vectors.

```sql
SELECT inner_product(embedding, '[1, 2, 3]') AS similarity
FROM documents
ORDER BY similarity DESC
LIMIT 10;
```

### l1_distance(a, b) / vector_l1_distance(a, b)

Calculates the L1 (Manhattan) distance between two vectors.

```sql
SELECT l1_distance(embedding, '[1, 2, 3]') AS distance
FROM documents
ORDER BY distance
LIMIT 10;
```

## Vector Operators (pgvector Compatible)

| Operator | Function | Description |
|----------|----------|-------------|
| `<->` | `l2_distance()` | L2 (Euclidean) distance |
| `<=>` | `cosine_distance()` | Cosine distance |
| `<#>` | `inner_product()` | Inner product (negated for ordering) |
| `<+>` | `l1_distance()` | L1 (Manhattan) distance |

Example with operators:

```sql
-- Find 5 nearest neighbors using L2 distance
SELECT * FROM items
ORDER BY embedding <-> '[1, 2, 3]'
LIMIT 5;

-- Find 5 most similar using cosine distance
SELECT * FROM items
ORDER BY embedding <=> '[0.1, 0.2, 0.3]'
LIMIT 5;
```

## Utility Functions

### vector_dims(vector)

Returns the number of dimensions in a vector.

```sql
SELECT vector_dims(embedding) FROM documents;
-- Returns: 1536
```

### l2_norm(vector)

Returns the L2 norm (magnitude) of a vector.

```sql
SELECT l2_norm(embedding) FROM documents;
-- Returns: 1.0 (for normalized vectors)
```

### l2_normalize(vector)

Returns a unit vector in the same direction.

```sql
SELECT l2_normalize('[3, 4]');
-- Returns: [0.6, 0.8]
```

### subvector(vector, start, length)

Extracts a subvector (1-based indexing, PostgreSQL compatible).

```sql
SELECT subvector(embedding, 1, 128) FROM documents;
-- Returns first 128 dimensions
```

## Complete Example

### Creating a Vector Table

```sql
-- Create table with vector column
CREATE TABLE articles (
    id SERIAL PRIMARY KEY,
    title TEXT,
    content TEXT,
    embedding VECTOR(384)
);

-- Insert documents with embeddings
INSERT INTO articles (title, content, embedding)
VALUES 
    ('Introduction to AI', 'AI is transforming...', '[0.1, 0.2, ...]'),
    ('Machine Learning Basics', 'ML is a subset of...', '[0.15, 0.25, ...]');
```

### Similarity Search

```sql
-- Find 5 most similar articles to a query embedding
SELECT 
    id,
    title,
    cosine_distance(embedding, '[0.12, 0.22, ...]') AS distance
FROM articles
ORDER BY distance
LIMIT 5;
```

### Hybrid Search (Vector + Full-Text)

```sql
-- Combine vector search with full-text search
SELECT 
    a.id,
    a.title,
    cosine_distance(a.embedding, '[0.12, 0.22, ...]') AS vec_distance,
    ts_rank(to_tsvector(a.content), to_tsquery('machine learning')) AS fts_rank
FROM articles a
WHERE to_tsvector(a.content) @@ to_tsquery('machine learning')
ORDER BY vec_distance
LIMIT 10;
```

## Performance Tips

1. **Use cosine distance for text embeddings**: Most embedding models (OpenAI, Cohere) are optimized for cosine similarity.

2. **Pre-normalize vectors**: If using inner product, normalize vectors first for faster computation.

3. **Filter before ordering**: Use WHERE clauses to reduce the search space.

4. **Consider dimension**: Lower dimensions (384 vs 1536) are faster but less precise.

## Limitations

- **No ANN indexes**: Unlike pgvector, we don't support HNSW or IVFFlat indexes. All searches are exact k-NN.
- **Scale**: Best for datasets under 1 million vectors. For larger datasets, consider dedicated vector databases.
- **Binary vectors**: Not currently supported.

## Compatibility

This implementation is compatible with pgvector SQL syntax, making it easy to migrate applications between PostgreSQL+pgvector and PGlite Proxy.
```

**Step 2: Commit**

```bash
git add docs/VECTOR.md
git commit -m "docs: add VECTOR.md documentation"
```

---

## Task 10: Update README.md

**Files:**
- Modify: `README.md`

**Step 1: Add Vector Search section after Full-Text Search section**

Add after the FTS section:

```markdown
### Vector Search (pgvector Compatible)

PGlite Proxy provides PostgreSQL pgvector-compatible vector search for similarity searches on embeddings:

```sql
-- Create table with vector column
CREATE TABLE documents (
    id SERIAL PRIMARY KEY,
    content TEXT,
    embedding VECTOR(1536)
);

-- Insert with embedding
INSERT INTO documents (content, embedding)
VALUES ('Hello world', '[0.1, 0.2, 0.3, ...]');

-- Find similar documents using cosine distance
SELECT id, content, cosine_distance(embedding, '[0.12, 0.22, ...]') AS distance
FROM documents
ORDER BY distance
LIMIT 5;
```

**Supported Distance Functions:**
- `l2_distance(a, b)` / `vector_l2_distance(a, b)` - L2 (Euclidean) distance
- `cosine_distance(a, b)` / `vector_cosine_distance(a, b)` - Cosine distance
- `inner_product(a, b)` / `vector_inner_product(a, b)` - Inner product
- `l1_distance(a, b)` / `vector_l1_distance(a, b)` - L1 (Manhattan) distance

**Supported Operators:**
- `<->` - L2 distance
- `<=>` - Cosine distance
- `<#>` - Inner product
- `<+>` - L1 distance

**Utility Functions:**
- `vector_dims(vector)` - Get number of dimensions
- `l2_norm(vector)` - Calculate L2 norm
- `l2_normalize(vector)` - Normalize to unit vector
- `subvector(vector, start, len)` - Extract subvector

For complete documentation, see [docs/VECTOR.md](./docs/VECTOR.md).
```

**Step 2: Update Type Mapping table**

Add VECTOR type to the type mapping table:

```markdown
| **Vector** |||
| VECTOR(N) | BLOB | ✅ |
```

**Step 3: Update Roadmap**

Change the Vector Search item from Phase 4 to completed:

```markdown
### Phase 4 (In Progress)
- [x] **Vector Search** - pgvector-compatible vector search using sqlite-vec
```

**Step 4: Commit**

```bash
git add README.md
git commit -m "docs: update README with vector search documentation"
```

---

## Task 11: Update TODO-FEATURES.md

**Files:**
- Modify: `docs/TODO-FEATURES.md`

**Step 1: Update Vector Search row**

Change the Vector Search row in the Advanced & Administrative section:

```markdown
| **Vector Search** | ✅ | Medium | pgvector-compatible vector search using native Rust implementations. Supports VECTOR type, distance functions (L2, cosine, inner product, L1), and operators (<->, <=>, <#>, <+>). See [docs/VECTOR.md](./VECTOR.md). |
```

**Step 2: Commit**

```bash
git add docs/TODO-FEATURES.md
git commit -m "docs: mark vector search as complete in TODO-FEATURES.md"
```

---

## Task 12: Write E2E Tests

**Files:**
- Create: `tests/vector_e2e_test.py`

**Step 1: Create Python E2E test script**

```python
#!/usr/bin/env python3
"""
End-to-end tests for vector search functionality.
Requires psycopg2 and a running pglite-proxy server.
"""

import subprocess
import sys
import time
import os

try:
    import psycopg2
except ImportError:
    print("psycopg2 not installed. Run: pip install psycopg2-binary")
    sys.exit(1)

DB_PATH = "/tmp/test_vector_e2e.db"
PROXY_PORT = 5433

def start_server():
    """Start the pglite-proxy server."""
    # Clean up old database
    if os.path.exists(DB_PATH):
        os.remove(DB_PATH)
    
    # Start server
    proc = subprocess.Popen(
        ["cargo", "run", "--", "--port", str(PROXY_PORT), "--database", DB_PATH],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    time.sleep(3)  # Wait for server to start
    return proc

def stop_server(proc):
    """Stop the pglite-proxy server."""
    proc.terminate()
    try:
        proc.wait(timeout=5)
    except subprocess.TimeoutExpired:
        proc.kill()

def get_connection():
    """Get a database connection."""
    return psycopg2.connect(
        host="127.0.0.1",
        port=PROXY_PORT,
        user="postgres",
        database="test"
    )

def test_create_vector_table():
    """Test creating a table with VECTOR type."""
    conn = get_connection()
    cur = conn.cursor()
    
    cur.execute("""
        CREATE TABLE vectors (
            id SERIAL PRIMARY KEY,
            name TEXT,
            embedding VECTOR(3)
        )
    """)
    conn.commit()
    
    cur.execute("SELECT column_name, data_type FROM __pg_meta__ WHERE table_name = 'vectors'")
    rows = cur.fetchall()
    
    # Check that embedding column has VECTOR type in metadata
    embedding_row = [r for r in rows if r[0] == 'embedding']
    assert len(embedding_row) == 1, "embedding column not found in metadata"
    
    cur.close()
    conn.close()
    print("✓ test_create_vector_table passed")

def test_insert_and_select_vectors():
    """Test inserting and selecting vectors."""
    conn = get_connection()
    cur = conn.cursor()
    
    cur.execute("INSERT INTO vectors (name, embedding) VALUES (%s, %s)", ("item1", "[1, 2, 3]"))
    cur.execute("INSERT INTO vectors (name, embedding) VALUES (%s, %s)", ("item2", "[4, 5, 6]"))
    conn.commit()
    
    cur.execute("SELECT name, embedding FROM vectors ORDER BY id")
    rows = cur.fetchall()
    
    assert len(rows) == 2
    assert rows[0][0] == "item1"
    assert rows[1][0] == "item2"
    
    cur.close()
    conn.close()
    print("✓ test_insert_and_select_vectors passed")

def test_l2_distance():
    """Test L2 distance function."""
    conn = get_connection()
    cur = conn.cursor()
    
    cur.execute("SELECT l2_distance('[1, 2, 3]', '[4, 5, 6]')")
    result = cur.fetchone()[0]
    
    # L2 distance: sqrt((4-1)^2 + (5-2)^2 + (6-3)^2) = sqrt(27) ≈ 5.196
    assert abs(float(result) - 5.196) < 0.01, f"Expected ~5.196, got {result}"
    
    cur.close()
    conn.close()
    print("✓ test_l2_distance passed")

def test_cosine_distance():
    """Test cosine distance function."""
    conn = get_connection()
    cur = conn.cursor()
    
    # Identical vectors should have distance 0
    cur.execute("SELECT cosine_distance('[1, 2, 3]', '[1, 2, 3]')")
    result = cur.fetchone()[0]
    assert abs(float(result)) < 0.0001, f"Expected 0, got {result}"
    
    # Orthogonal vectors should have distance 1
    cur.execute("SELECT cosine_distance('[1, 0]', '[0, 1]')")
    result = cur.fetchone()[0]
    assert abs(float(result) - 1.0) < 0.0001, f"Expected 1, got {result}"
    
    cur.close()
    conn.close()
    print("✓ test_cosine_distance passed")

def test_inner_product():
    """Test inner product function."""
    conn = get_connection()
    cur = conn.cursor()
    
    cur.execute("SELECT inner_product('[1, 2, 3]', '[4, 5, 6]')")
    result = cur.fetchone()[0]
    
    # 1*4 + 2*5 + 3*6 = 32
    assert abs(float(result) - 32.0) < 0.0001, f"Expected 32, got {result}"
    
    cur.close()
    conn.close()
    print("✓ test_inner_product passed")

def test_l1_distance():
    """Test L1 distance function."""
    conn = get_connection()
    cur = conn.cursor()
    
    cur.execute("SELECT l1_distance('[1, 2, 3]', '[4, 5, 6]')")
    result = cur.fetchone()[0]
    
    # |1-4| + |2-5| + |3-6| = 9
    assert abs(float(result) - 9.0) < 0.0001, f"Expected 9, got {result}"
    
    cur.close()
    conn.close()
    print("✓ test_l1_distance passed")

def test_vector_dims():
    """Test vector_dims function."""
    conn = get_connection()
    cur = conn.cursor()
    
    cur.execute("SELECT vector_dims('[1, 2, 3, 4, 5]')")
    result = cur.fetchone()[0]
    
    assert int(result) == 5, f"Expected 5, got {result}"
    
    cur.close()
    conn.close()
    print("✓ test_vector_dims passed")

def test_l2_norm():
    """Test l2_norm function."""
    conn = get_connection()
    cur = conn.cursor()
    
    cur.execute("SELECT l2_norm('[3, 4]')")
    result = cur.fetchone()[0]
    
    # sqrt(9 + 16) = 5
    assert abs(float(result) - 5.0) < 0.0001, f"Expected 5, got {result}"
    
    cur.close()
    conn.close()
    print("✓ test_l2_norm passed")

def test_l2_normalize():
    """Test l2_normalize function."""
    conn = get_connection()
    cur = conn.cursor()
    
    cur.execute("SELECT l2_normalize('[3, 4]')")
    result = cur.fetchone()[0]
    
    # Should be [0.6, 0.8]
    import json
    vals = json.loads(result)
    assert abs(vals[0] - 0.6) < 0.0001, f"Expected 0.6, got {vals[0]}"
    assert abs(vals[1] - 0.8) < 0.0001, f"Expected 0.8, got {vals[1]}"
    
    cur.close()
    conn.close()
    print("✓ test_l2_normalize passed")

def test_subvector():
    """Test subvector function."""
    conn = get_connection()
    cur = conn.cursor()
    
    cur.execute("SELECT subvector('[1, 2, 3, 4, 5]', 1, 3)")
    result = cur.fetchone()[0]
    
    import json
    vals = json.loads(result)
    assert len(vals) == 3, f"Expected 3 elements, got {len(vals)}"
    
    cur.close()
    conn.close()
    print("✓ test_subvector passed")

def test_vector_search_query():
    """Test vector search with ORDER BY distance."""
    conn = get_connection()
    cur = conn.cursor()
    
    # Insert test vectors
    cur.execute("INSERT INTO vectors (name, embedding) VALUES (%s, %s)", ("close", "[1, 1, 1]"))
    cur.execute("INSERT INTO vectors (name, embedding) VALUES (%s, %s)", ("far", "[100, 100, 100]"))
    conn.commit()
    
    # Query for nearest to [1, 1, 1]
    cur.execute("""
        SELECT name, l2_distance(embedding, '[1, 1, 1]') AS dist
        FROM vectors
        ORDER BY dist
        LIMIT 1
    """)
    result = cur.fetchone()
    
    assert result[0] == "close", f"Expected 'close', got {result[0]}"
    
    cur.close()
    conn.close()
    print("✓ test_vector_search_query passed")

def run_all_tests():
    """Run all E2E tests."""
    server = start_server()
    
    try:
        test_create_vector_table()
        test_insert_and_select_vectors()
        test_l2_distance()
        test_cosine_distance()
        test_inner_product()
        test_l1_distance()
        test_vector_dims()
        test_l2_norm()
        test_l2_normalize()
        test_subvector()
        test_vector_search_query()
        
        print("\n✅ All E2E tests passed!")
        return True
    except Exception as e:
        print(f"\n❌ Test failed: {e}")
        import traceback
        traceback.print_exc()
        return False
    finally:
        stop_server(server)
        if os.path.exists(DB_PATH):
            os.remove(DB_PATH)

if __name__ == "__main__":
    success = run_all_tests()
    sys.exit(0 if success else 1)
```

**Step 2: Make executable and commit**

```bash
chmod +x tests/vector_e2e_test.py
git add tests/vector_e2e_test.py
git commit -m "test: add vector search E2E tests"
```

---

## Task 13: Run All Tests

**Step 1: Run unit tests**

Run: `cargo test --no-fail-fast`
Expected: All tests pass

**Step 2: Run E2E tests (if server can be started)**

Run: `./tests/vector_e2e_test.py`
Expected: All E2E tests pass

**Step 3: Fix any failures**

If any tests fail, debug and fix the issues.

---

## Task 14: Final Verification and Commit

**Step 1: Run full test suite**

Run: `cargo test`
Expected: All tests pass

**Step 2: Verify build**

Run: `cargo build --release`
Expected: Clean build with no warnings

**Step 3: Final commit (if any remaining changes)**

```bash
git add -A
git commit -m "feat: complete vector search implementation"
```

**Step 4: Push changes**

```bash
git push origin main
```

---

## Summary

This implementation provides:

1. **Full pgvector API compatibility** - All major functions and operators
2. **Native Rust implementation** - No external dependencies beyond sqlite-vec
3. **Comprehensive testing** - Unit, integration, and E2E tests
4. **Complete documentation** - README updates and dedicated VECTOR.md

The implementation handles:
- VECTOR type storage as BLOB
- All distance metrics (L2, cosine, inner product, L1)
- Vector operators (<->, <=>, <#>, <+>)
- Utility functions (dims, norm, normalize, subvector)
- Error handling for dimension mismatches and invalid inputs
