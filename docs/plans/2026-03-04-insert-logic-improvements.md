# INSERT Logic Improvements Implementation Plan

> **REQUIRED SUB-SKILL:** Use the executing-plans skill to implement this plan task-by-task.

**Goal:** Implement automatic padding for INSERT statements with fewer values than columns and robust DEFAULT keyword support to match PostgreSQL behavior.

**Architecture:** Introduce a `MetadataProvider` trait to bridge the transpiler with the catalog system, allowing dynamic schema lookups during transpilation. The transpiler will use this to pad missing columns and resolve DEFAULT values.

**Tech Stack:** Rust, pg_query (PostgreSQL parser), rusqlite (SQLite), DashMap (concurrent hash map)

---

## Task 1: Define MetadataProvider Trait and ColumnInfo Struct

**Files:**
- Create: `src/transpiler/metadata.rs`
- Modify: `src/transpiler/mod.rs` (add module declaration)

**Step 1: Create the metadata module**

Create `src/transpiler/metadata.rs`:

```rust
//! Metadata provider trait for transpiler schema lookups
//!
//! This module defines the interface for the transpiler to query
//! database schema information during SQL transpilation.

use std::sync::Arc;

/// Information about a table column
#[derive(Debug, Clone)]
pub struct ColumnInfo {
    pub name: String,
    pub original_type: String,
    pub default_expr: Option<String>,
    pub is_nullable: bool,
}

/// Trait for providing table metadata to the transpiler
///
/// Implementations of this trait allow the transpiler to query
/// the database catalog for schema information during transpilation.
pub trait MetadataProvider: Send + Sync {
    /// Get column information for a table
    ///
    /// Returns a vector of ColumnInfo structs in column order.
    /// Returns None if the table is not found.
    fn get_table_columns(&self, table_name: &str) -> Option<Vec<ColumnInfo>>;
    
    /// Get default expression for a specific column
    ///
    /// Returns the default expression string, or None if no default.
    fn get_column_default(&self, table_name: &str, column_name: &str) -> Option<String>;
}

/// A no-op metadata provider that returns no information
///
/// Used when no catalog access is needed or available.
pub struct NoOpMetadataProvider;

impl MetadataProvider for NoOpMetadataProvider {
    fn get_table_columns(&self, _table_name: &str) -> Option<Vec<ColumnInfo>> {
        None
    }
    
    fn get_column_default(&self, _table_name: &str, _column_name: &str) -> Option<String> {
        None
    }
}

impl NoOpMetadataProvider {
    pub fn new() -> Self {
        Self
    }
}

impl Default for NoOpMetadataProvider {
    fn default() -> Self {
        Self::new()
    }
}
```

**Step 2: Add module declaration to transpiler**

Modify `src/transpiler/mod.rs`:

```rust
// Add after existing module declarations
pub mod metadata;

// Re-export metadata types
pub use metadata::{ColumnInfo, MetadataProvider, NoOpMetadataProvider};
```

**Step 3: Verify compilation**

Run: `cargo check`
Expected: Clean compile with no errors

**Step 4: Commit**

```bash
git add src/transpiler/metadata.rs src/transpiler/mod.rs
git commit -m "feat: add MetadataProvider trait for schema lookups"
```

---

## Task 2: Update TranspileContext to Hold MetadataProvider

**Files:**
- Modify: `src/transpiler/context.rs`

**Step 1: Add MetadataProvider to TranspileContext**

Modify `src/transpiler/context.rs`:

```rust
// Add import at top
use crate::transpiler::metadata::{MetadataProvider, NoOpMetadataProvider};
use std::sync::Arc;

// Update TranspileContext struct
pub struct TranspileContext {
    pub referenced_tables: Vec<String>,
    pub errors: Vec<String>,
    pub functions: Option<Arc<DashMap<String, crate::catalog::FunctionMetadata>>>,
    /// Column aliases for VALUES statements (when VALUES is used with AS alias (col1, col2))
    pub values_column_aliases: Vec<String>,
    /// Whether we're currently in a subquery context (for VALUES handling)
    pub in_subquery: bool,
    /// Metadata provider for schema lookups during transpilation
    metadata_provider: Option<Arc<dyn MetadataProvider>>,
    /// Current column index when processing VALUES (for DEFAULT resolution)
    pub current_column_index: usize,
    /// Current table name when processing INSERT (for metadata lookups)
    pub current_table: Option<String>,
}

impl TranspileContext {
    pub fn new() -> Self {
        Self {
            referenced_tables: Vec::new(),
            errors: Vec::new(),
            functions: None,
            values_column_aliases: Vec::new(),
            in_subquery: false,
            metadata_provider: None,
            current_column_index: 0,
            current_table: None,
        }
    }

    pub fn with_functions(functions: Arc<DashMap<String, crate::catalog::FunctionMetadata>>) -> Self {
        Self {
            referenced_tables: Vec::new(),
            errors: Vec::new(),
            functions: Some(functions),
            values_column_aliases: Vec::new(),
            in_subquery: false,
            metadata_provider: None,
            current_column_index: 0,
            current_table: None,
        }
    }
    
    /// Create a new context with a metadata provider
    pub fn with_metadata_provider(provider: Arc<dyn MetadataProvider>) -> Self {
        Self {
            referenced_tables: Vec::new(),
            errors: Vec::new(),
            functions: None,
            values_column_aliases: Vec::new(),
            in_subquery: false,
            metadata_provider: Some(provider),
            current_column_index: 0,
            current_table: None,
        }
    }
    
    /// Set the metadata provider
    pub fn set_metadata_provider(&mut self, provider: Arc<dyn MetadataProvider>) {
        self.metadata_provider = Some(provider);
    }
    
    /// Get column information for a table
    pub fn get_table_columns(&self, table_name: &str) -> Option<Vec<ColumnInfo>> {
        self.metadata_provider.as_ref()
            .and_then(|p| p.get_table_columns(table_name))
    }
    
    /// Get default expression for a column
    pub fn get_column_default(&self, table_name: &str, column_name: &str) -> Option<String> {
        self.metadata_provider.as_ref()
            .and_then(|p| p.get_column_default(table_name, column_name))
    }

    // ... existing methods remain unchanged ...
}
```

**Step 2: Verify compilation**

Run: `cargo check`
Expected: Clean compile with no errors

**Step 3: Commit**

```bash
git add src/transpiler/context.rs
git commit -m "feat: add MetadataProvider support to TranspileContext"
```

---

## Task 3: Implement MetadataProvider for SqliteHandler

**Files:**
- Modify: `src/handler/mod.rs`
- Modify: `src/catalog/table.rs` (add function to get columns with defaults)

**Step 1: Add function to retrieve table columns with defaults**

Modify `src/catalog/table.rs`:

```rust
/// Get column metadata including default expressions for a table
/// 
/// Returns column information in the order they appear in the table,
/// including default expressions from the catalog.
pub fn get_table_columns_with_defaults(conn: &Connection, table_name: &str) -> Result<Vec<super::ColumnMetadata>> {
    let mut stmt = conn.prepare(
        "SELECT table_name, column_name, original_type, constraints
         FROM __pg_meta__
         WHERE table_name = ?1
         ORDER BY rowid"
    )?;

    let rows = stmt.query_map([table_name], |row| {
        Ok(super::ColumnMetadata {
            table_name: row.get(0)?,
            column_name: row.get(1)?,
            original_type: row.get(2)?,
            constraints: row.get(3)?,
        })
    })?;

    let mut result = Vec::new();
    for row in rows {
        result.push(row?);
    }
    
    // If no metadata in catalog, fall back to pragma_table_info
    if result.is_empty() {
        let mut pragma_stmt = conn.prepare(
            "SELECT name, type, cid, dflt_value FROM pragma_table_info(?1) ORDER BY cid"
        )?;
        
        let pragma_rows = pragma_stmt.query_map([table_name], |row| {
            let col_name: String = row.get(0)?;
            let col_type: String = row.get(1)?;
            let dflt_value: Option<String> = row.get(3)?;
            
            Ok(super::ColumnMetadata {
                table_name: table_name.to_string(),
                column_name: col_name,
                original_type: col_type,
                constraints: dflt_value.map(|d| format!("DEFAULT {}", d)),
            })
        })?;
        
        for row in pragma_rows {
            result.push(row?);
        }
    }

    Ok(result)
}

/// Extract default expression from constraints string
/// 
/// Parses a constraints string like "NOT NULL DEFAULT 5" and extracts "5"
pub fn extract_default_from_constraints(constraints: &str) -> Option<String> {
    let upper = constraints.to_uppercase();
    if let Some(idx) = upper.find("DEFAULT") {
        let after_default = &constraints[idx + 7..].trim();
        // Take everything until the next constraint keyword
        let end_idx = after_default
            .find(|c: char| c == ',' || c == '(' || c == ')')
            .unwrap_or(after_default.len());
        let default_expr = after_default[..end_idx].trim();
        if !default_expr.is_empty() {
            return Some(default_expr.to_string());
        }
    }
    None
}
```

**Step 2: Implement MetadataProvider for SqliteHandler**

Modify `src/handler/mod.rs`:

```rust
// Add import at top
use crate::transpiler::metadata::{ColumnInfo, MetadataProvider};

// Add implementation after existing impl blocks
impl MetadataProvider for SqliteHandler {
    fn get_table_columns(&self, table_name: &str) -> Option<Vec<ColumnInfo>> {
        let conn = self.conn.lock().unwrap();
        
        match crate::catalog::get_table_columns_with_defaults(&conn, table_name) {
            Ok(metadata) => {
                let columns: Vec<ColumnInfo> = metadata
                    .into_iter()
                    .map(|m| {
                        let default_expr = m.constraints.as_ref()
                            .and_then(|c| crate::catalog::extract_default_from_constraints(c));
                        
                        ColumnInfo {
                            name: m.column_name,
                            original_type: m.original_type,
                            default_expr,
                            is_nullable: m.constraints.as_ref()
                                .map(|c| !c.to_uppercase().contains("NOT NULL"))
                                .unwrap_or(true),
                        }
                    })
                    .collect();
                
                if columns.is_empty() {
                    None
                } else {
                    Some(columns)
                }
            }
            Err(_) => None,
        }
    }
    
    fn get_column_default(&self, table_name: &str, column_name: &str) -> Option<String> {
        let conn = self.conn.lock().unwrap();
        
        match crate::catalog::get_column_metadata(&conn, table_name, column_name) {
            Ok(Some(metadata)) => {
                metadata.constraints
                    .and_then(|c| crate::catalog::extract_default_from_constraints(&c))
            }
            _ => None,
        }
    }
}
```

**Step 3: Verify compilation**

Run: `cargo check`
Expected: Clean compile with no errors

**Step 4: Commit**

```bash
git add src/catalog/table.rs src/handler/mod.rs
git commit -m "feat: implement MetadataProvider for SqliteHandler"
```

---

## Task 4: Update Handler to Use MetadataProvider in TranspileContext

**Files:**
- Modify: `src/handler/query.rs`

**Step 1: Update execute_query to pass MetadataProvider**

Modify `src/handler/query.rs` in the `execute_query` method:

```rust
// Find this line in execute_query:
let mut ctx = crate::transpiler::TranspileContext::with_functions(self.functions().clone());

// Replace with:
let mut ctx = crate::transpiler::TranspileContext::with_functions(self.functions().clone());
ctx.set_metadata_provider(Arc::new(self.clone()));
```

**Step 2: Verify compilation**

Run: `cargo check`
Expected: Clean compile with no errors

**Step 3: Commit**

```bash
git add src/handler/query.rs
git commit -m "feat: wire MetadataProvider into query execution"
```

---

## Task 5: Implement Automatic Padding in reconstruct_insert_stmt

**Files:**
- Modify: `src/transpiler/dml.rs`

**Step 1: Update reconstruct_insert_stmt to add implicit column list**

Modify `src/transpiler/dml.rs`:

```rust
/// Reconstruct an INSERT statement
pub(crate) fn reconstruct_insert_stmt(stmt: &InsertStmt, ctx: &mut TranspileContext) -> String {
    let mut parts = Vec::new();

    parts.push("insert into".to_string());

    // Table name
    let table_name = stmt
        .relation
        .as_ref()
        .map(|r| {
            let name = r.relname.to_lowercase();
            ctx.referenced_tables.push(name.clone());
            ctx.current_table = Some(name.clone()); // Set current table for DEFAULT resolution
            if r.schemaname.is_empty() || r.schemaname == "public" {
                name
            } else {
                format!("{}.{}.{}", r.schemaname.to_lowercase(), name, name)
            }
        })
        .unwrap_or_default();
    parts.push(table_name.clone());

    // Columns - if not specified, look up from metadata
    let columns: Vec<String>;
    if stmt.cols.is_empty() {
        // No column list specified - fetch from metadata
        if let Some(table_cols) = ctx.get_table_columns(&table_name) {
            columns = table_cols.iter().map(|c| c.name.clone()).collect();
            parts.push(format!("({})", columns.join(", ")));
        } else {
            // No metadata available - proceed without column list
            // SQLite will error if value count doesn't match
            columns = Vec::new();
        }
    } else {
        columns = stmt
            .cols
            .iter()
            .filter_map(|n| {
                if let Some(ref inner) = n.node {
                    if let NodeEnum::ResTarget(target) = inner {
                        return Some(target.name.to_lowercase());
                    }
                }
                None
            })
            .collect();
        parts.push(format!("({})", columns.join(", ")));
    }

    // Store column names in context for VALUES processing
    ctx.values_column_aliases = columns;

    // VALUES or SELECT
    if let Some(ref select_stmt) = stmt.select_stmt {
        let select_sql = reconstruct_node(select_stmt, ctx);
        parts.push(select_sql);
    }
    
    // Clear current table after processing
    ctx.current_table = None;
    ctx.values_column_aliases.clear();

    parts.join(" ")
}
```

**Step 2: Verify compilation**

Run: `cargo check`
Expected: Clean compile with no errors

**Step 3: Commit**

```bash
git add src/transpiler/dml.rs
git commit -m "feat: implement automatic column list padding in INSERT"
```

---

## Task 6: Implement DEFAULT Keyword Resolution

**Files:**
- Modify: `src/transpiler/expr.rs`

**Step 1: Update SetToDefault handling to resolve actual defaults**

Modify `src/transpiler/expr.rs` in `reconstruct_node`:

```rust
NodeEnum::SetToDefault(_) => {
    // Try to resolve the default value from metadata
    if let Some(ref table_name) = ctx.current_table {
        if let Some(ref col_aliases) = ctx.values_column_aliases.get(ctx.current_column_index) {
            if let Some(default_expr) = ctx.get_column_default(table_name, col_aliases) {
                // Transform PostgreSQL default expressions to SQLite equivalents
                let sqlite_default = transform_default_expression(&default_expr);
                return sqlite_default;
            }
        }
    }
    // Fallback to NULL if no default found
    "NULL".to_string()
}
```

**Step 2: Add transform_default_expression helper function**

Add to `src/transpiler/expr.rs`:

```rust
/// Transform PostgreSQL default expressions to SQLite equivalents
/// 
/// Handles common PostgreSQL default expressions like:
/// - now() -> datetime('now')
/// - current_timestamp -> datetime('now')
/// - nextval('seq') -> NULL (SQLite handles autoincrement separately)
fn transform_default_expression(expr: &str) -> String {
    let upper = expr.trim().to_uppercase();
    
    match upper.as_str() {
        "NOW()" | "CURRENT_TIMESTAMP" | "CURRENT_TIMESTAMP()" => {
            "datetime('now')".to_string()
        }
        "CURRENT_DATE" | "CURRENT_DATE()" => {
            "date('now')".to_string()
        }
        "CURRENT_TIME" | "CURRENT_TIME()" => {
            "time('now')".to_string()
        }
        "TRUE" => "1".to_string(),
        "FALSE" => "0".to_string(),
        _ => {
            // Check for nextval (sequence) - SQLite handles autoincrement differently
            if upper.starts_with("NEXTVAL") {
                "NULL".to_string()
            } else {
                // Pass through other expressions as-is
                expr.to_string()
            }
        }
    }
}
```

**Step 3: Update VALUES reconstruction to track column index**

Modify `reconstruct_values_stmt` in `src/transpiler/dml.rs`:

```rust
/// Reconstruct a VALUES statement (used in INSERT)
pub(crate) fn reconstruct_values_stmt(stmt: &SelectStmt, ctx: &mut TranspileContext) -> String {
    // Check if this VALUES has column aliases (via coldeflist in RangeSubselect)
    // If so, we need to convert to UNION ALL SELECT because SQLite doesn't support column aliases on VALUES
    if has_column_aliases(ctx) {
        return reconstruct_values_as_union_all(stmt, ctx);
    }

    let mut values_parts = Vec::new();

    for values_list in &stmt.values_lists {
        if let Some(ref inner) = values_list.node {
            if let NodeEnum::List(list) = inner {
                // Reset column index for each row
                ctx.current_column_index = 0;
                
                let values: Vec<String> = list
                    .items
                    .iter()
                    .map(|n| {
                        let val = reconstruct_node(n, ctx);
                        ctx.current_column_index += 1;
                        val
                    })
                    .collect();
                
                // Check if we need to pad this row with DEFAULTs
                let padded_values = pad_values_if_needed(values, ctx);
                
                values_parts.push(format!("({})", padded_values.join(", ")));
            }
        }
    }

    format!("values {}", values_parts.join(", "))
}

/// Pad VALUES list with DEFAULTs if needed to match column count
fn pad_values_if_needed(values: Vec<String>, ctx: &TranspileContext) -> Vec<String> {
    let expected_count = ctx.values_column_aliases.len();
    
    if expected_count == 0 || values.len() >= expected_count {
        return values;
    }
    
    let mut result = values;
    
    // Pad remaining columns with their default values
    for idx in result.len()..expected_count {
        if let Some(ref table_name) = ctx.current_table {
            if let Some(col_name) = ctx.values_column_aliases.get(idx) {
                if let Some(default_expr) = ctx.get_column_default(table_name, col_name) {
                    result.push(transform_default_expression(&default_expr));
                } else {
                    result.push("NULL".to_string());
                }
            } else {
                result.push("NULL".to_string());
            }
        } else {
            result.push("NULL".to_string());
        }
    }
    
    result
}

/// Transform PostgreSQL default expressions to SQLite equivalents
fn transform_default_expression(expr: &str) -> String {
    let upper = expr.trim().to_uppercase();
    
    match upper.as_str() {
        "NOW()" | "CURRENT_TIMESTAMP" | "CURRENT_TIMESTAMP()" => {
            "datetime('now')".to_string()
        }
        "CURRENT_DATE" | "CURRENT_DATE()" => {
            "date('now')".to_string()
        }
        "CURRENT_TIME" | "CURRENT_TIME()" => {
            "time('now')".to_string()
        }
        "TRUE" => "1".to_string(),
        "FALSE" => "0".to_string(),
        _ => {
            if upper.starts_with("NEXTVAL") {
                "NULL".to_string()
            } else {
                expr.to_string()
            }
        }
    }
}
```

**Step 4: Add necessary imports to dml.rs**

Add to `src/transpiler/dml.rs`:

```rust
use crate::transpiler::metadata::ColumnInfo;
```

**Step 5: Verify compilation**

Run: `cargo check`
Expected: Clean compile with no errors

**Step 6: Commit**

```bash
git add src/transpiler/dml.rs src/transpiler/expr.rs
git commit -m "feat: implement DEFAULT keyword resolution in INSERT VALUES"
```

---

## Task 7: Write Unit Tests for INSERT Padding

**Files:**
- Modify: `src/transpiler/mod.rs` (add tests at bottom)

**Step 1: Add unit tests for INSERT padding**

Add to the `#[cfg(test)]` module at the bottom of `src/transpiler/mod.rs`:

```rust
#[cfg(test)]
mod insert_tests {
    use super::*;
    use crate::transpiler::metadata::{ColumnInfo, MetadataProvider};
    use std::sync::Arc;

    struct MockMetadataProvider {
        columns: Vec<ColumnInfo>,
    }

    impl MockMetadataProvider {
        fn new(columns: Vec<ColumnInfo>) -> Self {
            Self { columns }
        }
    }

    impl MetadataProvider for MockMetadataProvider {
        fn get_table_columns(&self, _table_name: &str) -> Option<Vec<ColumnInfo>> {
            if self.columns.is_empty() {
                None
            } else {
                Some(self.columns.clone())
            }
        }

        fn get_column_default(&self, _table_name: &str, column_name: &str) -> Option<String> {
            self.columns.iter()
                .find(|c| c.name == column_name)
                .and_then(|c| c.default_expr.clone())
        }
    }

    #[test]
    fn test_insert_with_implicit_columns_adds_column_list() {
        let columns = vec![
            ColumnInfo { name: "id".to_string(), original_type: "INTEGER".to_string(), default_expr: None, is_nullable: false },
            ColumnInfo { name: "name".to_string(), original_type: "TEXT".to_string(), default_expr: None, is_nullable: true },
            ColumnInfo { name: "created_at".to_string(), original_type: "TEXT".to_string(), default_expr: Some("datetime('now')".to_string()), is_nullable: true },
        ];
        
        let provider = Arc::new(MockMetadataProvider::new(columns));
        let mut ctx = TranspileContext::with_metadata_provider(provider);
        
        let sql = "INSERT INTO users VALUES (1, 'Alice')";
        let result = transpile_with_context(sql, &mut ctx);
        
        // Should add explicit column list
        assert!(result.sql.contains("(id, name, created_at)"), "Should add column list: {}", result.sql);
        // Should pad with DEFAULT/NULL for missing value
        assert!(result.sql.contains("NULL") || result.sql.contains("datetime"), "Should pad missing value: {}", result.sql);
    }

    #[test]
    fn test_insert_with_explicit_columns_no_padding() {
        let columns = vec![
            ColumnInfo { name: "id".to_string(), original_type: "INTEGER".to_string(), default_expr: None, is_nullable: false },
            ColumnInfo { name: "name".to_string(), original_type: "TEXT".to_string(), default_expr: None, is_nullable: true },
        ];
        
        let provider = Arc::new(MockMetadataProvider::new(columns));
        let mut ctx = TranspileContext::with_metadata_provider(provider);
        
        let sql = "INSERT INTO users (id, name) VALUES (1, 'Alice')";
        let result = transpile_with_context(sql, &mut ctx);
        
        // Should not modify column list
        assert!(result.sql.contains("(id, name)"), "Should preserve explicit column list: {}", result.sql);
        assert!(!result.sql.contains("created_at"), "Should not add extra columns: {}", result.sql);
    }

    #[test]
    fn test_insert_default_keyword_resolution() {
        let columns = vec![
            ColumnInfo { name: "id".to_string(), original_type: "INTEGER".to_string(), default_expr: None, is_nullable: false },
            ColumnInfo { name: "status".to_string(), original_type: "TEXT".to_string(), default_expr: Some("'active'".to_string()), is_nullable: true },
        ];
        
        let provider = Arc::new(MockMetadataProvider::new(columns));
        let mut ctx = TranspileContext::with_metadata_provider(provider);
        
        let sql = "INSERT INTO users (id, status) VALUES (1, DEFAULT)";
        let result = transpile_with_context(sql, &mut ctx);
        
        // Should replace DEFAULT with the actual default value
        assert!(result.sql.contains("'active'"), "Should resolve DEFAULT to 'active': {}", result.sql);
        assert!(!result.sql.contains("DEFAULT"), "Should not contain DEFAULT keyword: {}", result.sql);
    }

    #[test]
    fn test_insert_default_now_transformation() {
        let columns = vec![
            ColumnInfo { name: "id".to_string(), original_type: "INTEGER".to_string(), default_expr: None, is_nullable: false },
            ColumnInfo { name: "created_at".to_string(), original_type: "TIMESTAMP".to_string(), default_expr: Some("now()".to_string()), is_nullable: true },
        ];
        
        let provider = Arc::new(MockMetadataProvider::new(columns));
        let mut ctx = TranspileContext::with_metadata_provider(provider);
        
        let sql = "INSERT INTO users VALUES (1, DEFAULT)";
        let result = transpile_with_context(sql, &mut ctx);
        
        // Should transform now() to SQLite equivalent
        assert!(result.sql.contains("datetime('now')"), "Should transform now() to datetime('now'): {}", result.sql);
    }

    #[test]
    fn test_insert_no_metadata_no_padding() {
        // No metadata provider - should pass through as-is
        let mut ctx = TranspileContext::new();
        
        let sql = "INSERT INTO users VALUES (1, 'Alice')";
        let result = transpile_with_context(sql, &mut ctx);
        
        // Should not add column list without metadata
        assert!(!result.sql.contains("(id"), "Should not add column list without metadata: {}", result.sql);
    }
}
```

**Step 2: Run unit tests**

Run: `cargo test insert_tests --lib`
Expected: All tests pass

**Step 3: Commit**

```bash
git add src/transpiler/mod.rs
git commit -m "test: add unit tests for INSERT padding and DEFAULT resolution"
```

---

## Task 8: Write Integration Tests

**Files:**
- Create: `tests/insert_tests.rs`

**Step 1: Create integration test file**

Create `tests/insert_tests.rs`:

```rust
//! Integration tests for INSERT logic improvements
//!
//! Tests automatic padding and DEFAULT keyword support.

use pgqt::transpiler::{transpile_with_metadata, TranspileContext};
use pgqt::transpiler::metadata::{ColumnInfo, MetadataProvider};
use std::sync::Arc;

struct MockMetadataProvider {
    columns: Vec<ColumnInfo>,
}

impl MockMetadataProvider {
    fn new(columns: Vec<ColumnInfo>) -> Self {
        Self { columns }
    }
}

impl MetadataProvider for MockMetadataProvider {
    fn get_table_columns(&self, _table_name: &str) -> Option<Vec<ColumnInfo>> {
        if self.columns.is_empty() {
            None
        } else {
            Some(self.columns.clone())
        }
    }

    fn get_column_default(&self, _table_name: &str, column_name: &str) -> Option<String> {
        self.columns.iter()
            .find(|c| c.name == column_name)
            .and_then(|c| c.default_expr.clone())
    }
}

#[test]
fn test_insert_implicit_columns_gets_padded() {
    let columns = vec![
        ColumnInfo { name: "id".to_string(), original_type: "INTEGER".to_string(), default_expr: None, is_nullable: false },
        ColumnInfo { name: "name".to_string(), original_type: "TEXT".to_string(), default_expr: None, is_nullable: true },
        ColumnInfo { name: "email".to_string(), original_type: "TEXT".to_string(), default_expr: None, is_nullable: true },
    ];
    
    let provider = Arc::new(MockMetadataProvider::new(columns));
    let mut ctx = TranspileContext::with_metadata_provider(provider);
    
    let sql = "INSERT INTO users VALUES (1, 'Alice')";
    let result = pgqt::transpiler::transpile_with_context(sql, &mut ctx);
    
    println!("Transpiled SQL: {}", result.sql);
    
    // Should have added explicit column list
    assert!(result.sql.contains("(id, name, email)"), "Should add column list");
    // Should have padded with NULL for missing email
    assert!(result.sql.contains("NULL"), "Should pad with NULL");
}

#[test]
fn test_insert_multiple_rows_with_different_defaults() {
    let columns = vec![
        ColumnInfo { name: "id".to_string(), original_type: "INTEGER".to_string(), default_expr: None, is_nullable: false },
        ColumnInfo { name: "status".to_string(), original_type: "TEXT".to_string(), default_expr: Some("'pending'".to_string()), is_nullable: true },
    ];
    
    let provider = Arc::new(MockMetadataProvider::new(columns));
    let mut ctx = TranspileContext::with_metadata_provider(provider);
    
    let sql = "INSERT INTO users VALUES (1, DEFAULT), (2, 'active')";
    let result = pgqt::transpiler::transpile_with_context(sql, &mut ctx);
    
    println!("Transpiled SQL: {}", result.sql);
    
    // Both rows should have explicit values
    assert!(result.sql.contains("'pending'"), "Should resolve first row DEFAULT");
    assert!(result.sql.contains("'active'"), "Should preserve second row value");
}

#[test]
fn test_insert_timestamp_default() {
    let columns = vec![
        ColumnInfo { name: "id".to_string(), original_type: "INTEGER".to_string(), default_expr: None, is_nullable: false },
        ColumnInfo { name: "created_at".to_string(), original_type: "TIMESTAMP".to_string(), default_expr: Some("now()".to_string()), is_nullable: true },
    ];
    
    let provider = Arc::new(MockMetadataProvider::new(columns));
    let mut ctx = TranspileContext::with_metadata_provider(provider);
    
    let sql = "INSERT INTO events (id) VALUES (1)";
    let result = pgqt::transpiler::transpile_with_context(sql, &mut ctx);
    
    println!("Transpiled SQL: {}", result.sql);
    
    // Should not have DEFAULT - SQLite will use its own default
    assert!(!result.sql.to_uppercase().contains("DEFAULT"), "Should not contain DEFAULT keyword");
}

#[test]
fn test_insert_boolean_default() {
    let columns = vec![
        ColumnInfo { name: "id".to_string(), original_type: "INTEGER".to_string(), default_expr: None, is_nullable: false },
        ColumnInfo { name: "is_active".to_string(), original_type: "BOOLEAN".to_string(), default_expr: Some("true".to_string()), is_nullable: true },
    ];
    
    let provider = Arc::new(MockMetadataProvider::new(columns));
    let mut ctx = TranspileContext::with_metadata_provider(provider);
    
    let sql = "INSERT INTO users VALUES (1, DEFAULT)";
    let result = pgqt::transpiler::transpile_with_context(sql, &mut ctx);
    
    println!("Transpiled SQL: {}", result.sql);
    
    // Should transform true to 1 for SQLite
    assert!(result.sql.contains("1") || result.sql.contains("'true'"), "Should resolve boolean default");
}
```

**Step 2: Run integration tests**

Run: `cargo test --test insert_tests`
Expected: All tests pass

**Step 3: Commit**

```bash
git add tests/insert_tests.rs
git commit -m "test: add integration tests for INSERT logic improvements"
```

---

## Task 9: Write E2E Test

**Files:**
- Create: `tests/insert_e2e_test.py`

**Step 1: Create E2E test file**

Create `tests/insert_e2e_test.py`:

```python
#!/usr/bin/env python3
"""
End-to-end tests for INSERT logic improvements.
Tests automatic padding and DEFAULT keyword support through wire protocol.
"""
import subprocess
import time
import psycopg2
import os
import sys
import signal

PROXY_HOST = "127.0.0.1"
PROXY_PORT = 5434
DB_PATH = "/tmp/test_insert_e2e.db"

def start_proxy():
    """Start the pgqt proxy server."""
    # Clean up any existing database
    if os.path.exists(DB_PATH):
        os.remove(DB_PATH)
    
    proc = subprocess.Popen(
        ["./target/release/pgqt", "--port", str(PROXY_PORT), "--database", DB_PATH],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    # Wait for server to start
    time.sleep(2)
    return proc

def stop_proxy(proc):
    """Stop the proxy server."""
    proc.send_signal(signal.SIGTERM)
    proc.wait()
    # Clean up
    if os.path.exists(DB_PATH):
        os.remove(DB_PATH)

def test_insert_with_implicit_columns():
    """Test that INSERT with fewer values than columns works."""
    proc = start_proxy()
    try:
        conn = psycopg2.connect(
            host=PROXY_HOST,
            port=PROXY_PORT,
            database="postgres",
            user="postgres",
            password="postgres"
        )
        cur = conn.cursor()
        
        # Create table with multiple columns
        cur.execute("""
            CREATE TABLE users (
                id SERIAL PRIMARY KEY,
                name TEXT NOT NULL,
                email TEXT,
                created_at TIMESTAMP DEFAULT now()
            )
        """)
        conn.commit()
        
        # Insert with fewer values than columns (PostgreSQL style)
        cur.execute("INSERT INTO users VALUES (1, 'Alice')")
        conn.commit()
        
        # Verify the row was inserted with defaults
        cur.execute("SELECT id, name, email, created_at FROM users WHERE id = 1")
        row = cur.fetchone()
        
        assert row[0] == 1, f"Expected id=1, got {row[0]}"
        assert row[1] == 'Alice', f"Expected name='Alice', got {row[1]}"
        # email and created_at should have been filled with defaults
        
        cur.close()
        conn.close()
        print("test_insert_with_implicit_columns: PASSED")
    finally:
        stop_proxy(proc)

def test_insert_with_default_keyword():
    """Test that DEFAULT keyword in VALUES works."""
    proc = start_proxy()
    try:
        conn = psycopg2.connect(
            host=PROXY_HOST,
            port=PROXY_PORT,
            database="postgres",
            user="postgres",
            password="postgres"
        )
        cur = conn.cursor()
        
        # Create table with defaults
        cur.execute("""
            CREATE TABLE items (
                id SERIAL PRIMARY KEY,
                name TEXT NOT NULL,
                status TEXT DEFAULT 'pending',
                count INTEGER DEFAULT 0
            )
        """)
        conn.commit()
        
        # Insert with DEFAULT keyword
        cur.execute("INSERT INTO items (id, name, status, count) VALUES (1, 'Item1', DEFAULT, DEFAULT)")
        conn.commit()
        
        # Verify defaults were applied
        cur.execute("SELECT id, name, status, count FROM items WHERE id = 1")
        row = cur.fetchone()
        
        assert row[0] == 1, f"Expected id=1, got {row[0]}"
        assert row[1] == 'Item1', f"Expected name='Item1', got {row[1]}"
        assert row[2] == 'pending', f"Expected status='pending', got {row[2]}"
        assert row[3] == 0, f"Expected count=0, got {row[3]}"
        
        cur.close()
        conn.close()
        print("test_insert_with_default_keyword: PASSED")
    finally:
        stop_proxy(proc)

def test_insert_mixed_explicit_and_default():
    """Test mixing explicit values and DEFAULT in same INSERT."""
    proc = start_proxy()
    try:
        conn = psycopg2.connect(
            host=PROXY_HOST,
            port=PROXY_PORT,
            database="postgres",
            user="postgres",
            password="postgres"
        )
        cur = conn.cursor()
        
        cur.execute("""
            CREATE TABLE products (
                id SERIAL PRIMARY KEY,
                name TEXT NOT NULL,
                price REAL DEFAULT 0.0,
                in_stock BOOLEAN DEFAULT true
            )
        """)
        conn.commit()
        
        # Mix explicit and DEFAULT values
        cur.execute("INSERT INTO products (id, name, price, in_stock) VALUES (1, 'Product1', 9.99, DEFAULT)")
        conn.commit()
        
        cur.execute("SELECT id, name, price, in_stock FROM products WHERE id = 1")
        row = cur.fetchone()
        
        assert row[0] == 1, f"Expected id=1, got {row[0]}"
        assert row[1] == 'Product1', f"Expected name='Product1', got {row[1]}"
        assert row[2] == 9.99, f"Expected price=9.99, got {row[2]}"
        assert row[3] == True, f"Expected in_stock=true, got {row[3]}"
        
        cur.close()
        conn.close()
        print("test_insert_mixed_explicit_and_default: PASSED")
    finally:
        stop_proxy(proc)

if __name__ == "__main__":
    # Build release binary first
    print("Building release binary...")
    result = subprocess.run(["cargo", "build", "--release"], capture_output=True, text=True)
    if result.returncode != 0:
        print(f"Build failed: {result.stderr}")
        sys.exit(1)
    
    test_insert_with_implicit_columns()
    test_insert_with_default_keyword()
    test_insert_mixed_explicit_and_default()
    print("\nAll E2E tests PASSED!")
```

**Step 2: Make the test executable and run**

Run:
```bash
chmod +x tests/insert_e2e_test.py
python3 tests/insert_e2e_test.py
```

Expected: All tests pass (may need to fix issues if found)

**Step 3: Commit**

```bash
git add tests/insert_e2e_test.py
git commit -m "test: add E2E tests for INSERT logic improvements"
```

---

## Task 10: Run Full Test Suite

**Files:**
- Run: `./run_tests.sh`

**Step 1: Run the full test suite**

Run: `./run_tests.sh`

Expected: All tests pass (or only pre-existing failures)

**Step 2: Commit any fixes**

If any fixes were needed:
```bash
git add -A
git commit -m "fix: address test failures from INSERT logic improvements"
```

---

## Task 11: Update Documentation

**Files:**
- Modify: `COMPATIBILITY_STATUS.md`

**Step 1: Update compatibility status**

Modify `COMPATIBILITY_STATUS.md` to mark INSERT improvements as complete:

```markdown
### 1. INSERT Logic Improvements ✅ COMPLETE
- **Automatic Padding**: ✅ Detects when an `INSERT` has fewer values than the table has columns. Fetches the schema from the catalog and pads with `DEFAULT` or `NULL` to match Postgres behavior.
- **DEFAULT Keyword**: ✅ Improves support for the `DEFAULT` keyword in `VALUES` lists, ensuring it maps to the correct SQLite behavior or omitted columns.
```

**Step 2: Commit**

```bash
git add COMPATIBILITY_STATUS.md
git commit -m "docs: mark INSERT logic improvements as complete"
```

---

## Summary

This implementation plan adds:

1. **MetadataProvider trait** for schema lookups during transpilation
2. **Automatic column list generation** when INSERT omits column list
3. **Automatic padding** of VALUES rows to match column count
4. **DEFAULT keyword resolution** to actual default expressions
5. **Comprehensive tests** at unit, integration, and E2E levels

The key architectural change is introducing the `MetadataProvider` trait that bridges the stateless transpiler with the database catalog, enabling context-aware SQL transformations.
