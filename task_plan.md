# Implementation Plan: PostgreSQL Range Types Support

## Overview
Implement PostgreSQL-compatible Range Types (`int4range`, `int8range`, `numrange`, `tsrange`, `tstzrange`, `daterange`) by emulating them as strings (TEXT) in SQLite. Support canonicalization for discrete types, common operators, and metadata functions.

## Tasks

### Phase 1: Core Logic (`src/range.rs`)
- [ ] Define `RangeValue` enum and `RangeBound` struct.
- [ ] Implement parser for PG range string format: `[low,high)`, `empty`, `(,high]`, etc.
- [ ] Implement canonicalization for discrete types (`int4range`, `int8range`, `daterange`):
    - `[low, high]` -> `[low, high+1)`
    - `(low, high)` -> `[low+1, high)`
    - Handle `empty` cases (e.g., `[10, 10)` or `[10, 9]`).
- [ ] Implement range operations:
    - `contains` (`@>`)
    - `contained_by` (`<@`)
    - `overlaps` (`&&`)
    - `strictly_left` (`<<`)
    - `strictly_right` (`>>`)
    - `adjacent` (`-|-`)
    - `no_extend_right` (`&<`)
    - `no_extend_left` (`&>`)
    - `union` (`+`) - if overlapping/adjacent
    - `intersection` (`*`)
    - `difference` (`-`)
- [ ] Implement metadata functions: `lower`, `upper`, `lower_inc`, `upper_inc`, `lower_inf`, `upper_inf`, `isempty`.
- [ ] Add extensive unit tests for all logic.

### Phase 2: Transpilation (`src/transpiler.rs`)
- [ ] Update `rewrite_type_for_sqlite` to map range types to `text`.
- [ ] Implement operator transpilation in `reconstruct_a_expr`:
    - `@>` -> `range_contains(l, r)`
    - `<@` -> `range_contained(l, r)`
    - `&&` -> `range_overlaps(l, r)`
    - `<<` -> `range_left(l, r)`
    - `>>` -> `range_right(l, r)`
    - `-|-` -> `range_adjacent(l, r)`
    - `&<` -> `range_no_extend_right(l, r)`
    - `&>` -> `range_no_extend_left(l, r)`
- [ ] Implement constructor functions: `int4range(...)`, `daterange(...)`, etc.

### Phase 3: Runtime Integration (`src/main.rs`)
- [ ] Register SQLite scalar functions:
    - `range_contains`, `range_contained`, `range_overlaps`, etc.
    - `lower`, `upper`, `lower_inc`, `upper_inc`, `lower_inf`, `upper_inf`, `isempty`, `range_merge`.
- [ ] Register constructor functions as SQLite functions.

### Phase 4: Testing & Documentation
- [ ] Create `tests/range_e2e_test.py` for full integration testing.
- [ ] Update `docs/TODO-FEATURES.md`.
- [ ] Create `docs/RANGE.md` with implementation details.
- [ ] Update `README.md`.

### Phase 5: Finalization
- [ ] Run all tests.
- [ ] Commit and push changes.

## Research Findings
- Discrete types: `int4range`, `int8range`, `daterange`.
- Continuous types: `numrange`, `tsrange`, `tstzrange`.
- Canonical form: `[low, high)` for discrete types.
- Unbounded: `(,)` or `(low,)` or `(,high)`.
- Empty: `empty` or any range where `low >= high` (after normalization).
- GiST indexes: Not applicable in SQLite directly, but we can use standard indexes on the TEXT column for equality, or virtual tables/R-trees for range queries if needed later. For now, simple emulated operators are enough.
