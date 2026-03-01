# PostgreSQL Range Types for SQLite

## Implementation Notes
- Store ranges as TEXT in PostgreSQL's canonical representation (e.g., `[10, 21)`).
- Use `RangeValue` enum with sub-types to handle discrete vs. continuous logic.
- Parser should handle all standard PG formats: `[low, high]`, `(low, high)`, `[low, )`, `empty`, etc.
- Discrete types: `int4range`, `int8range`, `daterange`.
- Continuous types: `numrange`, `tsrange`, `tstzrange`.

## Discrete Canonicalization
PostgreSQL normalizes discrete ranges to `[low, high)`:
- `[10, 10]` -> `[10, 11)`
- `(10, 20)` -> `[11, 20)`
- `[10, 20]` -> `[10, 21)`
- `(10, 11)` -> `empty`
- `[10, 9]` -> `empty`

## Metadata Functions
- `lower(range)` -> returns low bound (or NULL)
- `upper(range)` -> returns high bound (or NULL)
- `lower_inc(range)` -> boolean
- `upper_inc(range)` -> boolean
- `lower_inf(range)` -> boolean
- `upper_inf(range)` -> boolean
- `isempty(range)` -> boolean
- `range_merge(r1, r2)` -> returns smallest spanning range

## Operators
- `@>`: Contains element or range
- `<@`: Contained by
- `&&`: Overlaps
- `<<`: Strictly left
- `>>`: Strictly right
- `-|-`: Adjacent
- `&<`: Does not extend right
- `&>`: Does not extend left
- `+`: Union
- `*`: Intersection
- `-`: Difference
