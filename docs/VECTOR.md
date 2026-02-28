# Vector Search (pgvector Compatibility)

PGlite Proxy provides PostgreSQL pgvector-compatible vector search functionality. This allows you to perform similarity searches on vector embeddings using familiar PostgreSQL syntax.

## Overview

Vector search is essential for:
- **Semantic search**: Finding similar documents based on meaning
- **Recommendation systems**: Finding similar items
- **RAG (Retrieval-Augmented Generation)**: Providing context to LLMs
- **Image similarity**: Finding visually similar images

## Data Types

| PostgreSQL Type | SQLite Storage | Description |
|----------------|----------------|-------------|
| `vector(N)` | TEXT (JSON) | N-dimensional float32 vector |

```sql
CREATE TABLE documents (
    id SERIAL PRIMARY KEY,
    content TEXT,
    embedding VECTOR(1536)  -- OpenAI ada-002 embeddings
);
```

## Distance Functions

### l2_distance(a, b) / vector_l2_distance(a, b)

Calculates the L2 (Euclidean) distance between two vectors. Returns `sqrt(sum((a_i - b_i)^2))`.

```sql
SELECT l2_distance(embedding, '[1, 2, 3]') AS distance
FROM documents
ORDER BY distance
LIMIT 10;
```

### cosine_distance(a, b) / vector_cosine_distance(a, b)

Calculates the cosine distance (1 - cosine similarity) between two vectors. Returns 0 for identical direction, 2 for opposite direction. **Recommended for text embeddings** (OpenAI, Cohere, etc.).

```sql
SELECT cosine_distance(embedding, '[0.1, 0.2, 0.3]') AS distance
FROM documents
ORDER BY distance
LIMIT 10;
```

### inner_product(a, b) / vector_inner_product(a, b)

Calculates the dot product of two vectors. Useful for normalized vectors where higher values indicate more similarity.

```sql
SELECT inner_product(embedding, '[1, 2, 3]') AS similarity
FROM documents
ORDER BY similarity DESC
LIMIT 10;
```

### l1_distance(a, b) / vector_l1_distance(a, b)

Calculates the L1 (Manhattan) distance between two vectors. Returns `sum(|a_i - b_i|)`.

```sql
SELECT l1_distance(embedding, '[1, 2, 3]') AS distance
FROM documents
ORDER BY distance
LIMIT 10;
```

## Vector Operators (pgvector Compatible)

| Operator | Function | Description |
|----------|----------|-------------|
| `<->` | `l2_distance()` | L2 (Euclidean) distance |
| `<=>` | `cosine_distance()` | Cosine distance |
| `<#>` | `inner_product()` | Inner product (for ordering by similarity) |
| `<+>` | `l1_distance()` | L1 (Manhattan) distance |

Example with operators:

```sql
-- Find 5 nearest neighbors using L2 distance
SELECT * FROM items
ORDER BY embedding <-> '[1, 2, 3]'
LIMIT 5;

-- Find 5 most similar using cosine distance
SELECT * FROM items
ORDER BY embedding <=> '[0.1, 0.2, 0.3]'
LIMIT 5;

-- Find 5 most similar using inner product (note: higher is better)
SELECT * FROM items
ORDER BY embedding <#> '[1, 2, 3]' DESC
LIMIT 5;
```

## Utility Functions

### vector_dims(vector)

Returns the number of dimensions in a vector.

```sql
SELECT vector_dims(embedding) FROM documents;
-- Returns: 1536
```

### l2_norm(vector)

Returns the L2 norm (magnitude) of a vector. Formula: `sqrt(sum(x_i^2))`

```sql
SELECT l2_norm(embedding) FROM documents;
-- Returns: 1.0 (for normalized vectors)
```

### l2_normalize(vector)

Returns a unit vector in the same direction. The result has magnitude 1.

```sql
SELECT l2_normalize('[3, 4]');
-- Returns: [0.6, 0.8]
```

### subvector(vector, start, length)

Extracts a subvector using 1-based indexing (PostgreSQL compatible).

```sql
SELECT subvector(embedding, 1, 128) FROM documents;
-- Returns first 128 dimensions

SELECT subvector(embedding, 129, 128) FROM documents;
-- Returns dimensions 129-256
```

### vector_add(a, b)

Adds two vectors element-wise.

```sql
SELECT vector_add('[1, 2, 3]', '[4, 5, 6]');
-- Returns: [5, 7, 9]
```

### vector_sub(a, b)

Subtracts two vectors element-wise.

```sql
SELECT vector_sub('[4, 5, 6]', '[1, 2, 3]');
-- Returns: [3, 3, 3]
```

## Complete Example

### Creating a Vector Table

```sql
-- Create table with vector column
CREATE TABLE articles (
    id SERIAL PRIMARY KEY,
    title TEXT,
    content TEXT,
    embedding VECTOR(384)
);

-- Insert documents with embeddings
INSERT INTO articles (title, content, embedding)
VALUES 
    ('Introduction to AI', 'AI is transforming...', '[0.1, 0.2, 0.3]'),
    ('Machine Learning Basics', 'ML is a subset of...', '[0.15, 0.25, 0.35]');
```

### Similarity Search

```sql
-- Find 5 most similar articles to a query embedding
SELECT 
    id,
    title,
    cosine_distance(embedding, '[0.12, 0.22, 0.32]') AS distance
FROM articles
ORDER BY distance
LIMIT 5;
```

### Hybrid Search (Vector + Full-Text)

```sql
-- Combine vector search with full-text search
SELECT 
    a.id,
    a.title,
    cosine_distance(a.embedding, '[0.12, 0.22, 0.32]') AS vec_distance,
    ts_rank(to_tsvector(a.content), to_tsquery('machine learning')) AS fts_rank
FROM articles a
WHERE to_tsvector(a.content) @@ to_tsquery('machine learning')
ORDER BY vec_distance
LIMIT 10;
```

### Using with OpenAI Embeddings

```sql
-- Create table for document embeddings
CREATE TABLE doc_embeddings (
    id SERIAL PRIMARY KEY,
    document_id INTEGER REFERENCES documents(id),
    embedding VECTOR(1536),  -- OpenAI ada-002 dimension
    created_at TIMESTAMP DEFAULT NOW()
);

-- Insert embedding (from your application)
INSERT INTO doc_embeddings (document_id, embedding)
VALUES (1, '[0.0023, -0.0124, 0.0087, ...]');

-- Search for similar documents
SELECT 
    d.title,
    d.content,
    cosine_distance(e.embedding, :query_embedding) AS distance
FROM doc_embeddings e
JOIN documents d ON e.document_id = d.id
ORDER BY distance
LIMIT 5;
```

## Performance Tips

1. **Use cosine distance for text embeddings**: Most embedding models (OpenAI, Cohere) are optimized for cosine similarity.

2. **Pre-normalize vectors**: If using inner product, normalize vectors first for faster computation.

3. **Filter before ordering**: Use WHERE clauses to reduce the search space.

4. **Consider dimension**: Lower dimensions (384 vs 1536) are faster but less precise.

5. **Batch inserts**: When inserting many vectors, use transactions.

```sql
BEGIN;
INSERT INTO items (embedding) VALUES ('[1,2,3]');
INSERT INTO items (embedding) VALUES ('[4,5,6]');
-- ... more inserts
COMMIT;
```

## Limitations

- **No ANN indexes**: Unlike pgvector, we don't support HNSW or IVFFlat indexes. All searches are exact k-NN (brute force).
- **Scale**: Best for datasets under 1 million vectors. For larger datasets, consider dedicated vector databases like Pinecone, Weaviate, or Milvus.
- **Binary vectors**: Not currently supported.
- **Sparse vectors**: Not currently supported.

## Error Handling

The vector functions will return errors in the following cases:

```sql
-- Dimension mismatch
SELECT l2_distance('[1, 2]', '[1, 2, 3]');
-- Error: vector dimension mismatch: 2 vs 3

-- Zero vector for cosine distance
SELECT cosine_distance('[0, 0]', '[1, 2]');
-- Error: cannot compute cosine distance for zero vector

-- Zero vector for normalization
SELECT l2_normalize('[0, 0]');
-- Error: cannot normalize zero vector

-- Invalid format
SELECT l2_distance('not a vector', '[1, 2]');
-- Error: vector must be in format '[1,2,3]'

-- Subvector out of bounds
SELECT subvector('[1, 2, 3]', 5, 2);
-- Error: start index out of bounds
```

## Compatibility

This implementation is compatible with pgvector SQL syntax, making it easy to migrate applications between PostgreSQL+pgvector and PGlite Proxy.

| Feature | pgvector | PGlite Proxy |
|---------|----------|--------------|
| `vector(N)` type | ✅ | ✅ |
| `<->` L2 distance | ✅ | ✅ |
| `<=>` Cosine distance | ✅ | ✅ |
| `<#>` Inner product | ✅ | ✅ |
| `<+>` L1 distance | ✅ | ✅ |
| `l2_distance()` | ✅ | ✅ |
| `cosine_distance()` | ✅ | ✅ |
| `inner_product()` | ✅ | ✅ |
| `l1_distance()` | ✅ | ✅ |
| `vector_dims()` | ✅ | ✅ |
| `l2_norm()` | ✅ | ✅ |
| `l2_normalize()` | ✅ | ✅ |
| `subvector()` | ✅ | ✅ |
| HNSW index | ✅ | ❌ |
| IVFFlat index | ✅ | ❌ |
| Binary vectors | ✅ | ❌ |
| Sparse vectors | ✅ | ❌ |
