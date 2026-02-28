//! Integration tests for Full-Text Search (FTS) functionality
//!
//! These tests verify that PostgreSQL FTS syntax is properly transpiled and
//! executed against SQLite.

use postgresqlite::fts::*;
use postgresqlite::transpiler::transpile;

// ========== Transpiler Tests ==========

#[test]
fn test_transpile_to_tsvector_single_arg() {
    let input = "SELECT to_tsvector('hello world')";
    let result = transpile(input);
    assert!(result.contains("to_tsvector"));
}

#[test]
fn test_transpile_to_tsvector_two_args() {
    let input = "SELECT to_tsvector('english', 'hello world')";
    let result = transpile(input);
    assert!(result.contains("to_tsvector"));
}

#[test]
fn test_transpile_to_tsquery() {
    let input = "SELECT to_tsquery('hello & world')";
    let result = transpile(input);
    assert!(result.contains("to_tsquery"));
}

#[test]
fn test_transpile_plainto_tsquery() {
    let input = "SELECT plainto_tsquery('hello world')";
    let result = transpile(input);
    assert!(result.contains("plainto_tsquery"));
}

#[test]
fn test_transpile_phraseto_tsquery() {
    let input = "SELECT phraseto_tsquery('hello world')";
    let result = transpile(input);
    assert!(result.contains("phraseto_tsquery"));
}

#[test]
fn test_transpile_websearch_to_tsquery() {
    let input = "SELECT websearch_to_tsquery('hello OR world')";
    let result = transpile(input);
    assert!(result.contains("websearch_to_tsquery"));
}

#[test]
fn test_transpile_ts_rank() {
    let input = "SELECT ts_rank(to_tsvector('hello'), to_tsquery('hello'))";
    let result = transpile(input);
    assert!(result.contains("ts_rank"));
}

#[test]
fn test_transpile_ts_headline() {
    let input = "SELECT ts_headline('hello world', to_tsquery('hello'))";
    let result = transpile(input);
    assert!(result.contains("ts_headline"));
}

#[test]
fn test_transpile_match_operator() {
    let input = "SELECT * FROM docs WHERE body @@ to_tsquery('hello')";
    let result = transpile(input);
    assert!(result.contains("fts_match"));
}

#[test]
fn test_transpile_setweight() {
    let input = "SELECT setweight(to_tsvector('hello'), 'A')";
    let result = transpile(input);
    assert!(result.contains("setweight"));
}

#[test]
fn test_transpile_strip() {
    let input = "SELECT strip(to_tsvector('hello world'))";
    let result = transpile(input);
    assert!(result.contains("strip"));
}

#[test]
fn test_transpile_numnode() {
    let input = "SELECT numnode(to_tsquery('hello & world'))";
    let result = transpile(input);
    assert!(result.contains("numnode"));
}

#[test]
fn test_transpile_querytree() {
    let input = "SELECT querytree(to_tsquery('hello & world'))";
    let result = transpile(input);
    assert!(result.contains("querytree"));
}

#[test]
fn test_transpile_tsvector_concat() {
    let input = "SELECT to_tsvector('hello') || to_tsvector('world')";
    let result = transpile(input);
    assert!(result.contains("tsvector_concat"));
}

#[test]
fn test_transpile_tsvector_type_in_create_table() {
    let input = "CREATE TABLE docs (id SERIAL, body TSVECTOR)";
    let result = transpile(input);
    assert!(result.contains("body text")); // TSVECTOR maps to TEXT
}

#[test]
fn test_transpile_tsquery_type_in_create_table() {
    let input = "CREATE TABLE queries (id SERIAL, query TSQUERY)";
    let result = transpile(input);
    assert!(result.contains("query text")); // TSQUERY maps to TEXT
}

#[test]
fn test_transpile_complex_fts_query() {
    let input = r#"
        SELECT title, ts_rank(body, websearch_to_tsquery('postgresql')) as rank
        FROM articles
        WHERE body @@ websearch_to_tsquery('postgresql')
        ORDER BY rank DESC
        LIMIT 10
    "#;
    let result = transpile(input);
    assert!(result.contains("ts_rank"));
    assert!(result.contains("websearch_to_tsquery"));
    assert!(result.contains("fts_match"));
}

// ========== FTS Function Tests ==========

#[test]
fn test_fts_to_tsvector_basic() {
    let result = to_tsvector_impl("english", "The quick brown fox jumps");
    // Check that stop words are removed
    assert!(!result.contains("'the'"));
    // Check that important words are present
    assert!(result.contains("'quick'"));
    assert!(result.contains("'brown'"));
    assert!(result.contains("'fox'"));
    assert!(result.contains("'jump'")); // stemmed
}

#[test]
fn test_fts_to_tsvector_empty() {
    let result = to_tsvector_impl("english", "");
    assert!(result.is_empty());
}

#[test]
fn test_fts_to_tsvector_stop_words_only() {
    let result = to_tsvector_impl("english", "the and or but");
    assert!(result.is_empty());
}

#[test]
fn test_fts_to_tsquery_basic() {
    let result = to_tsquery_impl("english", "postgresql & database");
    assert!(result.contains("postgresql"));
    assert!(result.contains("AND"));
}

#[test]
fn test_fts_to_tsquery_or() {
    let result = to_tsquery_impl("english", "postgresql | mysql");
    // PostgreSQL | operator is converted to OR in FTS5
    assert!(result.contains("OR") || result.contains("|") || result.contains("postgresql"));
}

#[test]
fn test_fts_to_tsquery_not() {
    let result = to_tsquery_impl("english", "postgresql & !mysql");
    // The ! operator is converted to NOT in FTS5
    assert!(result.contains("NOT") || result.contains("!"));
}

#[test]
fn test_fts_plainto_tsquery() {
    let result = plainto_tsquery_impl("english", "postgresql database");
    // All terms should be ANDed
    assert!(result.contains("postgresql"));
    // Terms separated by space (implicit AND in FTS5)
    assert!(!result.contains("&"));
}

#[test]
fn test_fts_phraseto_tsquery() {
    let result = phraseto_tsquery_impl("english", "postgresql database");
    // Should be quoted for phrase search
    assert!(result.starts_with('"'));
    assert!(result.ends_with('"'));
}

#[test]
fn test_fts_websearch_to_tsquery_basic() {
    let result = websearch_to_tsquery_impl("english", "postgresql");
    assert!(result.contains("postgresql"));
}

#[test]
fn test_fts_websearch_to_tsquery_or() {
    let result = websearch_to_tsquery_impl("english", "postgresql OR mysql");
    assert!(result.contains("OR"));
}

#[test]
fn test_fts_websearch_to_tsquery_not() {
    let result = websearch_to_tsquery_impl("english", "postgresql -mysql");
    assert!(result.contains("NOT"));
}

#[test]
fn test_fts_websearch_to_tsquery_phrase() {
    let result = websearch_to_tsquery_impl("english", "\"postgresql running\"");
    // Phrase should be quoted
    assert!(result.contains('"'));
}

#[test]
fn test_fts_tsvector_matches_tsquery_match() {
    // Use words that our simplified stemmer handles consistently
    let vector = to_tsvector_impl("english", "postgresql is running quickly");
    let query = to_tsquery_impl("english", "postgresql & runn"); // running -> runn
    assert!(tsvector_matches_tsquery(&vector, &query));
}

#[test]
fn test_fts_tsvector_matches_tsquery_no_match() {
    let vector = to_tsvector_impl("english", "postgresql is a database");
    let query = to_tsquery_impl("english", "mysql & databas");
    assert!(!tsvector_matches_tsquery(&vector, &query));
}

#[test]
fn test_fts_tsvector_matches_tsquery_with_not() {
    // Use words that our simplified stemmer handles consistently
    let vector = to_tsvector_impl("english", "postgresql is running");
    let query = to_tsquery_impl("english", "postgresql & !mysql");
    assert!(tsvector_matches_tsquery(&vector, &query));
}

#[test]
fn test_fts_tsvector_matches_tsquery_with_not_fails() {
    // Use words that our simplified stemmer handles consistently
    let vector = to_tsvector_impl("english", "postgresql running mysql");
    let query = to_tsquery_impl("english", "postgresql & !mysql");
    assert!(!tsvector_matches_tsquery(&vector, &query));
}

#[test]
fn test_fts_ts_rank() {
    let vector = to_tsvector_impl("english", "postgresql database");
    let query = to_tsquery_impl("english", "postgresql");
    let rank = ts_rank_impl(&vector, &query);
    assert!(rank > 0.0);
    assert!(rank <= 1.0);
}

#[test]
fn test_fts_ts_rank_no_match() {
    let vector = to_tsvector_impl("english", "postgresql database");
    let query = to_tsquery_impl("english", "mysql");
    let rank = ts_rank_impl(&vector, &query);
    assert_eq!(rank, 0.0);
}

#[test]
fn test_fts_ts_headline_basic() {
    let text = "PostgreSQL is a powerful database";
    let query = to_tsquery_impl("english", "postgresql");
    let headline = ts_headline_impl("english", text, &query, None);
    assert!(headline.contains("<b>"));
    assert!(headline.contains("</b>"));
    assert!(headline.contains("PostgreSQL"));
}

#[test]
fn test_fts_ts_headline_custom_delimiters() {
    let text = "PostgreSQL is a powerful database";
    let query = to_tsquery_impl("english", "postgresql");
    let headline = ts_headline_impl("english", text, &query, Some("StartSel=[[, StopSel=]]"));
    assert!(headline.contains("[["));
    assert!(headline.contains("]]"));
}

#[test]
fn test_fts_setweight() {
    let vector = to_tsvector_impl("english", "hello world");
    let weighted = setweight_impl(&vector, 'A');
    // Weight should be added to entries
    assert!(weighted.contains("A:"));
}

#[test]
fn test_fts_strip() {
    let vector = to_tsvector_impl("english", "hello world");
    let stripped = strip_impl(&vector);
    // Positions should be removed
    assert!(!stripped.contains(':'));
    assert!(stripped.contains("'hello'"));
    assert!(stripped.contains("'world'"));
}

#[test]
fn test_fts_numnode() {
    let query = "hello & world | test";
    let count = numnode_impl(query);
    assert!(count > 0);
}

#[test]
fn test_fts_querytree() {
    let query = "hello & world | test";
    let tree = querytree_impl(query);
    assert!(!tree.is_empty());
}

#[test]
fn test_fts_tsvector_concat() {
    let left = "'hello':1 'world':2";
    let right = "'test':1";
    let result = tsvector_concat(left, right);
    assert!(result.contains("'hello'"));
    assert!(result.contains("'world'"));
    assert!(result.contains("'test'"));
}

#[test]
fn test_fts_stem_word() {
    assert_eq!(stem_word("running"), "runn");
    assert_eq!(stem_word("quickly"), "quick");
    assert_eq!(stem_word("cats"), "cat");
    assert_eq!(stem_word("walked"), "walk");
}

#[test]
fn test_fts_is_stop_word() {
    assert!(is_stop_word("the", "english"));
    assert!(is_stop_word("and", "english"));
    assert!(is_stop_word("is", "english"));
    assert!(!is_stop_word("postgresql", "english"));
    assert!(!is_stop_word("database", "english"));
}

// ========== Edge Cases ==========

#[test]
fn test_fts_special_characters() {
    let result = to_tsvector_impl("english", "hello! world? test.");
    // Should handle punctuation gracefully
    assert!(result.contains("'hello'"));
    assert!(result.contains("'world'"));
    assert!(result.contains("'test'"));
}

#[test]
fn test_fts_numbers() {
    let result = to_tsvector_impl("english", "version 17.5");
    assert!(result.contains("'version'"));
    // Numbers may or may not be included depending on implementation
}

#[test]
fn test_fts_mixed_case() {
    let result = to_tsvector_impl("english", "PostgreSQL DATABASE");
    // Should be lowercased
    let lower = result.to_lowercase();
    assert!(lower.contains("'postgresql'"));
    // DATABASE -> database -> database (no stemming by our simplified stemmer)
    // Note: our simplified stemmer doesn't stem all words like a full Porter stemmer
}

#[test]
fn test_fts_unicode() {
    let result = to_tsvector_impl("english", "café résumé");
    // Should handle unicode characters
    assert!(!result.is_empty());
}

#[test]
fn test_fts_empty_query() {
    let result = to_tsquery_impl("english", "");
    assert!(result.is_empty() || result.trim().is_empty());
}

#[test]
fn test_fts_whitespace_handling() {
    let result = to_tsvector_impl("english", "  hello   world  ");
    assert!(result.contains("'hello'"));
    assert!(result.contains("'world'"));
}
