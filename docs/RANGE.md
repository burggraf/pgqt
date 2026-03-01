# PostgreSQL Range Types Support

PGlite Proxy provides support for PostgreSQL range types by emulating them as `TEXT` in SQLite. This includes support for canonicalization, common operators, and metadata functions.

## Supported Range Types

| PostgreSQL Type | Element Type | SQLite Storage |
| :--- | :--- | :--- |
| `int4range` | `integer` | `TEXT` |
| `int8range` | `bigint` | `TEXT` |
| `numrange` | `numeric` | `TEXT` |
| `tsrange` | `timestamp` | `TEXT` |
| `tstzrange` | `timestamptz` | `TEXT` |
| `daterange` | `date` | `TEXT` |

## Canonicalization

For discrete types (`int4range`, `int8range`, `daterange`), PGlite Proxy automatically normalizes ranges to the `[low, high)` format, consistent with PostgreSQL behavior.

Example:
- `[10, 20]` becomes `[10, 21)`
- `(10, 20)` becomes `[11, 20)`
- `[10, 10)` becomes `empty`

## Supported Operators

| Operator | Description |
| :--- | :--- |
| `@>` | Contains (element or range) |
| `<@` | Contained by |
| `&&` | Overlaps |
| `<<` | Strictly left |
| `>>` | Strictly right |
| `-|-` | Adjacent |
| `&<` | Does not extend right |
| `&>` | Does not extend left |

## Supported Functions

| Function | Description |
| :--- | :--- |
| `lower(range)` | Returns the lower bound |
| `upper(range)` | Returns the upper bound |
| `lower_inc(range)` | Returns true if lower bound is inclusive |
| `upper_inc(range)` | Returns true if upper bound is inclusive |
| `lower_inf(range)` | Returns true if range is unbounded on the left |
| `upper_inf(range)` | Returns true if range is unbounded on the right |
| `isempty(range)` | Returns true if range is empty |

## Usage Examples

```sql
-- Create a table with a range column
CREATE TABLE reservations (
    room_id INT,
    booking_period DATERANGE
);

-- Insert a range
INSERT INTO reservations (room_id, booking_period) 
VALUES (101, '[2023-01-01, 2023-01-05]');

-- Check overlap
SELECT * FROM reservations 
WHERE booking_period && '[2023-01-04, 2023-01-10]'::daterange;

-- Metadata functions
SELECT lower(booking_period), upper(booking_period) FROM reservations;
```

## Implementation Details

Ranges are stored as strings in their PostgreSQL canonical format. Operators and functions are implemented as SQLite scalar functions that parse these strings on the fly. 

Note: While functional, these operations do not currently benefit from GiST-style indexing in SQLite. Range queries will perform a full table scan for range-specific operators.
