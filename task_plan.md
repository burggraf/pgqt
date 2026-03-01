# Implementation Plan: Geometric Types for PostgreSQL-to-SQLite Proxy

## Overview
Implement support for PostgreSQL's geometric data types: `point`, `line`, `lseg`, `box`, `path`, `polygon`, and `circle`. These types will be stored as `TEXT` in SQLite using PostgreSQL's canonical string representation. Operators will be transpiled to custom SQLite functions implemented in Rust.

## 1. Storage Strategy
- All geometric types will be stored as `TEXT` in SQLite.
- The format will match PostgreSQL's output format (canonical strings).
- Type mapping in `src/transpiler.rs` will be updated to map these types to `TEXT`.

## 2. Core Logic (`src/geo.rs`)
Create a new module `src/geo.rs` to handle:
- Parsing of geometric string formats.
- Implementation of geometric operators as Rust functions.
- Structs representing each geometric type:
    - `Point { x: f64, y: f64 }`
    - `Line { a: f64, b: f64, c: f64 }`
    - `Lseg { p1: Point, p2: Point }`
    - `Box { p1: Point, p2: Point }` (reordered to UR, LL)
    - `Path { points: Vec<Point>, closed: bool }`
    - `Polygon { points: Vec<Point> }`
    - `Circle { center: Point, radius: f64 }`

## 3. SQL Transpilation (`src/transpiler.rs`)
Update `src/transpiler.rs`:
- Map PG geometric types to SQLite `TEXT`.
- Transpile operators to function calls:
    - `&&` (overlaps) -> `geo_overlaps(left, right)`
    - `@>` (contains) -> `geo_contains(left, right)`
    - `<@` (contained in) -> `geo_contained(left, right)`
    - `<<` (strictly left) -> `geo_left(left, right)`
    - `>>` (strictly right) -> `geo_right(left, right)`
    - `<->` (distance) -> `geo_distance(left, right)`
    - `?|` (is vertical) -> `geo_vertical(obj)`
    - `?-` (is horizontal) -> `geo_horizontal(obj)`
    - `?||` (is parallel) -> `geo_parallel(left, right)`
    - `?-|` (is perpendicular) -> `geo_perpendicular(left, right)`

## 4. Function Registration (`src/main.rs`)
Register the new `geo_*` functions in the SQLite connection in `src/main.rs`.

## 5. Verification & Testing
- **Unit Tests**: Test parsing and operator logic in `src/geo.rs`.
- **Integration Tests**: Test transpiler changes in `tests/transpiler_tests.rs`.
- **E2E Tests**: Python-based tests connecting via `pgwire` to verify full end-to-end compatibility.

## 6. Documentation
- Update `README.md` to include Geometric types.
- Update `docs/TODO-FEATURES.md`.
- Create `docs/GEO.md` with detailed information on supported types and operators.
