# Window Functions Implementation Plan

## Overview
Implement full PostgreSQL-compatible window functions for PGlite Proxy by properly transpiling window function syntax to SQLite.

## Background

### PostgreSQL Window Function Features
1. **Window Functions** - Functions that operate over a set of rows (window frame)
2. **OVER Clause** - Defines the window specification:
   - PARTITION BY - Divides rows into partitions
   - ORDER BY - Orders rows within partition
   - Frame Specification - Defines the window frame bounds

### SQLite Support (3.25.0+)
SQLite supports window functions with nearly identical syntax:
- All ranking functions: row_number(), rank(), dense_rank(), percent_rank(), cume_dist(), ntile()
- All offset functions: lag(), lead(), first_value(), last_value(), nth_value()
- Aggregate functions as window functions: sum(), avg(), count(), min(), max(), etc.
- Frame specifications: ROWS, RANGE, GROUPS
- Frame bounds: UNBOUNDED PRECEDING/FOLLOWING, CURRENT ROW, offset PRECEDING/FOLLOWING
- EXCLUDE clause (SQLite 3.28.0+)

### Key Differences to Address
1. **Default Frame Behavior**: PostgreSQL defaults differ based on ORDER BY presence
   - With ORDER BY: RANGE BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW
   - Without ORDER BY: Entire partition (RANGE BETWEEN UNBOUNDED PRECEDING AND UNBOUNDED FOLLOWING)

2. **Named Windows**: PostgreSQL allows WINDOW clause for reusable window definitions

3. **Function Compatibility**: All built-in window functions are supported in SQLite

## Implementation Plan

### Phase 1: Core Window Function Support
- [ ] Add WindowDef handling in transpiler
- [ ] Handle PARTITION BY clause
- [ ] Handle ORDER BY clause within window
- [ ] Handle frame_options bitmask decoding
- [ ] Handle frame bounds (UNBOUNDED, CURRENT ROW, offset)

### Phase 2: Frame Specifications
- [ ] ROWS frame mode
- [ ] RANGE frame mode
- [ ] GROUPS frame mode (SQLite 3.28+)
- [ ] BETWEEN ... AND ... syntax
- [ ] EXCLUDE clause (SQLite 3.28+)

### Phase 3: Named Windows
- [ ] Parse WINDOW clause in SelectStmt
- [ ] Resolve window references by name
- [ ] Handle window inheritance

### Phase 4: Function Transpilation
- [ ] Verify all PostgreSQL window functions work
- [ ] Add any necessary function name mappings
- [ ] Handle aggregate functions as window functions

## Frame Options Bitmask

From PostgreSQL source (parsenodes.h):
```
FRAMEOPTION_NONDEFAULT                  = 0x00001
FRAMEOPTION_RANGE                       = 0x00002
FRAMEOPTION_ROWS                        = 0x00004
FRAMEOPTION_GROUPS                      = 0x00008
FRAMEOPTION_BETWEEN                     = 0x00010
FRAMEOPTION_START_UNBOUNDED_PRECEDING   = 0x00020
FRAMEOPTION_END_UNBOUNDED_PRECEDING     = 0x00040 (disallowed)
FRAMEOPTION_START_UNBOUNDED_FOLLOWING   = 0x00080 (disallowed)
FRAMEOPTION_END_UNBOUNDED_FOLLOWING     = 0x00100
FRAMEOPTION_START_CURRENT_ROW           = 0x00200
FRAMEOPTION_END_CURRENT_ROW             = 0x00400
FRAMEOPTION_START_OFFSET_PRECEDING      = 0x00800
FRAMEOPTION_END_OFFSET_PRECEDING        = 0x01000
FRAMEOPTION_START_OFFSET_FOLLOWING      = 0x02000
FRAMEOPTION_END_OFFSET_FOLLOWING        = 0x04000
FRAMEOPTION_EXCLUDE_CURRENT_ROW         = 0x08000
FRAMEOPTION_EXCLUDE_GROUP               = 0x10000
FRAMEOPTION_EXCLUDE_TIES                = 0x20000
```

## Testing Strategy

### Unit Tests
- Test each window function (row_number, rank, dense_rank, etc.)
- Test PARTITION BY with single and multiple columns
- Test ORDER BY with ASC/DESC and multiple columns
- Test frame specifications (ROWS, RANGE, GROUPS)
- Test frame bounds (UNBOUNDED, CURRENT ROW, offset)
- Test BETWEEN syntax
- Test named windows

### E2E Tests
- Execute window queries through proxy
- Compare results with expected SQLite output
- Test complex queries combining window functions

## Files to Modify/Create

1. `src/transpiler.rs` - Add WindowDef reconstruction
2. `tests/window_tests.rs` - Unit tests for window functions
3. `tests/window_e2e_test.py` - End-to-end tests
4. `docs/WINDOW.md` - User documentation

## Compatibility Notes

### SQLite Version Requirements
- Window functions: SQLite 3.25.0+ (2018-09-15)
- GROUPS mode: SQLite 3.28.0+ (2019-04-16)
- EXCLUDE clause: SQLite 3.28.0+ (2019-04-16)

Most systems should have SQLite 3.35+ by now (required for RETURNING), so this should not be an issue.
