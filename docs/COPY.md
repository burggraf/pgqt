# COPY Command Support

PGQT provides full support for the PostgreSQL `COPY` command, allowing efficient data import and export between PostgreSQL clients and the underlying SQLite database.

## Supported Commands

- `COPY table_name FROM STDIN`: Import data from the client.
- `COPY table_name TO STDOUT`: Export data to the client.
- `COPY (query) TO STDOUT`: Export results of a query to the client.

## Supported Formats

### Text Format (Default)
The default PostgreSQL text format uses tab-delimited columns and newline-separated rows.
- **Delimiter**: `\t` (Tab)
- **Null representation**: `\N`
- **Escape character**: `\` (Backslash)

### CSV Format
Standard Comma-Separated Values format.
- **Delimiter**: `,` (Comma)
- **Quote character**: `"` (Double quote)
- **Null representation**: Empty string (default)
- **Header**: Optional header row support.

### Binary Format
The PostgreSQL binary format is also supported for both import and export, providing the most efficient data transfer.

## Supported Options

Options can be specified using the `WITH` clause:

```sql
COPY table_name FROM STDIN WITH (
    FORMAT CSV,
    DELIMITER ',',
    HEADER,
    NULL 'NULL',
    ENCODING 'UTF8'
);
```

| Option | Values | Description |
| :--- | :--- | :--- |
| **FORMAT** | `TEXT`, `CSV`, `BINARY` | Data format |
| **DELIMITER** | char | Character that separates columns |
| **QUOTE** | char | Character used for quoting fields (CSV only) |
| **ESCAPE** | char | Character used for escaping (CSV only) |
| **NULL** | string | String that represents a null value |
| **HEADER** | - | If specified, the first line is treated as a header row |
| **ENCODING** | string | Character set encoding (default UTF8) |

## Implementation Details

The `COPY` command is implemented as a special sub-protocol within the PostgreSQL wire protocol. PGQT handles the state transitions and data streaming required by the protocol:

1. **Transpilation**: The `COPY` statement is parsed by `pg_query` to extract options, table names, and column lists.
2. **Protocol Negotiation**: The proxy sends a `CopyInResponse` or `CopyOutResponse` message to the client to initiate data transfer.
3. **Data Processing**: 
   - For `COPY FROM`, the proxy buffers incoming `CopyData` messages and parses them based on the selected format before performing batch inserts into SQLite.
   - For `COPY TO`, the proxy executes the corresponding query in SQLite and streams the rows back to the client as `CopyData` messages.

## Examples

### Import CSV data using psql
```bash
cat data.csv | psql -h localhost -c "COPY my_table FROM STDIN WITH (FORMAT CSV)"
```

### Export to a file using psql
```bash
psql -h localhost -c "COPY my_table TO STDOUT" > data.txt
```

### Export query results to CSV
```bash
psql -h localhost -c "COPY (SELECT name, email FROM users WHERE active = true) TO STDOUT WITH (FORMAT CSV, HEADER)" > active_users.csv
```
