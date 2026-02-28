# Full-Text Search (FTS) Support

PGlite Proxy provides PostgreSQL-compatible full-text search functionality using SQLite's FTS5 extension under the hood. This allows you to use PostgreSQL FTS syntax with your SQLite database.

## Overview

The FTS implementation provides:

- **Type mapping**: `TSVECTOR` and `TSQUERY` types are stored as `TEXT` in SQLite
- **Query translation**: PostgreSQL FTS operators are translated to SQLite equivalents
- **Function emulation**: Core PostgreSQL FTS functions are implemented as SQLite scalar functions
- **Match operator**: The `@@` operator for FTS matching

## Supported Features

### Data Types

| PostgreSQL Type | SQLite Storage | Description |
|-----------------|----------------|-------------|
| `TSVECTOR` | `TEXT` | Sorted list of lexemes with positions |
| `TSQUERY` | `TEXT` | Search query with boolean operators |

### Operators

| Operator | Description | Example |
|----------|-------------|---------|
| `@@` | Match (tsvector against tsquery) | `body @@ to_tsquery('hello')` |
| `&` | AND (in tsquery) | `'hello & world'` |
| `\|` | OR (in tsquery) | `'hello \| world'` |
| `!` | NOT (in tsquery) | `'hello & !world'` |
| `<->` | Phrase search | `'hello <-> world'` |
| `\|\|` | Concatenate tsvectors | `tsvector1 \|\| tsvector2` |
| `@>` | Contains | `query1 @> query2` |
| `<@` | Contained by | `query1 <@ query2` |

### Functions

#### Document Processing

| Function | Description |
|----------|-------------|
| `to_tsvector([config,] text)` | Convert text to tsvector |
| `setweight(tsvector, char)` | Set weight (A, B, C, D) on tsvector |
| `strip(tsvector)` | Remove positions and weights from tsvector |
| `array_to_tsvector(text[])` | Convert array to tsvector |

#### Query Building

| Function | Description |
|----------|-------------|
| `to_tsquery([config,] text)` | Convert text to tsquery (requires operators) |
| `plainto_tsquery([config,] text)` | Convert plain text to tsquery (ANDs all terms) |
| `phraseto_tsquery([config,] text)` | Convert phrase to tsquery (phrase search) |
| `websearch_to_tsquery([config,] text)` | Convert web-style query (supports OR, -, "") |

#### Ranking and Highlighting

| Function | Description |
|----------|-------------|
| `ts_rank(tsvector, tsquery)` | Calculate rank based on term frequency |
| `ts_rank_cd(tsvector, tsquery)` | Cover density ranking |
| `ts_headline([config,] text, tsquery [, options])` | Return highlighted snippet |

#### Utility Functions

| Function | Description |
|----------|-------------|
| `numnode(tsquery)` | Count nodes in tsquery |
| `querytree(tsquery)` | Get query tree representation |

## Usage Examples

### Basic Full-Text Search

```sql
-- Create a table with a tsvector column
CREATE TABLE articles (
    id SERIAL PRIMARY KEY,
    title TEXT,
    body TEXT,
    search_vector TSVECTOR
);

-- Insert data
INSERT INTO articles (title, body, search_vector)
VALUES (
    'PostgreSQL Guide',
    'PostgreSQL is a powerful database system',
    to_tsvector('english', 'PostgreSQL is a powerful database system')
);

-- Search using @@ operator
SELECT * FROM articles 
WHERE search_vector @@ to_tsquery('english', 'postgresql & database');

-- Search with ranking
SELECT title, ts_rank(search_vector, to_tsquery('postgresql')) as rank
FROM articles
WHERE search_vector @@ to_tsquery('postgresql')
ORDER BY rank DESC;
```

### Web-Style Search

```sql
-- Use websearch_to_tsquery for Google-style queries
SELECT * FROM articles
WHERE search_vector @@ websearch_to_tsquery('postgresql OR mysql');

-- Exclude terms with minus
SELECT * FROM articles
WHERE search_vector @@ websearch_to_tsquery('database -mysql');

-- Phrase search with quotes
SELECT * FROM articles
WHERE search_vector @@ websearch_to_tsquery('"powerful database"');
```

### Highlighting Results

```sql
-- Highlight matching terms in results
SELECT 
    title,
    ts_headline('english', body, to_tsquery('postgresql')) as highlighted_body
FROM articles
WHERE search_vector @@ to_tsquery('postgresql');

-- Custom highlight delimiters
SELECT 
    ts_headline('english', body, to_tsquery('postgresql'), 
                'StartSel=<mark>, StopSel=</mark>') as highlighted
FROM articles;
```

### Weighted Search

```sql
-- Assign weights to different parts of the document
SELECT 
    setweight(to_tsvector('english', title), 'A') || 
    setweight(to_tsvector('english', body), 'B') as weighted_vector
FROM articles;

-- Weight A is most important, D is least
-- This affects ranking calculations
```

## Configuration

### Text Search Configurations

Currently supported configurations:
- `english` (default) - English stemming and stop words
- Other configurations fall back to basic tokenization

### Stop Words

The English configuration includes common English stop words that are automatically removed:
- Articles: a, an, the
- Conjunctions: and, or, but
- Prepositions: in, on, at, to, for
- Pronouns: he, she, it, they, etc.
- Common verbs: is, are, was, were, be, etc.

## Implementation Notes

### Stemming

The implementation uses a simplified Porter stemmer for English. This handles common word forms:
- `running` → `runn`
- `quickly` → `quick`
- `cats` → `cat`

For production use requiring exact PostgreSQL compatibility, consider using a full Porter stemmer implementation.

### Query Translation

PostgreSQL FTS queries are translated to SQLite FTS5 syntax:

| PostgreSQL | SQLite FTS5 |
|------------|-------------|
| `&` | `AND` |
| `\|` | `OR` |
| `!` | `NOT` |
| `<->` | Phrase (quoted) |

### Performance Considerations

1. **Indexing**: For large datasets, create FTS5 virtual tables on your text columns
2. **Pre-computed vectors**: Store pre-computed tsvector values instead of computing on-the-fly
3. **Limit results**: Use `LIMIT` with `ts_rank` to avoid ranking large result sets

## Limitations

### Current Limitations

1. **Stemmer**: Simplified stemmer, not full Porter stemmer
2. **Languages**: Only English stop words are built-in
3. **Dictionaries**: Custom dictionaries not supported
4. **GIN/GiST indexes**: Use FTS5 virtual tables instead

### PostgreSQL Features Not Implemented

- `ts_debug()` - Debugging function
- `ts_stat()` - Statistics function
- `ts_rewrite()` - Query rewriting
- Custom text search configurations
- Thesaurus dictionaries
- Ispell dictionaries

## Compatibility Matrix

| Feature | Support | Notes |
|---------|---------|-------|
| `to_tsvector()` | ✅ | Simplified stemming |
| `to_tsquery()` | ✅ | Operator translation |
| `plainto_tsquery()` | ✅ | - |
| `phraseto_tsquery()` | ✅ | - |
| `websearch_to_tsquery()` | ✅ | - |
| `@@` operator | ✅ | - |
| `ts_rank()` | ✅ | Simplified algorithm |
| `ts_rank_cd()` | ✅ | Same as ts_rank |
| `ts_headline()` | ✅ | - |
| `setweight()` | ✅ | - |
| `strip()` | ✅ | - |
| `\|\|` (concat) | ✅ | - |
| `@>` / `<@` | ✅ | Simplified check |
| `numnode()` | ✅ | - |
| `querytree()` | ✅ | - |
| Custom configs | ⚠️ | Falls back to basic |
| Multiple languages | ⚠️ | Only English stop words |
