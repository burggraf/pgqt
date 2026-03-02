use pgqt::transpiler::transpile;

// =====================================================
// Vector Type Tests
// =====================================================

#[test]
fn test_transpile_vector_type() {
    let input = "CREATE TABLE items (id SERIAL, embedding VECTOR(3))";
    let result = transpile(input);
    // VECTOR should be mapped to TEXT
    assert!(result.to_lowercase().contains("text"));
    assert!(result.to_lowercase().contains("embedding"));
}

#[test]
fn test_transpile_vector_type_no_dimensions() {
    let input = "CREATE TABLE items (id SERIAL, embedding VECTOR)";
    let result = transpile(input);
    // VECTOR should still be mapped to TEXT
    assert!(result.to_lowercase().contains("text"));
}

// =====================================================
// Distance Function Tests
// =====================================================

#[test]
fn test_transpile_l2_distance_function() {
    let input = "SELECT l2_distance(embedding, '[1,2,3]') FROM items";
    let result = transpile(input);
    assert!(result.contains("l2_distance"));
}

#[test]
fn test_transpile_cosine_distance_function() {
    let input = "SELECT cosine_distance(a, b) FROM vectors";
    let result = transpile(input);
    assert!(result.contains("cosine_distance"));
}

#[test]
fn test_transpile_inner_product_function() {
    let input = "SELECT inner_product(a, b) FROM vectors";
    let result = transpile(input);
    assert!(result.contains("inner_product"));
}

#[test]
fn test_transpile_l1_distance_function() {
    let input = "SELECT l1_distance(a, b) FROM vectors";
    let result = transpile(input);
    assert!(result.contains("l1_distance"));
}

// =====================================================
// Vector Operator Tests (pgvector compatible)
// =====================================================

#[test]
fn test_transpile_l2_distance_operator() {
    let input = "SELECT * FROM items ORDER BY embedding <-> '[1,2,3]' LIMIT 5";
    let result = transpile(input);
    assert!(result.contains("vector_l2_distance"));
}

#[test]
fn test_transpile_cosine_distance_operator() {
    let input = "SELECT * FROM items ORDER BY embedding <=> '[1,2,3]' LIMIT 5";
    let result = transpile(input);
    assert!(result.contains("vector_cosine_distance"));
}

#[test]
fn test_transpile_inner_product_operator() {
    let input = "SELECT * FROM items ORDER BY embedding <#> '[1,2,3]' LIMIT 5";
    let result = transpile(input);
    assert!(result.contains("vector_inner_product"));
}

#[test]
fn test_transpile_l1_distance_operator() {
    let input = "SELECT * FROM items ORDER BY embedding <+> '[1,2,3]' LIMIT 5";
    let result = transpile(input);
    assert!(result.contains("vector_l1_distance"));
}

// =====================================================
// Utility Function Tests
// =====================================================

#[test]
fn test_transpile_vector_dims() {
    let input = "SELECT vector_dims(embedding) FROM items";
    let result = transpile(input);
    assert!(result.contains("vector_dims"));
}

#[test]
fn test_transpile_l2_norm() {
    let input = "SELECT l2_norm(embedding) FROM items";
    let result = transpile(input);
    assert!(result.contains("l2_norm"));
}

#[test]
fn test_transpile_l2_normalize() {
    let input = "SELECT l2_normalize(embedding) FROM items";
    let result = transpile(input);
    assert!(result.contains("l2_normalize"));
}

#[test]
fn test_transpile_subvector() {
    let input = "SELECT subvector(embedding, 1, 3) FROM items";
    let result = transpile(input);
    assert!(result.contains("subvector"));
}

#[test]
fn test_transpile_vector_add() {
    let input = "SELECT vector_add(a, b) FROM vectors";
    let result = transpile(input);
    assert!(result.contains("vector_add"));
}

#[test]
fn test_transpile_vector_sub() {
    let input = "SELECT vector_sub(a, b) FROM vectors";
    let result = transpile(input);
    assert!(result.contains("vector_sub"));
}

// =====================================================
// Complex Query Tests
// =====================================================

#[test]
fn test_transpile_vector_in_order_by() {
    let input = "SELECT * FROM items ORDER BY l2_distance(embedding, '[1,2,3]') LIMIT 5";
    let result = transpile(input);
    assert!(result.contains("l2_distance"));
    assert!(result.to_lowercase().contains("order by"));
}

#[test]
fn test_transpile_vector_in_where_clause() {
    let input = "SELECT * FROM items WHERE l2_distance(embedding, '[1,2,3]') < 0.5";
    let result = transpile(input);
    assert!(result.contains("l2_distance"));
    assert!(result.to_lowercase().contains("where"));
}

#[test]
fn test_transpile_vector_with_alias() {
    let input = "SELECT id, l2_distance(embedding, '[1,2,3]') AS distance FROM items";
    let result = transpile(input);
    assert!(result.contains("l2_distance"));
    assert!(result.to_lowercase().contains("distance"));
}

#[test]
fn test_transpile_vector_with_join() {
    let input = r#"
        SELECT a.id, b.id, l2_distance(a.embedding, b.embedding) AS dist
        FROM items a, items b
        WHERE l2_distance(a.embedding, b.embedding) < 0.5
    "#;
    let result = transpile(input);
    assert!(result.contains("l2_distance"));
}

#[test]
fn test_transpile_vector_insert() {
    let input = "INSERT INTO items (embedding) VALUES ('[1,2,3]')";
    let result = transpile(input);
    assert!(result.to_lowercase().contains("insert"));
    assert!(result.contains("[1,2,3]"));
}

#[test]
fn test_transpile_vector_update() {
    let input = "UPDATE items SET embedding = '[4,5,6]' WHERE id = 1";
    let result = transpile(input);
    assert!(result.to_lowercase().contains("update"));
    assert!(result.contains("[4,5,6]"));
}

// =====================================================
// Combination with Other Features Tests
// =====================================================

#[test]
fn test_transpile_vector_with_fts() {
    // Combining vector search with full-text search
    let input = r#"
        SELECT id, title, 
               l2_distance(embedding, '[1,2,3]') AS vec_dist,
               ts_rank(to_tsvector(content), to_tsquery('hello')) AS fts_rank
        FROM documents
        WHERE to_tsvector(content) @@ to_tsquery('hello')
        ORDER BY vec_dist
        LIMIT 10
    "#;
    let result = transpile(input);
    assert!(result.contains("l2_distance"));
    assert!(result.contains("fts_match") || result.contains("@@"));
}

#[test]
fn test_transpile_vector_with_filter() {
    let input = r#"
        SELECT * FROM items 
        WHERE category = 'electronics' 
          AND l2_distance(embedding, '[1,2,3]') < 0.5
        ORDER BY l2_distance(embedding, '[1,2,3]')
        LIMIT 10
    "#;
    let result = transpile(input);
    assert!(result.contains("l2_distance"));
    assert!(result.to_lowercase().contains("category"));
}
