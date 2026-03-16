# COPY Encoding Support Plan

**Date:** 2026-03-16  
**Status:** Planning  
**Priority:** Medium  
**Estimated Effort:** 2-3 development cycles  
**Dependencies:** Phase 1-4 COPY implementation (COMPLETE)

---

## Overview

This plan outlines the work needed to add comprehensive character encoding support to the COPY command in PGQT. The current implementation only supports UTF-8 encoding. This plan will add support for common PostgreSQL encodings including LATIN1, WINDOWS-1252, EUC_JP, and others.

---

## Current State Analysis

### ✅ What's Already Implemented

1. **Encoding Framework** (`src/copy.rs`)
   - `encoding` field in `CopyOptions` struct
   - Default encoding set to "UTF8"
   - Framework in place for future encoding_rs integration

2. **UTF-8 Support**
   - All COPY operations currently work with UTF-8
   - Rust native string handling is UTF-8
   - No conversion needed for UTF-8 data

3. **COPY Options Parsing**
   - `ENCODING` option recognized in COPY command
   - Value stored in `CopyOptions.encoding` field
   - Passed to processing functions

### ❌ What's Missing

1. **Encoding Conversion Implementation**
   - ❌ No actual encoding conversion code
   - ❌ `encoding_rs` crate not added to dependencies
   - ❌ No encoding detection or validation
   - ❌ No fallback for unsupported encodings

2. **Supported Encodings**
   - ❌ LATIN1 (ISO-8859-1)
   - ❌ LATIN2, LATIN3, LATIN4, etc.
   - ❌ WINDOWS-1250, WINDOWS-1251, WINDOWS-1252, etc.
   - ❌ EUC_JP, EUC_KR, EUC_TW
   - ❌ SJIS (Shift_JIS)
   - ❌ GB18030, GBK, BIG5
   - ❌ KOI8-R, KOI8-U
   - ❌ ISO-8859-5, ISO-8859-6, ISO-8859-7, etc.
   - ❌ SQL_ASCII (no conversion)

3. **COPY FROM Encoding**
   - ❌ No conversion from source encoding to UTF-8
   - ❌ No validation of input encoding
   - ❌ No error handling for invalid byte sequences

4. **COPY TO Encoding**
   - ❌ No conversion from UTF-8 to target encoding
   - ❌ No handling of characters not representable in target encoding
   - ❌ No fallback for unconvertible characters

5. **Server-Side File Encoding**
   - ❌ No encoding handling for COPY FROM/TO '/path/to/file'
   - ❌ File encoding not detected or specified

6. **Error Handling**
   - ❌ No specific error messages for encoding issues
   - ❌ No recovery from encoding errors
   - ❌ No ON_ERROR handling for encoding problems

7. **Tests**
   - ❌ No unit tests for encoding conversion
   - ❌ No E2E tests for non-UTF8 encodings
   - ❌ No round-trip tests (COPY TO encoding X, COPY FROM encoding X)
   - ❌ No tests for invalid encoding data

8. **Documentation**
   - ❌ Encoding support not documented
   - ❌ No list of supported encodings
   - ❌ No examples of encoding usage
   - ❌ No performance impact documentation

---

## Implementation Phases

### Phase 1: Core Encoding Infrastructure (Week 1)

**Goal:** Add encoding_rs dependency and basic conversion framework

#### Tasks

1. **Add encoding_rs Dependency**
   - **File:** `Cargo.toml`
   - **Action:** Add encoding_rs crate
   - **Details:**
     ```toml
     [dependencies]
     encoding_rs = "0.8"
     ```
   - **Why encoding_rs:**
     - High-performance encoding library
     - Used by Firefox
     - Supports all major encodings
     - Well-maintained and tested
     - Compatible with Rust's encoding standards

2. **Create Encoding Module**
   - **File:** `src/copy/encoding.rs` (new)
   - **Action:** Create dedicated encoding module
   - **Details:**
     ```rust
     //! COPY encoding support
     //!
     //! This module handles character encoding conversion for COPY operations.
     
     use encoding_rs::{Encoding, UTF_8, LATIN1, WINDOWS_1252, /* etc */};
     use anyhow::{anyhow, Result};
     
     /// Supported encodings for COPY operations
     #[derive(Debug, Clone, PartialEq)]
     pub enum CopyEncoding {
         Utf8,
         Latin1,
         Latin2,
         // ... other encodings
         Windows1252,
         // ... etc
         EucJp,
         ShiftJis,
         // ... etc
     }
     
     impl CopyEncoding {
         /// Parse encoding name from string (case-insensitive)
         pub fn from_name(name: &str) -> Result<Self> {
             match name.to_uppercase().as_str() {
                 "UTF8" | "UTF-8" | "UNICODE" => Ok(CopyEncoding::Utf8),
                 "LATIN1" | "ISO-8859-1" => Ok(CopyEncoding::Latin1),
                 "WINDOWS-1252" | "WIN-1252" => Ok(CopyEncoding::Windows1252),
                 // ... etc
                 _ => Err(anyhow!("Unsupported encoding: {}", name)),
             }
         }
         
         /// Get the encoding_rs Encoding for this encoding
         pub fn to_encoding_rs(&self) -> &'static Encoding {
             match self {
                 CopyEncoding::Utf8 => UTF_8,
                 CopyEncoding::Latin1 => LATIN1,
                 CopyEncoding::Windows1252 => WINDOWS_1252,
                 // ... etc
             }
         }
     }
     
     /// Convert bytes from source encoding to UTF-8
     pub fn decode_to_utf8(bytes: &[u8], encoding: &CopyEncoding) -> Result<String> {
         let enc = encoding.to_encoding_rs();
         let (cow, _, had_errors) = enc.decode(bytes);
         if had_errors {
             // Handle decoding errors based on strategy
             return Err(anyhow!("Invalid byte sequence in {}", encoding.to_name()));
         }
         Ok(cow.into_owned())
     }
     
     /// Convert UTF-8 string to target encoding
     pub fn encode_from_utf8(text: &str, encoding: &CopyEncoding) -> Result<Vec<u8>> {
         let enc = encoding.to_encoding_rs();
         let (cow, _, had_errors) = enc.encode(text);
         if had_errors {
             return Err(anyhow!("Character cannot be encoded in {}", encoding.to_name()));
         }
         Ok(cow.into_owned())
     }
     ```

3. **Update CopyOptions**
   - **File:** `src/copy.rs`
   - **Action:** Change encoding field from String to CopyEncoding
   - **Details:**
     ```rust
     pub struct CopyOptions {
         // ... existing fields ...
         pub encoding: super::encoding::CopyEncoding,
     }
     ```

4. **Update COPY Option Parsing**
   - **File:** `src/transpiler/ddl.rs`
   - **Action:** Parse ENCODING option and validate
   - **Details:**
     ```rust
     "ENCODING" => {
         if let Some(ref arg) = def.arg {
             if let Some(ref inner) = arg.node {
                 if let NodeEnum::String(s) = inner {
                     options.encoding = CopyEncoding::from_name(&s.sval)?;
                 }
             }
         }
     }
     ```

#### Checkpoints

- [ ] `cargo check` passes with zero errors
- [ ] All build warnings fixed
- [ ] `./run_tests.sh` passes (343 unit + 35 integration + 21 E2E)
- [ ] Unit tests for encoding parsing
- [ ] Manual test: COPY with ENCODING 'LATIN1'

---

### Phase 2: COPY FROM Encoding Conversion (Week 1-2)

**Goal:** Implement encoding conversion for COPY FROM STDIN

#### Tasks

1. **Update process_text_data()**
   - **File:** `src/copy.rs`
   - **Action:** Add encoding conversion before parsing
   - **Details:**
     ```rust
     fn process_text_data(
         &self,
         data: &[u8],
         table_name: &str,
         columns: &[String],
         options: &CopyOptions,
     ) -> Result<usize> {
         // Convert from source encoding to UTF-8
         let utf8_content = decode_to_utf8(data, &options.encoding)
             .map_err(|e| anyhow!("COPY {}: encoding error: {}", table_name, e))?;
         
         // Now parse UTF-8 content as before
         let lines: Vec<&str> = utf8_content.split_inclusive('\n').collect();
         // ... rest of processing
     }
     ```

2. **Update process_csv_data()**
   - **File:** `src/copy.rs`
   - **Action:** Add encoding conversion before parsing
   - **Details:**
     ```rust
     fn process_csv_data(
         &self,
         data: &[u8],
         table_name: &str,
         columns: &[String],
         options: &CopyOptions,
     ) -> Result<usize> {
         // Convert from source encoding to UTF-8
         let utf8_content = decode_to_utf8(data, &options.encoding)
             .map_err(|e| anyhow!("COPY {}: encoding error: {}", table_name, e))?;
         
         // Now parse UTF-8 content as before
         let lines: Vec<&str> = utf8_content.lines().collect();
         // ... rest of processing
     }
     ```

3. **Update process_binary_data()**
   - **File:** `src/copy.rs`
   - **Action:** Add encoding conversion for text fields
   - **Details:**
     ```rust
     fn process_binary_data(
         &self,
         data: &[u8],
         table_name: &str,
         columns: &[String],
         options: &CopyOptions,
     ) -> Result<usize> {
         // Binary format has type OIDs, so we know which fields are text
         // For TEXT, VARCHAR, etc., convert from source encoding
         // For other types, use native binary format
         
         // ... existing binary parsing ...
         
         if is_text_type(type_oid) {
             let utf8_value = decode_to_utf8(field_data, &options.encoding)?;
             // Use utf8_value
         } else {
             // Use field_data directly
         }
     }
     ```

4. **Implement Encoding Error Handling**
   - **File:** `src/copy.rs`
   - **Action:** Add strategies for handling encoding errors
   - **Details:**
     ```rust
     /// Strategy for handling encoding errors
     #[derive(Debug, Clone, PartialEq)]
     pub enum EncodingErrorStrategy {
         /// Return error on first invalid byte sequence
         Error,
         /// Replace invalid sequences with replacement character ()
         Replace,
         /// Skip invalid byte sequences
         Ignore,
     }
     
     fn decode_to_utf8_with_strategy(
         bytes: &[u8],
         encoding: &CopyEncoding,
         strategy: &EncodingErrorStrategy,
     ) -> Result<String> {
         let enc = encoding.to_encoding_rs();
         let (cow, _, had_errors) = match strategy {
             EncodingErrorStrategy::Error => enc.decode(bytes),
             EncodingErrorStrategy::Replace => enc.decode_with_bom_removal(bytes),
             EncodingErrorStrategy::Ignore => {
                 // Custom handling for ignore strategy
                 enc.decode(bytes)
             }
         };
         
         if had_errors && matches!(strategy, EncodingErrorStrategy::Error) {
             return Err(anyhow!("Invalid byte sequence"));
         }
         
         Ok(cow.into_owned())
     }
     ```

5. **Add ENCODING Option to COPY**
   - **File:** `src/transpiler/ddl.rs`
   - **Action:** Support encoding in COPY options
   - **Details:**
     ```sql
     COPY table FROM STDIN WITH (FORMAT csv, ENCODING 'LATIN1');
     ```

#### Checkpoints

- [ ] `cargo check` passes
- [ ] All warnings fixed
- [ ] `./run_tests.sh` passes
- [ ] Unit tests for encoding conversion
- [ ] Manual test: COPY FROM with LATIN1 data

---

### Phase 3: COPY TO Encoding Conversion (Week 2)

**Goal:** Implement encoding conversion for COPY TO STDOUT

#### Tasks

1. **Update start_copy_to()**
   - **File:** `src/copy.rs`
   - **Action:** Add encoding conversion after formatting
   - **Details:**
     ```rust
     pub fn start_copy_to(&self, query: String, options: CopyOptions) -> Result<Response> {
         // ... existing query execution ...
         
         while let Some(row) = rows.next()? {
             // ... format row as UTF-8 string ...
             let utf8_line = format_row(...);
             
             // Convert from UTF-8 to target encoding
             let encoded_line = encode_from_utf8(&utf8_line, &options.encoding)
                 .map_err(|e| anyhow!("Encoding error: {}", e))?;
             
             all_data.push(Ok(CopyData::new(Bytes::from(encoded_line))));
         }
         
         // ... rest of processing
     }
     ```

2. **Handle Unconvertible Characters**
   - **File:** `src/copy.rs`
   - **Action:** Add strategy for characters not in target encoding
   - **Details:**
     ```rust
     fn encode_from_utf8_with_strategy(
         text: &str,
         encoding: &CopyEncoding,
         strategy: &EncodingErrorStrategy,
     ) -> Result<Vec<u8>> {
         let enc = encoding.to_encoding_rs();
         let (cow, _, had_errors) = match strategy {
             EncodingErrorStrategy::Error => enc.encode(text),
             EncodingErrorStrategy::Replace => enc.encode_with_bom_removal(text),
             EncodingErrorStrategy::Ignore => enc.encode(text),
         };
         
         if had_errors && matches!(strategy, EncodingErrorStrategy::Error) {
             return Err(anyhow!("Character cannot be encoded"));
         }
         
         Ok(cow.into_owned())
     }
     ```

3. **Add ENCODING Option to COPY TO**
   - **File:** `src/transpiler/ddl.rs`
   - **Action:** Support encoding in COPY TO options
   - **Details:**
     ```sql
     COPY table TO STDOUT WITH (FORMAT csv, ENCODING 'LATIN1');
     ```

4. **Optimize Common Case (UTF-8)**
   - **File:** `src/copy.rs`
   - **Action:** Skip conversion for UTF-8 (no-op)
   - **Details:**
     ```rust
     if options.encoding == CopyEncoding::Utf8 {
         // No conversion needed, use UTF-8 data directly
         all_data.push(Ok(CopyData::new(Bytes::from(utf8_line.into_bytes()))));
     } else {
         // Convert encoding
         let encoded_line = encode_from_utf8(&utf8_line, &options.encoding)?;
         all_data.push(Ok(CopyData::new(Bytes::from(encoded_line))));
     }
     ```

#### Checkpoints

- [ ] `cargo check` passes
- [ ] All warnings fixed
- [ ] `./run_tests.sh` passes
- [ ] Unit tests for encoding conversion
- [ ] Manual test: COPY TO with LATIN1 encoding

---

### Phase 4: Testing & Documentation (Week 3)

**Goal:** Comprehensive testing and documentation

#### Tasks

1. **Create Encoding Unit Tests**
   - **File:** `src/copy/encoding.rs`
   - **Action:** Add comprehensive unit tests
   - **Details:**
     ```rust
     #[cfg(test)]
     mod tests {
         use super::*;
         
         #[test]
         fn test_encoding_from_name() {
             assert_eq!(CopyEncoding::from_name("UTF8").unwrap(), CopyEncoding::Utf8);
             assert_eq!(CopyEncoding::from_name("utf-8").unwrap(), CopyEncoding::Utf8);
             assert_eq!(CopyEncoding::from_name("LATIN1").unwrap(), CopyEncoding::Latin1);
             assert_eq!(CopyEncoding::from_name("WINDOWS-1252").unwrap(), CopyEncoding::Windows1252);
             
             assert!(CopyEncoding::from_name("INVALID").is_err());
         }
         
         #[test]
         fn test_latin1_roundtrip() {
             let original = "Hello, Wörld!";
             let encoded = encode_from_utf8(original, &CopyEncoding::Latin1).unwrap();
             let decoded = decode_to_utf8(&encoded, &CopyEncoding::Latin1).unwrap();
             assert_eq!(original, decoded);
         }
         
         #[test]
         fn test_latin1_special_chars() {
             // Test characters specific to LATIN1
             let original = "ÄÖÜäöüß";
             let encoded = encode_from_utf8(original, &CopyEncoding::Latin1).unwrap();
             let decoded = decode_to_utf8(&encoded, &CopyEncoding::Latin1).unwrap();
             assert_eq!(original, decoded);
         }
         
         #[test]
         fn test_windows1252_euro() {
             // Euro sign is in WINDOWS-1252 but not LATIN1
             let original = "€100";
             let encoded = encode_from_utf8(original, &CopyEncoding::Windows1252).unwrap();
             let decoded = decode_to_utf8(&encoded, &CopyEncoding::Windows1252).unwrap();
             assert_eq!(original, decoded);
             
             // Should fail for LATIN1
             assert!(encode_from_utf8(original, &CopyEncoding::Latin1).is_err());
         }
         
         #[test]
         fn test_invalid_byte_sequence() {
             // Invalid LATIN1 byte sequence
             let invalid_bytes = vec![0xFF, 0xFE];
             let result = decode_to_utf8(&invalid_bytes, &CopyEncoding::Latin1);
             // LATIN1 accepts all byte values, so this should succeed
             assert!(result.is_ok());
         }
     }
     ```

2. **Create E2E Encoding Tests**
   - **File:** `tests/copy_encoding_e2e_test.py`
   - **Action:** Create comprehensive E2E tests
   - **Details:**
     ```python
     #!/usr/bin/env python3
     """COPY encoding E2E tests"""
     
     import psycopg2
     
     def test_copy_from_latin1():
         conn = psycopg2.connect("host=localhost port=5434 dbname=test user=postgres")
         cur = conn.cursor()
         
         cur.execute("CREATE TABLE test (id INT, name TEXT)")
         
         # LATIN1 encoded data with special characters
         # "José" in LATIN1: 4A 6F 73 E9
         latin1_data = b"COPY test FROM STDIN WITH (FORMAT csv, ENCODING 'LATIN1');\n1,Jos\xe9\n2,Fran\xe7ois\n\\.\n"
         
         cur.copy_expert("COPY test FROM STDIN WITH (FORMAT csv, ENCODING 'LATIN1')", latin1_data)
         conn.commit()
         
         # Verify data was converted to UTF-8 correctly
         cur.execute("SELECT name FROM test ORDER BY id")
         names = [row[0] for row in cur.fetchall()]
         assert names == ['José', 'François']
         
         cur.close()
         conn.close()
     
     def test_copy_to_latin1():
         conn = psycopg2.connect("host=localhost port=5434 dbname=test user=postgres")
         cur = conn.cursor()
         
         cur.execute("CREATE TABLE test (id INT, name TEXT)")
         cur.execute("INSERT INTO test VALUES (1, 'José'), (2, 'François')")
         
         # Export to LATIN1
         output = cur.copy_expert("COPY test TO STDOUT WITH (FORMAT csv, ENCODING 'LATIN1')", "")
         
         # Verify output is LATIN1 encoded
         assert b'Jos\xe9' in output
         assert b'Fran\xe7ois' in output
         
         cur.close()
         conn.close()
     
     def test_copy_roundtrip_latin1():
         conn = psycopg2.connect("host=localhost port=5434 dbname=test user=postgres")
         cur = conn.cursor()
         
         cur.execute("CREATE TABLE test (id INT, name TEXT)")
         cur.execute("INSERT INTO test VALUES (1, 'José'), (2, 'François')")
         
         # Export to LATIN1
         latin1_data = cur.copy_expert("COPY test TO STDOUT WITH (FORMAT csv, ENCODING 'LATIN1')", "")
         
         # Import from LATIN1
         cur.execute("DELETE FROM test")
         cur.copy_expert("COPY test FROM STDIN WITH (FORMAT csv, ENCODING 'LATIN1')", latin1_data)
         conn.commit()
         
         # Verify data matches
         cur.execute("SELECT COUNT(*) FROM test")
         assert cur.fetchone()[0] == 2
         
         cur.execute("SELECT name FROM test ORDER BY id")
         names = [row[0] for row in cur.fetchall()]
         assert names == ['José', 'François']
         
         cur.close()
         conn.close()
     
     def test_copy_windows1252_euro():
         conn = psycopg2.connect("host=localhost port=5434 dbname=test user=postgres")
         cur = conn.cursor()
         
         cur.execute("CREATE TABLE test (id INT, price TEXT)")
         
         # WINDOWS-1252 encoded data with Euro sign
         # Euro in WINDOWS-1252: 0x80
         win1252_data = b"COPY test FROM STDIN WITH (FORMAT csv, ENCODING 'WINDOWS-1252');\n1,\x80100\n\\.\n"
         
         cur.copy_expert("COPY test FROM STDIN WITH (FORMAT csv, ENCODING 'WINDOWS-1252')", win1252_data)
         conn.commit()
         
         # Verify Euro sign was converted correctly
         cur.execute("SELECT price FROM test")
         price = cur.fetchone()[0]
         assert price == '€100'
         
         cur.close()
         conn.close()
     
     def test_copy_encoding_error():
         conn = psycopg2.connect("host=localhost port=5434 dbname=test user=postgres")
         cur = conn.cursor()
         
         cur.execute("CREATE TABLE test (id INT, name TEXT)")
         
         # Try to encode Euro sign in LATIN1 (should fail)
         cur.execute("INSERT INTO test VALUES (1, '€100')")
         
         try:
             output = cur.copy_expert("COPY test TO STDOUT WITH (FORMAT csv, ENCODING 'LATIN1')", "")
             # Should have failed
             assert False, "Expected encoding error"
         except psycopg2.Error as e:
             # Expected error
             assert "encoding" in str(e).lower() or "character" in str(e).lower()
         
         cur.close()
         conn.close()
     ```

3. **Update Documentation**
   - **File:** `docs/copy-command.md`
   - **Action:** Add encoding support documentation
   - **Content:**
     ```markdown
     ## Encoding Support
     
     PGQT supports multiple character encodings for COPY operations.
     
     ### Supported Encodings
     
     | Encoding | Aliases | Description |
     |----------|---------|-------------|
     | UTF8 | UTF-8, UNICODE | Default, recommended |
     | LATIN1 | ISO-8859-1 | Western European |
     | LATIN2 | ISO-8859-2 | Central European |
     | WINDOWS-1252 | WIN-1252 | Windows Western European |
     | WINDOWS-1251 | WIN-1251 | Windows Cyrillic |
     | EUC_JP | EUCJP | Japanese |
     | SHIFT_JIS | SJIS, MS_KANJI | Japanese (Shift JIS) |
     | EUC_KR | EUCKR | Korean |
     | GB18030 | GBK | Chinese (Simplified) |
     | BIG5 | BIG5-HKSCS | Chinese (Traditional) |
     | KOI8-R | KOI8R | Russian |
     | KOI8-U | KOI8U | Ukrainian |
     
     ### Usage Examples
     
     ```sql
     -- Import LATIN1 encoded data
     COPY customers FROM STDIN WITH (FORMAT csv, ENCODING 'LATIN1');
     
     -- Export to WINDOWS-1252
     COPY products TO STDOUT WITH (FORMAT csv, ENCODING 'WINDOWS-1252');
     
     -- Japanese data
     COPY japanese_text FROM STDIN WITH (FORMAT text, ENCODING 'EUC_JP');
     ```
     
     ### Encoding Error Handling
     
     When a character cannot be converted:
     
     - **COPY FROM**: Error if invalid byte sequence for source encoding
     - **COPY TO**: Error if character not representable in target encoding
     
     Future versions may support:
     - `ON_ENCODING_ERROR REPLACE` - Replace with replacement character ()
     - `ON_ENCODING_ERROR IGNORE` - Skip unconvertible characters
     
     ### Performance Impact
     
     Encoding conversion has minimal performance impact:
     - UTF-8: No conversion (native)
     - Single-byte encodings (LATIN1, WINDOWS-1252): ~5% overhead
     - Multi-byte encodings (EUC_JP, GB18030): ~10-15% overhead
     
     ### Best Practices
     
     1. **Use UTF-8 when possible** - Native format, best performance
     2. **Specify encoding explicitly** - Don't rely on defaults
     3. **Validate source data** - Ensure data matches declared encoding
     4. **Test round-trips** - Verify COPY TO/FROM with same encoding
     ```

4. **Update README.md**
   - **File:** `README.md`
   - **Action:** Add encoding support to feature list
   - **Content:**
     ```markdown
     ### COPY Command Support
     
     - ✅ COPY FROM STDIN (TEXT, CSV, BINARY)
     - ✅ COPY TO STDOUT (TEXT, CSV, BINARY)
     - ✅ Multiple encodings (UTF8, LATIN1, WINDOWS-1252, EUC_JP, etc.)
     - ✅ Encoding conversion on import/export
     - ✅ Error reporting with line/column numbers
     ```

5. **Add Performance Benchmarks**
   - **File:** `benches/copy_benchmark.rs`
   - **Action:** Add encoding performance benchmarks
   - **Details:**
     ```rust
     #[bench]
     fn bench_copy_from_latin1(b: &mut Bencher) {
         // Benchmark LATIN1 to UTF-8 conversion
     }
     
     #[bench]
     fn bench_copy_from_eucjp(b: &mut Bencher) {
         // Benchmark EUC_JP to UTF-8 conversion
     }
     
     #[bench]
     fn bench_copy_to_latin1(b: &mut Bencher) {
         // Benchmark UTF-8 to LATIN1 conversion
     }
     ```

#### Checkpoints

- [ ] `cargo check` passes
- [ ] All warnings fixed
- [ ] `./run_tests.sh` passes
- [ ] All E2E encoding tests pass
- [ ] Documentation complete
- [ ] Benchmarks added

---

## Technical Details

### Encoding Conversion Flow

#### COPY FROM (Import)
```
Source File (Encoding X)
    ↓
Read bytes
    ↓
decode_to_utf8(bytes, Encoding X)
    ↓
UTF-8 String
    ↓
Parse (TEXT/CSV/BINARY)
    ↓
SQLite INSERT (UTF-8)
```

#### COPY TO (Export)
```
SQLite SELECT (UTF-8)
    ↓
UTF-8 String
    ↓
Format (TEXT/CSV/BINARY)
    ↓
encode_from_utf8(text, Encoding X)
    ↓
Bytes (Encoding X)
    ↓
Output Stream
```

### Supported Encodings (Priority Order)

**Tier 1 (Must Have):**
- UTF8 (default, no conversion)
- LATIN1 (ISO-8859-1)
- WINDOWS-1252

**Tier 2 (Should Have):**
- LATIN2, LATIN3, LATIN4, LATIN5, LATIN6, LATIN7, LATIN8, LATIN9, LATIN10
- WINDOWS-1250, WINDOWS-1251, WINDOWS-1253, WINDOWS-1254, WINDOWS-1255, WINDOWS-1256, WINDOWS-1257, WINDOWS-1258
- EUC_JP
- SHIFT_JIS

**Tier 3 (Nice to Have):**
- EUC_KR
- GB18030, GBK
- BIG5, BIG5-HKSCS
- KOI8-R, KOI8-U
- ISO-8859-5, ISO-8859-6, ISO-8859-7, ISO-8859-8, ISO-8859-9, ISO-8859-13, ISO-8859-14, ISO-8859-15, ISO-8859-16

### encoding_rs API Usage

```rust
use encoding_rs::{Encoding, UTF_8, LATIN1, WINDOWS_1252};

// Get encoding by name
let encoding = Encoding::for_label("LATIN1".as_bytes()).unwrap();

// Decode to UTF-8
let (cow, _, had_errors) = encoding.decode(bytes);
// cow is Cow<'a, str> - borrowed if no conversion needed, owned if converted

// Encode from UTF-8
let (cow, _, had_errors) = encoding.encode(utf8_string);
// cow is Cow<'a, [u8]> - borrowed if ASCII-only, owned if conversion needed
```

### Error Handling Strategies

1. **Error (Default)**: Fail on first encoding error
   - Pros: Data integrity, explicit errors
   - Cons: May fail on large datasets with minor issues

2. **Replace**: Replace invalid sequences with  (U+FFFD)
   - Pros: Completes operation, visible markers
   - Cons: Data modification, may not be noticeable

3. **Ignore**: Skip invalid byte sequences
   - Pros: Completes operation
   - Cons: Silent data loss, hard to detect

---

## Testing Strategy

### Unit Tests

```rust
#[cfg(test)]
mod encoding_tests {
    use super::*;
    
    #[test]
    fn test_utf8_no_conversion() {
        // UTF-8 to UTF-8 should be no-op
        let text = "Hello, World!";
        let encoded = encode_from_utf8(text, &CopyEncoding::Utf8).unwrap();
        assert_eq!(encoded, text.as_bytes());
    }
    
    #[test]
    fn test_latin1_roundtrip() {
        let original = "Hello, Wörld! ÄÖÜ";
        let encoded = encode_from_utf8(original, &CopyEncoding::Latin1).unwrap();
        let decoded = decode_to_utf8(&encoded, &CopyEncoding::Latin1).unwrap();
        assert_eq!(original, decoded);
    }
    
    #[test]
    fn test_windows1252_euro() {
        let original = "€100";
        let encoded = encode_from_utf8(original, &CopyEncoding::Windows1252).unwrap();
        assert_eq!(encoded, vec![0x80, b'1', b'0', b'0']); // Euro is 0x80 in WIN-1252
        
        let decoded = decode_to_utf8(&encoded, &CopyEncoding::Windows1252).unwrap();
        assert_eq!(original, decoded);
    }
    
    #[test]
    fn test_latin1_cannot_encode_euro() {
        let text = "€100";
        let result = encode_from_utf8(text, &CopyEncoding::Latin1);
        assert!(result.is_err());
    }
    
    #[test]
    fn test_eucjp_roundtrip() {
        let original = "日本語"; // Japanese
        let encoded = encode_from_utf8(original, &CopyEncoding::EucJp).unwrap();
        let decoded = decode_to_utf8(&encoded, &CopyEncoding::EucJp).unwrap();
        assert_eq!(original, decoded);
    }
}
```

### Integration Tests

```rust
#[test]
fn test_copy_from_latin1_integration() {
    let conn = setup_test_db();
    conn.execute("CREATE TABLE test (id INT, name TEXT)", [])?;
    
    // LATIN1 encoded CSV: "1,Jos\xe9\n" where \xe9 is é in LATIN1
    let latin1_data = b"1,Jos\xe9\n2,Fran\xe7ois\n";
    
    let copy_handler = CopyHandler::new(...);
    let mut options = CopyOptions::default();
    options.encoding = CopyEncoding::Latin1;
    options.format = CopyFormat::Csv;
    
    // Set state
    *copy_handler.state.lock().unwrap() = CopyState::FromStdin {
        table_name: "test".to_string(),
        columns: vec!["id".to_string(), "name".to_string()],
        options,
    };
    
    // Process data
    copy_handler.on_copy_data(..., CopyData::new(latin1_data.to_vec()))?;
    copy_handler.on_copy_done(...)?;
    
    // Verify
    let mut stmt = conn.prepare("SELECT name FROM test ORDER BY id")?;
    let names: Vec<String> = stmt.query_map([], |r| r.get(0))?
        .filter_map(|r| r.ok())
        .collect();
    
    assert_eq!(names, vec!["José", "François"]);
}
```

### E2E Tests (Python)

See Phase 4 Task 2 for complete E2E test examples.

---

## Risk Assessment

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| encoding_rs compatibility issues | Low | High | Test on multiple platforms |
| Performance regression | Medium | Medium | Benchmark, optimize hot paths |
| Encoding detection errors | Medium | High | Explicit encoding specification |
| Data corruption | Low | Critical | Extensive testing, validation |
| Memory issues with large conversions | Low | High | Streaming conversion, chunked processing |

---

## Success Criteria

1. **Functionality**
   - ✅ All Tier 1 encodings supported (UTF8, LATIN1, WINDOWS-1252)
   - ✅ COPY FROM with encoding conversion works
   - ✅ COPY TO with encoding conversion works
   - ✅ Round-trip tests pass (TO encoding X, FROM encoding X)

2. **Performance**
   - ✅ UTF-8: No performance impact (native)
   - ✅ Single-byte encodings: <5% overhead
   - ✅ Multi-byte encodings: <15% overhead

3. **Compatibility**
   - ✅ Works with psycopg2 encoding options
   - ✅ Works with psql \encoding command
   - ✅ Compatible with PostgreSQL encoding behavior

4. **Quality**
   - ✅ Zero compiler warnings
   - ✅ All existing tests pass
   - ✅ New encoding tests pass (20+ tests)
   - ✅ Documentation complete

---

## Timeline

| Week | Phase | Deliverables |
|------|-------|--------------|
| 1 | Phase 1 | Core infrastructure, encoding_rs integration |
| 1-2 | Phase 2 | COPY FROM encoding conversion |
| 2 | Phase 3 | COPY TO encoding conversion |
| 3 | Phase 4 | Testing, documentation, benchmarks |

---

## References

- [PostgreSQL Encoding Documentation](https://www.postgresql.org/docs/current/multibyte.html)
- [PostgreSQL Character Set Support](https://www.postgresql.org/docs/current/charset.html)
- [encoding_rs Documentation](https://docs.rs/encoding_rs/latest/encoding_rs/)
- [Unicode Character Encoding](https://www.unicode.org/encodings/)

---

*Last updated: 2026-03-16*
