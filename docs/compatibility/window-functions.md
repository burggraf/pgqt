# Window Functions and Hypothetical-Set Aggregates

## Window Functions

Standard window functions with `OVER` clause are fully supported in PGQT by mapping them to SQLite's native window function support.

Example:
```sql
SELECT name, rank() OVER (PARTITION BY department ORDER BY salary DESC) FROM employees;
```

## Hypothetical-Set Aggregates

PostgreSQL supports hypothetical-set aggregates that compute what rank a value *would* have if it were inserted into a dataset. These are used with the `WITHIN GROUP` clause.

PGQT implements these by transpiling them to custom SQLite aggregate functions.

### Supported Functions

| Function | Description | Status |
|----------|-------------|--------|
| `rank(val) WITHIN GROUP (ORDER BY col)` | Hypothetical rank with gaps | ✅ Implemented |
| `dense_rank(val) WITHIN GROUP (ORDER BY col)` | Hypothetical rank without gaps | ✅ Implemented |
| `percent_rank(val) WITHIN GROUP (ORDER BY col)` | Hypothetical relative rank | ✅ Implemented |
| `cume_dist(val) WITHIN GROUP (ORDER BY col)` | Hypothetical cumulative distribution | ✅ Implemented |

### Examples

```sql
-- What rank would the value 3 have in the set of x?
SELECT rank(3) WITHIN GROUP (ORDER BY x) FROM t;

-- Hypothetical percent rank of 0.5
SELECT percent_rank(0.5) WITHIN GROUP (ORDER BY x) FROM t;
```

### Implementation Notes

- These functions are implemented as custom SQLite aggregate functions: `__pg_hypothetical_rank`, `__pg_hypothetical_dense_rank`, etc.
- Currently, single-column `ORDER BY` is supported.
- The implementation correctly handles empty sets (returning rank 1 for `rank` and `dense_rank`, 0 for `percent_rank`, and 1 for `cume_dist`).
