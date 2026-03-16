# COPY Binary Format Completion Plan

**Date:** 2026-03-16  
**Status:** Planning  
**Priority:** Medium  
**Estimated Effort:** 2-3 development cycles  
**Dependencies:** Phase 1-4 COPY implementation (COMPLETE)

---

## Overview

This plan outlines the work needed to complete PostgreSQL binary COPY format support in PGQT. The current implementation has partial binary format support in `src/copy.rs::process_binary_data()`, but it lacks:
- Complete binary type encoding/decoding for all PostgreSQL types
- pg_dump/pg_restore compatibility verification
- Comprehensive binary format tests
- Performance benchmarks for binary format

---

## Current State Analysis

### ✅ What's Already Implemented

1. **Binary Format Infrastructure** (`src/copy.rs`)
   - `CopyFormat::Binary` enum variant
   - `process_binary_data()` function with basic structure
   - Binary signature validation (`PGCOPY\n\xff\r\n\0`)
   - Flags and header extension parsing
   - Tuple format parsing (field count, lengths, values)
   - Binary trailer detection (-1 field count)

2. **COPY Protocol Support**
   - `CopyResponse` with format code (1 for binary)
   - `on_copy_data()`, `on_copy_done()`, `on_copy_fail()` handlers
   - State management for binary COPY operations

3. **Basic Type Handling**
   - NULL detection (length = -1)
   - Integer types (i16, i32, i64)
   - Text/blob passthrough

### ❌ What's Missing/Broken

1. **Incomplete Type Conversions**
   - ❌ Boolean type (PostgreSQL uses 1 byte: 0=false, 1=true)
   - ❌ Float types (float4, float8 in big-endian)
   - ❌ Date/Time types (timestamp, timestamptz, date, time, interval)
   - ❌ Numeric/Decimal type (variable length, special encoding)
   - ❌ UUID type (16 bytes)
   - ❌ JSON/JSONB types
   - ❌ Array types (dimensioned arrays with bounds)
   - ❌ Composite/Record types
   - ❌ Range types

2. **Endianness Issues**
   - PostgreSQL binary format uses **big-endian** (network byte order)
   - Current implementation may not handle byte order correctly on all platforms
   - Need explicit byte order conversion for all multi-byte types

3. **Type OID Mapping**
   - Binary format includes type OIDs for each column
   - Need to map PostgreSQL type OIDs to SQLite types
   - Current implementation doesn't validate or use type OIDs

4. **pg_dump Compatibility**
   - ❌ Not tested against `pg_dump -Fc` output
   - ❌ Not tested against `pg_restore` input
   - ❌ Unknown if binary format matches PostgreSQL exactly

5. **Error Handling**
   - ❌ No specific error messages for binary format issues
   - ❌ No line/row numbers in binary errors (binary doesn't have lines)
   - ❌ No recovery from malformed binary data

6. **Tests**
   - ❌ No unit tests for binary format parsing
   - ❌ No E2E tests for binary COPY
   - ❌ No pg_dump compatibility tests
   - ❌ No round-trip tests (COPY TO binary, COPY FROM binary)

7. **Documentation**
   - ❌ Binary format not documented in README.md
   - ❌ No examples in docs/copy-command.md
   - ❌ No performance benchmarks for binary format

---

## Implementation Phases

### Phase 1: Complete Type Conversions (Week 1)

**Goal:** Implement binary encoding/decoding for all common PostgreSQL types

#### Tasks

1. **Fix Endianness Handling**
   - **File:** `src/copy.rs`
   - **Action:** Use explicit big-endian conversion for all multi-byte types
   - **Details:**
     - Use `i16::from_be_bytes()`, `i32::from_be_bytes()`, etc. for reading
     - Use `i16::to_be_bytes()`, `i32::to_be_bytes()`, etc. for writing
     - Add helper functions for consistent byte order handling
   - **Verification:**
     ```rust
     // Test on both little-endian (x86) and big-endian systems
     let bytes = [0x00, 0x01];
     let value = i16::from_be_bytes(bytes);
     assert_eq!(value, 1);
     ```

2. **Implement Boolean Type**
   - **File:** `src/copy.rs`
   - **Action:** Add boolean type conversion
   - **Details:**
     - PostgreSQL: 1 byte (0x00=false, 0x01=true)
     - SQLite: INTEGER (0=false, 1=true)
   - **Code:**
     ```rust
     fn read_bool_binary(data: &[u8]) -> Result<bool> {
         if data.len() != 1 {
             return Err(anyhow!("Boolean must be 1 byte"));
         }
         Ok(data[0] != 0)
     }
     
     fn write_bool_binary(value: bool) -> Vec<u8> {
         vec![if value { 1 } else { 0 }]
     }
     ```

3. **Implement Float Types**
   - **File:** `src/copy.rs`
   - **Action:** Add float4 and float8 conversion
   - **Details:**
     - PostgreSQL: IEEE 754 big-endian
     - Use `f32::from_be_bytes()`, `f64::from_be_bytes()`
   - **Code:**
     ```rust
     fn read_f64_binary(data: &[u8]) -> Result<f64> {
         if data.len() != 8 {
             return Err(anyhow!("float8 must be 8 bytes"));
         }
         let bytes: [u8; 8] = data.try_into().unwrap();
         Ok(f64::from_be_bytes(bytes))
     }
     ```

4. **Implement Date/Time Types**
   - **File:** `src/copy.rs`
   - **Action:** Add timestamp, date, time, interval conversion
   - **Details:**
     - PostgreSQL timestamp: microseconds since 2000-01-01 (i64, big-endian)
     - PostgreSQL date: days since 2000-01-01 (i32, big-endian)
     - Convert to/from SQLite text format
   - **Reference:** PostgreSQL epoch is 2000-01-01, Unix epoch is 1970-01-01

5. **Implement Numeric/Decimal Type**
   - **File:** `src/copy.rs`
   - **Action:** Add numeric type conversion (most complex)
   - **Details:**
     - PostgreSQL numeric: variable length, special binary format
     - Header: ndigits (i16), weight (i16), sign (i16), dscale (i16)
     - Followed by ndigits 16-bit digits
     - Sign: 0x0000=positive, 0x4000=negative, 0xC000=NaN
   - **Complexity:** High - this is the most complex type

6. **Implement UUID Type**
   - **File:** `src/copy.rs`
   - **Action:** Add UUID conversion
   - **Details:**
     - PostgreSQL UUID: 16 bytes (raw bytes)
     - SQLite: TEXT (hex string with dashes)
   - **Code:**
     ```rust
     fn read_uuid_binary(data: &[u8]) -> Result<String> {
         if data.len() != 16 {
             return Err(anyhow!("UUID must be 16 bytes"));
         }
         Ok(format!("{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
             data[0], data[1], data[2], data[3], data[4], data[5], data[6], data[7],
             data[8], data[9], data[10], data[11], data[12], data[13], data[14], data[15]))
     }
     ```

7. **Implement Array Types**
   - **File:** `src/copy.rs`
   - **Action:** Add array type conversion
   - **Details:**
     - PostgreSQL arrays: dimensioned with bounds
     - Header: ndims (i32), flags (i32), element_type_oid (i32)
     - For each dimension: dim_size (i32), dim_lowerBound (i32)
     - Followed by element values
   - **Complexity:** High - recursive type handling

#### Checkpoints

- [ ] `cargo check` passes with zero errors
- [ ] All build warnings fixed
- [ ] `./run_tests.sh` passes (343 unit + 35 integration + 21 E2E)
- [ ] Unit tests for each type conversion function
- [ ] Manual test: COPY with all supported types

---

### Phase 2: Type OID Mapping (Week 1-2)

**Goal:** Implement PostgreSQL type OID to SQLite type mapping

#### Tasks

1. **Create Type OID Registry**
   - **File:** `src/copy.rs` or new `src/copy/type_oid.rs`
   - **Action:** Create mapping of PostgreSQL type OIDs to handlers
   - **Details:**
     ```rust
     const BOOL_OID: i32 = 16;
     const INT2_OID: i32 = 21;
     const INT4_OID: i32 = 23;
     const INT8_OID: i32 = 20;
     const FLOAT4_OID: i32 = 700;
     const FLOAT8_OID: i32 = 701;
     const TEXT_OID: i32 = 25;
     const TIMESTAMP_OID: i32 = 1114;
     const UUID_OID: i32 = 2950;
     // ... etc
     ```

2. **Implement Type Dispatcher**
   - **File:** `src/copy.rs`
   - **Action:** Route binary data to correct type handler based on OID
   - **Details:**
     ```rust
     fn read_value_binary(data: &[u8], type_oid: i32) -> Result<rusqlite::types::Value> {
         match type_oid {
             BOOL_OID => Ok(rusqlite::types::Value::Integer(if read_bool_binary(data)? { 1 } else { 0 })),
             INT4_OID => Ok(rusqlite::types::Value::Integer(read_i32_binary(data)? as i64)),
             FLOAT8_OID => Ok(rusqlite::types::Value::Real(read_f64_binary(data)?)),
             TEXT_OID => Ok(rusqlite::types::Value::Text(String::from_utf8_lossy(data).to_string())),
             _ => Err(anyhow!("Unsupported type OID: {}", type_oid)),
         }
     }
     ```

3. **Query Column Types**
   - **File:** `src/copy.rs`
   - **Action:** Get column type OIDs from `__pg_type__` catalog
   - **Details:**
     - Before processing binary COPY, query column types
     - Store type OIDs for each column
     - Use OIDs to dispatch type conversion

#### Checkpoints

- [ ] `cargo check` passes
- [ ] All warnings fixed
- [ ] `./run_tests.sh` passes
- [ ] Type OID mapping tested with mixed-type tables

---

### Phase 3: pg_dump Compatibility Testing (Week 2)

**Goal:** Verify binary format matches PostgreSQL exactly

#### Tasks

1. **Create Test Data**
   - **File:** `tests/data/binary_copy_test.sql`
   - **Action:** Create SQL script with all supported types
   - **Details:**
     ```sql
     CREATE TABLE test_all_types (
         id INT,
         name TEXT,
         active BOOLEAN,
         score FLOAT8,
         created TIMESTAMP,
         uid UUID
     );
     INSERT INTO test_all_types VALUES
         (1, 'John', true, 95.5, '2024-01-15 10:30:00', 'a0eebc99-9c0b-4ef8-bb6d-6bb9bd380a11'),
         (2, 'Jane', false, 87.3, '2024-01-16 14:45:00', 'a0eebc99-9c0b-4ef8-bb6d-6bb9bd380a12');
     ```

2. **Generate Reference Binary**
   - **File:** `tests/data/test_all_types.binary`
   - **Action:** Use PostgreSQL to generate reference binary COPY
   - **Command:**
     ```bash
     psql -h localhost -U postgres -d testdb -c \
       "COPY test_all_types TO STDOUT WITH (FORMAT binary)" > test_all_types.binary
     ```

3. **Create Compatibility Test**
   - **File:** `tests/copy_binary_compatibility_test.py`
   - **Action:** Test PGQT against PostgreSQL binary output
   - **Details:**
     ```python
     def test_binary_copy_from_postgres_dump():
         # Load binary data generated by PostgreSQL
         with open('tests/data/test_all_types.binary', 'rb') as f:
             binary_data = f.read()
         
         # Feed to PGQT
         cur.copy_expert("COPY test_all_types FROM STDIN WITH (FORMAT binary)", binary_data)
         
         # Verify data matches
         cur.execute("SELECT COUNT(*) FROM test_all_types")
         assert cur.fetchone()[0] == 2
     ```

4. **Create Round-Trip Test**
   - **File:** `tests/copy_binary_roundtrip_test.py`
   - **Action:** Test COPY TO binary then COPY FROM binary
   - **Details:**
     ```python
     def test_binary_copy_roundtrip():
         # Export to binary
         cur.execute("COPY test_all_types TO STDOUT WITH (FORMAT binary)")
         binary_data = cur.copy_expert("COPY test_all_types TO STDOUT WITH (FORMAT binary)", "")
         
         # Import from binary
         cur.execute("DELETE FROM test_all_types")
         cur.copy_expert("COPY test_all_types FROM STDIN WITH (FORMAT binary)", binary_data)
         
         # Verify row count matches
         cur.execute("SELECT COUNT(*) FROM test_all_types")
         assert cur.fetchone()[0] == 2
     ```

5. **Test Against pg_dump**
   - **File:** `tests/copy_pg_dump_test.py`
   - **Action:** Test loading pg_dump binary format
   - **Details:**
     ```bash
     pg_dump -Fc -t test_all_types testdb > dump.binary
     ```
   - **Note:** `-Fc` is custom format, not pure binary COPY. May need different approach.

#### Checkpoints

- [ ] `cargo check` passes
- [ ] All warnings fixed
- [ ] `./run_tests.sh` passes
- [ ] Binary compatibility test passes
- [ ] Round-trip test passes
- [ ] pg_dump test passes (if applicable)

---

### Phase 4: Error Handling & Documentation (Week 3)

**Goal:** Add comprehensive error handling and documentation

#### Tasks

1. **Implement Binary-Specific Error Messages**
   - **File:** `src/copy.rs`
   - **Action:** Add detailed error messages for binary format issues
   - **Details:**
     ```rust
     Err(anyhow!(
         "COPY {}: binary format error at byte {}: {}",
         table_name, cursor.position(), error_msg
     ))
     ```

2. **Add Binary Format Documentation**
   - **File:** `docs/copy-command.md`
   - **Action:** Document binary format support
   - **Content:**
     - Supported types table
     - Binary format specification reference
     - Usage examples
     - Performance characteristics
     - Known limitations

3. **Update README.md**
   - **File:** `README.md`
   - **Action:** Add binary format to feature list
   - **Content:**
     ```markdown
     ### COPY Command Support
     
     - ✅ COPY FROM STDIN (TEXT, CSV, BINARY)
     - ✅ COPY TO STDOUT (TEXT, CSV, BINARY)
     - ✅ All common PostgreSQL types supported in binary format
     - ✅ pg_dump compatible binary format
     ```

4. **Add Performance Benchmarks**
   - **File:** `benches/copy_benchmark.rs`
   - **Action:** Add binary format benchmarks
   - **Metrics:**
     - Rows/sec for binary format
     - Comparison: TEXT vs CSV vs BINARY
     - Memory usage

#### Checkpoints

- [ ] `cargo check` passes
- [ ] All warnings fixed
- [ ] `./run_tests.sh` passes
- [ ] Documentation complete
- [ ] Benchmarks show binary format performance

---

## Technical Details

### PostgreSQL Binary Format Specification

#### File Structure
```
File Header (11 bytes): "PGCOPY\n\377\r\n\0"
Flags (4 bytes): 0x00000000 (bit 16 = has OIDs, deprecated)
Header Extension (4 bytes): Length of extension area (usually 0)

For each row:
  Field Count (2 bytes): Number of columns (i16, big-endian)
  For each column:
    Length (4 bytes): -1 for NULL, or byte count (i32, big-endian)
    Value (N bytes): Type-specific binary representation

File Trailer:
  Field Count (2 bytes): -1 (i16, big-endian)
```

#### Type Encodings

| Type | OID | Size | Encoding |
|------|-----|------|----------|
| BOOLEAN | 16 | 1 byte | 0x00=false, 0x01=true |
| INT2 | 21 | 2 bytes | i16, big-endian |
| INT4 | 23 | 4 bytes | i32, big-endian |
| INT8 | 20 | 8 bytes | i64, big-endian |
| FLOAT4 | 700 | 4 bytes | f32, IEEE 754, big-endian |
| FLOAT8 | 701 | 8 bytes | f64, IEEE 754, big-endian |
| TEXT | 25 | Variable | UTF-8 bytes |
| TIMESTAMP | 1114 | 8 bytes | i64 microseconds since 2000-01-01 |
| DATE | 1082 | 4 bytes | i32 days since 2000-01-01 |
| UUID | 2950 | 16 bytes | Raw bytes |
| NUMERIC | 1700 | Variable | Special format (see below) |

#### Numeric Type Binary Format
```
ndigits (2 bytes): Number of digits (i16, big-endian)
weight (2 bytes): Weight of first digit (i16, big-endian)
sign (2 bytes): 0x0000=positive, 0x4000=negative, 0xC000=NaN
dscale (2 bytes): Display scale (i16, big-endian)
digits (ndigits * 2 bytes): Each digit is i16, big-endian (base 10000)
```

---

## Testing Strategy

### Unit Tests

```rust
#[cfg(test)]
mod binary_format_tests {
    use super::*;
    
    #[test]
    fn test_read_bool_binary() {
        assert_eq!(read_bool_binary(&[0]).unwrap(), false);
        assert_eq!(read_bool_binary(&[1]).unwrap(), true);
        assert!(read_bool_binary(&[2]).is_err());
    }
    
    #[test]
    fn test_read_i32_binary() {
        let bytes = [0x00, 0x00, 0x00, 0x01]; // big-endian 1
        assert_eq!(read_i32_binary(&bytes).unwrap(), 1);
        
        let bytes = [0xFF, 0xFF, 0xFF, 0xFF]; // big-endian -1
        assert_eq!(read_i32_binary(&bytes).unwrap(), -1);
    }
    
    #[test]
    fn test_read_f64_binary() {
        let bytes = [0x3F, 0xF0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]; // 1.0
        assert_eq!(read_f64_binary(&bytes).unwrap(), 1.0);
    }
    
    #[test]
    fn test_binary_copy_signature() {
        let signature = b"PGCOPY\n\xff\r\n\0";
        assert_eq!(signature.len(), 11);
    }
}
```

### Integration Tests

```rust
#[test]
fn test_binary_copy_all_types() {
    let conn = setup_test_db();
    conn.execute(
        "CREATE TABLE test (id INT, name TEXT, active BOOLEAN, score FLOAT8)",
        [],
    )?;
    
    let copy_handler = CopyHandler::new(...);
    
    // Create binary COPY data
    let mut binary_data = Vec::new();
    binary_data.extend_from_slice(b"PGCOPY\n\xff\r\n\0"); // Signature
    binary_data.extend_from_slice(&0i32.to_be_bytes()); // Flags
    binary_data.extend_from_slice(&0i32.to_be_bytes()); // Header extension
    
    // Row 1
    binary_data.extend_from_slice(&4i16.to_be_bytes()); // 4 columns
    binary_data.extend_from_slice(&4i32.to_be_bytes()); // id length
    binary_data.extend_from_slice(&1i32.to_be_bytes()); // id=1
    binary_data.extend_from_slice(&4i32.to_be_bytes()); // name length
    binary_data.extend_from_slice(b"John"); // name
    binary_data.extend_from_slice(&1i32.to_be_bytes()); // active length
    binary_data.extend_from_slice(&[1]); // active=true
    binary_data.extend_from_slice(&8i32.to_be_bytes()); // score length
    binary_data.extend_from_slice(&95.5f64.to_be_bytes()); // score
    
    // Row 2
    // ... similar
    
    // Trailer
    binary_data.extend_from_slice(&(-1i16).to_be_bytes());
    
    copy_handler.on_copy_data(..., CopyData::new(binary_data))?;
    copy_handler.on_copy_done(...)?;
    
    // Verify
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM test", [], |r| r.get(0))?;
    assert_eq!(count, 2);
}
```

### E2E Tests (Python)

```python
#!/usr/bin/env python3
"""Binary COPY format E2E tests"""

import psycopg2
import struct

def test_binary_copy_basic():
    conn = psycopg2.connect("host=localhost port=5434 dbname=test user=postgres")
    cur = conn.cursor()
    
    cur.execute("CREATE TABLE test (id INT, name TEXT)")
    
    # Create binary data manually
    binary_data = bytearray()
    binary_data.extend(b"PGCOPY\n\xff\r\n\0")  # Signature
    binary_data.extend(struct.pack('>i', 0))    # Flags
    binary_data.extend(struct.pack('>i', 0))    # Header extension
    
    # Row 1
    binary_data.extend(struct.pack('>h', 2))    # 2 columns
    binary_data.extend(struct.pack('>i', 4))    # id length
    binary_data.extend(struct.pack('>i', 1))    # id=1
    binary_data.extend(struct.pack('>i', 4))    # name length
    binary_data.extend(b'John')                  # name
    
    # Trailer
    binary_data.extend(struct.pack('>h', -1))   # End marker
    
    cur.copy_expert("COPY test FROM STDIN WITH (FORMAT binary)", binary_data)
    conn.commit()
    
    cur.execute("SELECT COUNT(*) FROM test")
    assert cur.fetchone()[0] == 1
    
    cur.close()
    conn.close()
```

---

## Risk Assessment

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Numeric type complexity | High | High | Implement last, test extensively |
| Endianness bugs | Medium | High | Test on both x86 and ARM |
| pg_dump incompatibility | Medium | Medium | Test against multiple PostgreSQL versions |
| Performance regression | Low | Medium | Benchmark before/after |
| Memory issues with large binaries | Low | High | Implement streaming, chunked processing |

---

## Success Criteria

1. **Functionality**
   - ✅ All common PostgreSQL types supported in binary format
   - ✅ Binary COPY FROM works with pg_dump output
   - ✅ Binary COPY TO produces valid PostgreSQL binary format
   - ✅ Round-trip test passes (TO binary, FROM binary)

2. **Performance**
   - ✅ Binary format faster than TEXT for large datasets
   - ✅ Binary format faster than CSV for large datasets
   - ✅ Memory usage <100MB for 1M row binary COPY

3. **Compatibility**
   - ✅ Works with PostgreSQL 14, 15, 16, 17
   - ✅ Works with pg_dump binary output
   - ✅ Works with psycopg2 copy_expert()
   - ✅ Works with node-pg copyStreams

4. **Quality**
   - ✅ Zero compiler warnings
   - ✅ All existing tests pass
   - ✅ New binary format tests pass
   - ✅ Documentation complete

---

## Timeline

| Week | Phase | Deliverables |
|------|-------|--------------|
| 1 | Phase 1 | Type conversions complete |
| 1-2 | Phase 2 | Type OID mapping complete |
| 2 | Phase 3 | pg_dump compatibility verified |
| 3 | Phase 4 | Error handling, docs, benchmarks |

---

## References

- [PostgreSQL Binary Format Documentation](https://www.postgresql.org/docs/current/sql-copy.html#SQL-COPY-BINARY-FORMAT)
- [PostgreSQL Type OIDs](https://github.com/postgres/postgres/blob/master/src/include/catalog/pg_type.dat)
- [PostgreSQL Numeric Binary Format](https://github.com/postgres/postgres/blob/master/src/backend/utils/adt/numeric.c)
- [RFC 4180 CSV Format](https://www.ietf.org/rfc/rfc4180.txt)

---

*Last updated: 2026-03-16*
