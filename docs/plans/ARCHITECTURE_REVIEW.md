# PGQT Architectural Analysis and Refactoring Report

This report provides a thorough analysis of the PGQT codebase, focusing on architectural changes and refactorings to improve PostgreSQL compatibility and long-term maintainability.

---

## 1. Executive Summary

PGQT's current architecture—a transpilation layer backed by a "shadow catalog" in SQLite—is a sound approach for bridging PostgreSQL and SQLite. However, as the project scales to meet more complex PostgreSQL features (currently at ~28% passing rate in the compatibility suite), several structural bottlenecks have emerged. 

The most critical areas for improvement are:
1.  **Transpiler Modularization**: Reducing the complexity of the core AST walking logic.
2.  **Data-Driven Mapping**: Moving away from hardcoded type/function mappings toward a unified registry.
3.  **Transactional Integrity**: Implementing real transaction mapping instead of NO-OPs.
4.  **Error System Emulation**: Mapping SQLite errors to PostgreSQL `SQLSTATE` codes.
5.  **Result Set Rewriting**: Formalizing the logic for column naming and metadata aliasing.

---

## 2. Detailed Architectural Analysis

### 2.1 Transpiler Bottlenecks (`src/transpiler/`)
The current transpiler uses a monolithic AST walking approach. 
- **`expr.rs` Complexity**: `reconstruct_node` is nearly 1,000 lines long and handles everything from basic literals to complex geometric and range types. This makes it difficult to add new types without risking regressions.
- **Hardcoded Mappings**: `utils.rs` (types) and `func.rs` (functions) rely on large `match` statements. This manual mapping is error-prone and doesn't scale well to PostgreSQL's vast function library.
- **Limited Contextual Awareness**: While `TranspileContext` exists, it lacks the ability to resolve expression types dynamically. This is why features like "Array vs. Range Operator Detection" require brittle string-matching (e.g., checking if a string starts with `[` or `(`).

### 2.2 Transaction Management (`src/handler/transaction.rs`)
The current implementation treats `BEGIN`, `COMMIT`, and `ROLLBACK` as NO-OPs.
- **Risk**: PostgreSQL clients expect transactional atomicity. If a client wraps multiple `INSERT`s in a `BEGIN`/`COMMIT` block and one fails, the client expects the previous ones to roll back. Currently, PGQT will leave the SQLite database in a partially updated state.
- **Incompatibility**: Clients using `SAVEPOINT` or specific isolation levels (e.g., `SERIALIZABLE`) will receive an "OK" but get "READ COMMITTED" (or SQLite's default) behavior, leading to silent data integrity issues.

### 2.3 Shadow Catalog Synchronization (`src/catalog/`)
The `__pg_catalog__` tables are essential for transpilation, but their synchronization with the actual SQLite schema is a potential weak point.
- **Manual Store**: If a `CREATE TABLE` is transpiled but the execution fails, the metadata might still be stored (or vice versa), leading to a split-brain state between the proxy's metadata and the actual database.

### 2.4 Error Handling & SQLSTATE
PostgreSQL clients rely heavily on `SQLSTATE` codes (e.g., `23505` for unique violations). SQLite provides its own set of error codes.
- **Current State**: There is no centralized mapping layer to convert SQLite results into PostgreSQL-compatible error responses.

---

## 3. Recommended Architectural Changes

### 3.1 Refactor: Modular Expression Handlers
**Action**: Split `src/transpiler/expr.rs` into specialized sub-modules:
- `expr/geometric.rs`: Handle `point`, `box`, `circle`, etc.
- `expr/arrays.rs`: Handle array literals and operators.
- `expr/ranges.rs`: Handle range literals and operators.
- `expr/jsonb.rs`: Handle JSONB operators.

**Benefit**: Increases "velocity" by allowing developers to work on specific feature sets without touching the core `reconstruct_node` loop.

### 3.2 Feature: Unified Type & Function Registry
**Action**: Move mappings out of `match` statements and into a data-driven registry (potentially initialized from the Catalog).
- Define a `TypeMapping` struct: `(PG_Type, SQLite_Type, Cast_Logic, Alias_Logic)`.
- Define a `FunctionMapping` struct: `(PG_Function, SQLite_Equivalent, Transformation_Closure)`.

**Benefit**: Simplifies the addition of "stubs" and polyfills. It also allows the `MetadataProvider` to supply type information during the "Reconstruct" phase of transpilation.

### 3.3 Architecture: Real Transaction Mapping
**Action**: Implement a proper transaction state machine in `SqliteHandler`.
- Map `BEGIN` to SQLite's `BEGIN TRANSACTION`.
- Map `SAVEPOINT` to SQLite's `SAVEPOINT`.
- Track the "In Transaction" state to ensure clean-up on connection loss.

**Benefit**: Prevents silent data loss and meets client expectations for atomicity.

### 3.4 Architecture: Result Set Rewriter
**Action**: Introduce a formal layer that executes *after* the SQLite query returns but *before* the results are sent to `pgwire`.
- **Column Naming**: Automatically rename `?column?` to match PostgreSQL's expected naming for expressions (the current work-in-progress).
- **Type Metadata**: Force the `pgwire` field types based on the original PostgreSQL types stored in the shadow catalog, rather than relying on SQLite's dynamic typing.

### 3.5 Refactor: Error Mapping Layer
**Action**: Create a `src/handler/errors.rs` module that wraps `rusqlite::Error`.
- Map `SQLITE_CONSTRAINT_UNIQUE` to `23505`.
- Map `SQLITE_CONSTRAINT_NOTNULL` to `23502`.

**Benefit**: Allows sophisticated clients (like ORMs or migration tools) to handle errors programmatically.

---

## 4. Maintenance & Compatibility Roadmap

| Phase | Focus | Impact |
| :--- | :--- | :--- |
| **Short Term** | **Column Naming & Anonymous results** | Fixes immediate "Column Metadata" failures. |
| **Short Term** | **Error Mapping Layer** | Improves client library stability (Psycopg2, JDBC). |
| **Mid Term** | **Transpiler Modularization** | Increases speed of adding FTS, Geo, and Range features. |
| **Mid Term** | **Real Transaction Support** | Ensures data integrity for complex applications. |
| **Long Term** | **Dynamic Type Resolution** | Allows the transpiler to handle nested expressions correctly. |

## 5. Conclusion

The current codebase is functional but "heavy" in its core transpilation loop. By shifting from a hardcoded, monolithic design to a modular, data-driven architecture, you will significantly reduce the time required to address the remaining 35 failing files in the compatibility suite. 

The current work on **Column Metadata** is a perfect candidate for the "Result Set Rewriter" pattern—formalizing it there will make it easier to maintain than adding more logic to the already large `dml.rs`.
