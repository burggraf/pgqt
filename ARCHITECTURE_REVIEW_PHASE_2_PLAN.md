# Phase 2 Implementation Plan: Result Set & Metadata Formalization

This document outlines the detailed steps for Phase 2 of the PGQT architecture review implementation.

## 1. Create `src/handler/rewriter.rs`

The goal is to centralize the logic for transforming SQLite result sets into PostgreSQL-compatible responses, specifically focusing on column names and type OIDs.

### Tasks
- [ ] Define `ResultSetRewriter` struct.
- [ ] Implement `rewrite_field_info`:
    ```rust
    pub fn rewrite_field_info(
        &self, 
        sqlite_stmt: &rusqlite::Statement,
        referenced_tables: &[String],
        metadata_provider: Arc<dyn MetadataProvider>
    ) -> Vec<FieldInfo>;
    ```
- [ ] Logic for `rewrite_field_info`:
    - For each column in the SQLite statement:
        1. Get the column name from SQLite.
        2. If it's a simple column (not an expression), look up its original PostgreSQL type in the shadow catalog via `metadata_provider`.
        3. Map the original type (e.g., `VARCHAR`, `INT4`, `TIMESTAMP`) to a `pgwire::api::Type`.
        4. If it's an expression (like `COUNT(*)`) or anonymous, use PostgreSQL's `?column?` naming convention or a inferred type.
- [ ] Implement `encode_row`:
    - Centralize the conversion of SQLite `Value` to PostgreSQL wire format (Text or Binary).

## 2. Refactor `src/handler/query.rs`

Update `execute_select` to use the new rewriter.

### Tasks
- [ ] Modify `execute_select` to:
    1. Call `rewriter.rewrite_field_info(...)` after preparing the statement but before execution.
    2. Use the returned `FieldInfo` to initialize the `DataRowEncoder`.
- [ ] Clean up `src/handler/query.rs`:
    - Remove the hardcoded `Type::TEXT` mapping.
    - Remove the inline anonymous column naming logic.

## 3. Consolidate Naming Logic

Move "hinting" logic from the transpiler to the handler/rewriter where possible.

### Tasks
- [ ] **`src/transpiler/dml.rs`**:
    - Evaluate if we can remove the `AS "int4"` etc. type casts that were added to "trick" the client into seeing the right type.
    - Ideally, the transpiler should generate clean SQLite SQL, and the `ResultSetRewriter` should handle the type OID metadata in the `pgwire` response.
- [ ] **Metadata Provider Enhancement**:
    - Update `MetadataProvider` to return OIDs if available, or a mapping from original type strings to OIDs.

## 4. Verification & Regression

- [ ] **Unit Tests**:
    - Test `ResultSetRewriter` in isolation with mocked `MetadataProvider`.
    - Verify that a column named `id` in table `users` (which is `SERIAL` in PG) gets the `INT4` (or `INT8`) OID in `FieldInfo`.
- [ ] **Integration Tests**:
    - `tests/schema_tests.rs`: Ensure that `SELECT *` from a table created with complex types returns the correct OIDs.
    - Use `psycopg2` in an E2E test to inspect `cursor.description` and verify type OIDs match real PostgreSQL.
- [ ] **Compatibility**:
    - Run `postgres-compatibility-suite/run_suite.sh`.
    - *Success Metric*: Passing rate for "Section 2 (Column Metadata & Naming)" increases.
    - Specifically check tests that verify `SELECT 1` returns a column named `?column?` or similar PG conventions.
