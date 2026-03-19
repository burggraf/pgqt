# Interval Type Support

PGQT supports PostgreSQL's `INTERVAL` type for storing and manipulating time durations.

## Overview

Intervals in PGQT are stored as a delimited string in SQLite: `months|days|microseconds`. This three-component representation matches PostgreSQL's internal interval storage.

## Supported Input Formats

### Standard Format
```sql
SELECT '1 day'::interval;
SELECT '2 hours 30 minutes'::interval;
SELECT '1 year 6 months 3 days'::interval;
```

Supported units:
- **Time**: microseconds, milliseconds, seconds, minutes, hours
- **Date**: days, weeks
- **Month/Year**: months, years, decades, centuries, millennia

### ISO 8601 Duration Format
```sql
SELECT 'P1Y2M3DT4H5M6S'::interval;  -- 1 year, 2 months, 3 days, 4 hours, 5 minutes, 6 seconds
SELECT 'PT1H30M'::interval;          -- 1 hour 30 minutes (time only)
SELECT 'P1W'::interval;              -- 1 week
```

Format: `P[n]Y[n]M[n]DT[n]H[n]M[n]S`
- `P` marks the start
- `T` separates date and time components

### At-Style Format
```sql
SELECT '@ 1 minute'::interval;
SELECT '@ 5 hours'::interval;
```

### Negative Intervals
```sql
SELECT '-1 day'::interval;
SELECT '1 day ago'::interval;  -- Same as '-1 day'
```

### Special Values
```sql
SELECT 'infinity'::interval;
SELECT '-infinity'::interval;
```

## Storage Format

Intervals are stored in SQLite as: `months|days|microseconds`

Examples:
- `'1 day'::interval` → `0|1|0`
- `'2 hours'::interval` → `0|0|7200000000` (2 × 3600 × 1,000,000 microseconds)
- `'1 month'::interval` → `1|0|0`

## SQL Functions

### parse_interval(text)
Parses a PostgreSQL interval string and returns the storage format:
```sql
SELECT parse_interval('1 day 2 hours');  -- Returns: 0|1|7200000000
```

### Interval Arithmetic Functions

For internal use by the transpiler:

- `interval_add(i1, i2)` - Add two intervals
- `interval_sub(i1, i2)` - Subtract two intervals
- `interval_mul(interval, factor)` - Multiply interval by a number
- `interval_div(interval, divisor)` - Divide interval by a number
- `interval_neg(interval)` - Negate an interval

### Interval Comparison Functions

- `interval_eq(i1, i2)` - Check equality
- `interval_lt(i1, i2)` - Less than
- `interval_le(i1, i2)` - Less than or equal
- `interval_gt(i1, i2)` - Greater than
- `interval_ge(i1, i2)` - Greater than or equal
- `interval_ne(i1, i2)` - Not equal

### EXTRACT Function

Extract fields from an interval:
```sql
SELECT extract_from_interval('DAY', parse_interval('1 day 2 hours'));  -- Returns: 1
```

Supported fields:
- `EPOCH` - Total seconds
- `CENTURY`, `DECADE`, `YEAR`, `MONTH`, `DAY`
- `HOUR`, `MINUTE`, `SECOND`
- `MILLISECOND`, `MICROSECOND`

## Type Conversion

### From String
```sql
-- Cast syntax
SELECT '1 day'::interval;

-- INTERVAL literal syntax
SELECT INTERVAL '1 day';
```

### To String
```sql
SELECT CAST('1 day'::interval AS TEXT);
```

## Limitations

- Month arithmetic uses an approximation of 30.44 days per month for conversions
- Time zone information is not preserved in intervals
- Fractional months are converted to days using the 30.44 approximation

## Implementation Details

The interval module (`src/interval.rs`) provides:

- `Interval` struct with `months`, `days`, and `microseconds` fields
- `Interval::from_str()` - Parse various interval formats
- `ToString` trait - Convert to storage format
- Arithmetic operations (add, sub, mul, div, neg)
- Comparison operations (eq, lt, le, gt, ge)
- `extract()` method for field extraction

## Examples

### Basic Usage
```sql
-- Create table with interval column
CREATE TABLE events (
    id SERIAL PRIMARY KEY,
    name TEXT,
    duration INTERVAL
);

-- Insert interval values
INSERT INTO events (name, duration) VALUES 
    ('Meeting', '1 hour 30 minutes'),
    ('Conference', '2 days'),
    ('Sprint', '2 weeks');

-- Query intervals
SELECT * FROM events WHERE duration > '1 day'::interval;
```

### Arithmetic (Phase 2.2+)
```sql
-- Add intervals
SELECT '1 day'::interval + '2 hours'::interval;

-- Multiply interval
SELECT '1 hour'::interval * 2;
```

### Extraction (Phase 2.4+)
```sql
-- Extract components
SELECT EXTRACT(DAY FROM '1 day 2 hours'::interval);     -- Returns: 1
SELECT EXTRACT(HOUR FROM '1 day 2 hours'::interval);    -- Returns: 2
SELECT EXTRACT(EPOCH FROM '1 day'::interval);           -- Returns: 86400
```

## See Also

- [PostgreSQL Interval Type Documentation](https://www.postgresql.org/docs/current/datatype-datetime.html#DATATYPE-INTERVAL-INPUT)
- Phase 2.2: Interval Arithmetic Operators
- Phase 2.3: Interval Comparison Operators
- Phase 2.4: Interval Extraction Functions
