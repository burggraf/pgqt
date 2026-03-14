# Known Compatibility Issues

This document tracks known compatibility limitations where PGQT is more permissive than PostgreSQL.

## Error Handling Gaps

PGQT is more permissive than PostgreSQL in some cases. The following errors may not be caught:

### GROUP BY Validation
- Non-aggregated columns not in GROUP BY may not error
- Complex GROUP BY expressions may not be validated
- Example: `SELECT a, b FROM t GROUP BY a` - PostgreSQL errors, PGQT may accept

### Type Checking
- Some invalid type casts may be accepted
- Implicit type conversions may differ from PostgreSQL
- Example: `'text'::int` - PostgreSQL errors, PGQT may accept

### Column References
- Invalid column references in subqueries may not error
- Ambiguous column references may resolve differently

### Constraint Validation
- Some CHECK constraints may not be fully validated
- Foreign key constraints are not enforced by SQLite in some configurations

## Why These Gaps Exist

1. **SQLite Foundation**: PGQT uses SQLite as the underlying engine, which is more permissive than PostgreSQL
2. **Transpilation Approach**: PGQT transpiles PostgreSQL SQL to SQLite SQL rather than implementing a full PostgreSQL engine
3. **Practical Trade-offs**: Strict validation would require significant additional complexity

## Impact on Applications

Most applications will not encounter issues with these gaps. However, if you rely on PostgreSQL's strict validation to catch errors early, you should:

1. Test your SQL against a real PostgreSQL instance during development
2. Use the compatibility test suite to identify potential issues
3. Review this document for known gaps

## Future Improvements

These gaps are tracked as "Error Handling Gap" in compatibility tests. Future versions may add optional strict mode validation.
