# Phase 3 Implementation Plan: Transpiler Structural Refactoring (Modularization)

This document outlines the detailed steps for Phase 3 of the PGQT architecture review implementation.

## 1. Initialize Modular Structure

The goal is to transition from a single monolithic file `src/transpiler/expr.rs` to a directory-based module `src/transpiler/expr/`.

### Tasks
- [ ] Create the directory `src/transpiler/expr/`.
- [ ] Move the current contents of `src/transpiler/expr.rs` to `src/transpiler/expr/mod.rs`.
- [ ] Update `src/transpiler/mod.rs` to reference the new structure (no change to the `mod expr;` line is usually needed, but the compiler must find the new path).
- [ ] Verify that the project still builds with `cargo check`.

## 2. Incremental Extraction

Systematically move domain-specific logic into its own sub-module.

### 2.1 Extract Array Logic (`src/transpiler/expr/arrays.rs`)
- [ ] Move `reconstruct_array_expr` and `reconstruct_a_array_expr` into `expr/arrays.rs`.
- [ ] Identify array-specific operator logic in `reconstruct_a_expr` (e.g., `&&`, `@>`, `<@` when used with arrays) and move to a helper in `expr/arrays.rs`.
- [ ] Expose these functions to `mod.rs`.

### 2.2 Extract Range Logic (`src/transpiler/expr/ranges.rs`)
- [ ] Move `reconstruct_range_function` into `expr/ranges.rs`.
- [ ] Extract range operator logic from `reconstruct_a_expr` (overlapping `&&`, contains `@>`, etc.) and range canonicalization logic from `reconstruct_aconst`.
- [ ] Expose these functions to `mod.rs`.

### 2.3 Extract Geometric Logic (`src/transpiler/expr/geo.rs`)
- [ ] Extract point, box, circle, and path literal parsing logic from `reconstruct_aconst`.
- [ ] Move geometric-specific operators (e.g., `<->` distance operator) into `expr/geo.rs`.

### 2.4 Extract Common Utilities (`src/transpiler/expr/utils.rs`)
- [ ] Move generic helpers like `reconstruct_aconst`, `reconstruct_column_ref`, and `reconstruct_type_cast` if they are shared across modules to prevent circular dependencies.

## 3. Refactor `mod.rs` (the main loop)

- [ ] Update the `match` statement in `reconstruct_node` to call the sub-module functions.
- [ ] Clean up imports in all new files.

## 4. Verification & Regression

- [ ] **Static Analysis**: `cargo check` and `cargo clippy`.
- [ ] **Functionality Identity Test**: 
    - Before refactoring, capture the output of `cargo run -- --transpile` for a suite of complex queries (provided in `tests/fixtures/complex_queries.sql`).
    - After refactoring, ensure the output is identical.
- [ ] **Integration Tests**: 
    - `cargo test --test array_tests`
    - `cargo test --test range_tests`
    - `python3 tests/geo_e2e_test.py`
- [ ] **Full Suite**: Run `./run_tests.sh`.

## Success Metric
- `src/transpiler/expr/mod.rs` is reduced to under 400 lines (down from ~1000).
- Logic is logically grouped, making it easier to add support for new operators without cluttering the main expression walker.
