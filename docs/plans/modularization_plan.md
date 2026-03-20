# PGQT Modularization Plan

This document outlines the steps to modularize the PGQT codebase, focusing on reducing the size of the largest files (`src/transpiler.rs`, `src/main.rs`, and `src/catalog.rs`) to improve maintainability and testability.

## Current State Assessment (approx. lines)
- `src/transpiler.rs`: 4,376 lines (Critical)
- `src/main.rs`: 2,524 lines (High)
- `src/catalog.rs`: 1,636 lines (Medium)
- `src/array.rs`: 1,226 lines (Low)

## Phase 1: Modularizing `src/transpiler.rs`
**Goal**: Transform `src/transpiler.rs` into a module directory `src/transpiler/`.

### 1.1 Create Directory Structure
- `src/transpiler/mod.rs`: Public API and re-exports.
- `src/transpiler/context.rs`: `TranspileContext`, `TranspileResult`, `OperationType`, and `ColumnTypeInfo`.
- `src/transpiler/ddl.rs`: `CREATE`, `ALTER`, `DROP`, `TRUNCATE`, `INDEX` reconstruction logic.
- `src/transpiler/dml.rs`: `SELECT`, `INSERT`, `UPDATE`, `DELETE` and basic SQL node reconstruction.
- `src/transpiler/expr.rs`: `AExpr`, `BoolExpr`, `TypeCast`, `CaseExpr`, and operator handling.
- `src/transpiler/rls.rs`: RLS-specific query augmentation and reconstruction.
- `src/transpiler/func.rs`: `FuncCall` reconstruction and `CREATE FUNCTION` parsing.
- `src/transpiler/utils.rs`: Shared utilities like `rewrite_type_for_sqlite`.

### 1.2 Execution Steps
1. Create the `src/transpiler/` directory.
2. Extract types to `context.rs` and `utils.rs`.
3. Incrementally move function groups (DDL -> DML -> Expr -> RLS -> Func).
4. Update imports in each new file.
5. Replace `src/transpiler.rs` with `src/transpiler/mod.rs`.

---

## Phase 2: Modularizing `src/main.rs`
**Goal**: Extract the `SqliteHandler` implementation (approx. 2,000 lines) into a dedicated module.

### 2.1 Create Directory Structure
- `src/handler/mod.rs`: `SqliteHandler` struct definition and `PgWireServerHandlers` / `SimpleQueryHandler` trait implementations.
- `src/handler/query.rs`: Core query execution methods (`execute_query`, `execute_select`, `execute_statement`).
- `src/handler/schema.rs`: Schema management handlers (`handle_create_schema`, `handle_drop_schema`).
- `src/handler/function.rs`: Function management handlers (`handle_create_function`, `handle_drop_function`).
- `src/handler/copy.rs`: `handle_copy_statement` and related logic.
- `src/handler/rls.rs`: `apply_rls_to_query` and permission checking.

### 2.2 Execution Steps
1. Create `src/handler/` directory.
2. Move `SqliteHandler` and its dependencies from `main.rs` to `handler/mod.rs`.
3. Extract specific handler logic into sub-modules.
4. Update `main.rs` to use the new `handler` module.

---

## Phase 3: Modularizing `src/catalog.rs`
**Goal**: Split catalog management by functional area.

### 3.1 Create Directory Structure
- `src/catalog/mod.rs`: Public types and re-exports.
- `src/catalog/init.rs`: `init_catalog` and `init_pg_types`.
- `src/catalog/table.rs`: Table and column metadata storage/retrieval.
- `src/catalog/function.rs`: Function metadata storage/retrieval.
- `src/catalog/rls.rs`: RLS policy storage and retrieval.
- `src/catalog/system_views.rs`: `init_system_views` and large SQL strings.

---

## Verification & Safety
1. **Cargo Check**: Run `cargo check` after every file move to ensure visibility (`pub(crate)`) and imports are correct.
2. **Unit Tests**: Run `cargo test` (ignoring PL/pgSQL) after each phase.
3. **E2E Tests**: Run `./run_tests.sh` after each phase to ensure the wire protocol behavior remains identical.
4. **No Logic Changes**: This is a pure refactor. No functional logic should be altered during this process.

---

## Timeline
- **Phase 1**: 2-3 hours
- **Phase 2**: 2 hours
- **Phase 3**: 1 hour
