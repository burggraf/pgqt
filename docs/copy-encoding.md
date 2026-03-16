# COPY Encoding Support

PGQT supports comprehensive character encoding for COPY operations, allowing data import/export in various encodings.

## Supported Encodings

| Encoding | Aliases | Description |
|----------|---------|-------------|
| UTF8 | UTF-8, UNICODE | Default encoding |
| LATIN1 | ISO-8859-1 | Western European |
| LATIN2 | ISO-8859-2 | Central European |
| LATIN3 | ISO-8859-3 | South European |
| LATIN4 | ISO-8859-4 | North European |
| LATIN5 | ISO-8859-9 | Turkish |
| LATIN6 | ISO-8859-10 | Nordic |
| LATIN7 | ISO-8859-13 | Baltic |
| LATIN8 | ISO-8859-14 | Celtic |
| LATIN9 | ISO-8859-15 | Western European (with Euro) |
| LATIN10 | ISO-8859-16 | South-Eastern European |
| WINDOWS-1250 | WIN1250, CP1250 | Windows Central European |
| WINDOWS-1251 | WIN1251, CP1251 | Windows Cyrillic |
| WINDOWS-1252 | WIN1252, CP1252 | Windows Western European |
| WINDOWS-1253 | WIN1253, CP1253 | Windows Greek |
| WINDOWS-1254 | WIN1254, CP1254 | Windows Turkish |
| WINDOWS-1255 | WIN1255, CP1255 | Windows Hebrew |
| WINDOWS-1256 | WIN1256, CP1256 | Windows Arabic |
| WINDOWS-1257 | WIN1257, CP1257 | Windows Baltic |
| WINDOWS-1258 | WIN1258, CP1258 | Windows Vietnamese |
| EUC_JP | EUCJP | Japanese (EUC) |
| SHIFT_JIS | SJIS, MS_KANJI | Japanese (Shift JIS) |
| EUC_KR | EUCKR, KSC5601 | Korean |
| GB18030 | | Chinese (Simplified) |
| GBK | CP936 | Chinese (Simplified, legacy) |
| BIG5 | BIG5-HKSCS | Chinese (Traditional) |
| KOI8-R | KOI8R | Russian |
| KOI8-U | KOI8U | Ukrainian |
| SQL_ASCII | | No encoding conversion (treats bytes as-is) |

## Usage Examples

### Importing LATIN1 Data

```sql
-- Import data from a LATIN1 encoded file
COPY customers FROM STDIN WITH (FORMAT csv, ENCODING 'LATIN1');
1,José
2,François
3,München
\.
```

### Exporting to WINDOWS-1252

```sql
-- Export data to WINDOWS-1252 encoding (supports Euro sign)
COPY products TO STDOUT WITH (FORMAT csv, ENCODING 'WINDOWS-1252');
```

### Japanese Text in EUC-JP

```sql
-- Import Japanese text in EUC-JP encoding
COPY japanese_text FROM STDIN WITH (FORMAT text, ENCODING 'EUC_JP');
```

### Cyrillic Text

```sql
-- Import Russian text in WINDOWS-1251
COPY russian_data FROM STDIN WITH (FORMAT csv, ENCODING 'WINDOWS-1251');
```

## Encoding Names

Encoding names are case-insensitive and support multiple aliases:

- `UTF8`, `utf-8`, `UTF-8`, `UNICODE` → UTF-8
- `LATIN1`, `ISO-8859-1`, `ISO88591` → LATIN1
- `WINDOWS-1252`, `WIN-1252`, `WIN1252`, `CP1252` → WINDOWS-1252
- `EUC_JP`, `EUC-JP`, `EUCCPJP` → EUC_JP

## Error Handling

### COPY FROM (Import)

When importing data with a specific encoding:
- Invalid byte sequences for the specified encoding will result in an error
- The error message includes the encoding name and line number

Example error:
```
ERROR: COPY test_table: encoding error: Invalid byte sequence for encoding LATIN1
```

### COPY TO (Export)

When exporting data to a specific encoding:
- Characters that cannot be represented in the target encoding will result in an error
- For example, the Euro sign (€) cannot be encoded in LATIN1

Example error:
```
ERROR: COPY encoding error: Character cannot be encoded in LATIN1
```

## Performance Impact

Encoding conversion has minimal performance impact:

| Encoding Type | Overhead |
|---------------|----------|
| UTF-8 | No overhead (native format) |
| Single-byte encodings (LATIN1, WINDOWS-1252) | ~5% overhead |
| Multi-byte encodings (EUC_JP, GB18030) | ~10-15% overhead |

## Best Practices

1. **Use UTF-8 when possible** - Native format, best performance, widest character support
2. **Specify encoding explicitly** - Don't rely on defaults when working with non-UTF8 data
3. **Validate source data** - Ensure data matches declared encoding before import
4. **Test round-trips** - Verify COPY TO/FROM with same encoding preserves data
5. **Use WINDOWS-1252 instead of LATIN1** if you need Euro sign (€) support

## Implementation Details

PGQT uses the [`encoding_rs`](https://docs.rs/encoding_rs/) library for encoding conversion, which is a high-performance encoding library used by Firefox.

### Conversion Flow

**COPY FROM:**
```
Source Bytes (Encoding X) → Decode to UTF-8 → Parse → Store in SQLite (UTF-8)
```

**COPY TO:**
```
SQLite (UTF-8) → Format → Encode to Encoding X → Output Bytes
```

### Special Encodings

- **UTF-8**: No conversion performed (pass-through)
- **LATIN1**: Direct byte-to-character mapping (0x00-0xFF maps to U+0000-U+00FF)
- **SQL_ASCII**: No conversion, treats each byte as a character

## Compatibility

The encoding support is compatible with PostgreSQL's behavior:
- Same encoding names and aliases
- Same error handling semantics
- Compatible with `psql` and `pg_dump` encoding options
- Compatible with `psycopg2` encoding parameters

## See Also

- [PostgreSQL Character Set Support](https://www.postgresql.org/docs/current/charset.html)
- [encoding_rs Documentation](https://docs.rs/encoding_rs/)
- [COPY Command Documentation](copy-command.md)
