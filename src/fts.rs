//! Full-Text Search (FTS) module for PostgreSQL compatibility
//!
//! This module implements PostgreSQL's full-text search functionality using SQLite's FTS5
//! extension. It provides:

// These functions are part of the public FTS API
#![allow(dead_code)]
//!
//! - Type mapping: TSVECTOR → TEXT, TSQUERY → TEXT
//! - Query translation: PostgreSQL FTS syntax → SQLite FTS5 syntax
//! - Function emulation: to_tsvector, to_tsquery, plainto_tsquery, phraseto_tsquery, websearch_to_tsquery
//! - Ranking: ts_rank (maps to bm25)
//! - Highlighting: ts_headline (maps to highlight)
//! - Match operator: @@ → MATCH

use std::collections::HashSet;

/// Common English stop words (PostgreSQL's default english stop list)
const ENGLISH_STOP_WORDS: &[&str] = &[
    "a", "about", "above", "after", "again", "against", "all", "am", "an", "and",
    "any", "are", "as", "at", "be", "because", "been", "before", "being", "below",
    "between", "both", "but", "by", "can", "could", "did", "do", "does", "doing",
    "down", "during", "each", "few", "for", "from", "further", "had", "has", "have",
    "having", "he", "her", "here", "hers", "herself", "him", "himself", "his", "how",
    "i", "if", "in", "into", "is", "it", "its", "itself", "just", "me", "more",
    "most", "my", "myself", "no", "nor", "not", "now", "of", "off", "on", "once",
    "only", "or", "other", "our", "ours", "ourselves", "out", "over", "own", "same",
    "she", "should", "so", "some", "such", "than", "that", "the", "their", "theirs",
    "them", "themselves", "then", "there", "these", "they", "this", "those", "through",
    "to", "too", "under", "until", "up", "very", "was", "we", "were", "what", "when",
    "where", "which", "while", "who", "whom", "why", "will", "with", "would", "you",
    "your", "yours", "yourself", "yourselves",
];

/// Simple Porter stemmer for English (simplified implementation)
/// This provides basic stemming for common English word patterns
pub fn stem_word(word: &str) -> String {
    let word = word.to_lowercase();
    let word = word.trim_end_matches('s');
    
    // Handle common suffixes
    let stemmed = if word.ends_with("ies") && word.len() > 4 {
        format!("{}y", &word[..word.len()-3])
    } else if word.ends_with("es") && word.len() > 4 {
        word[..word.len()-2].to_string()
    } else if word.ends_with("ing") && word.len() > 5 {
        word[..word.len()-3].to_string()
    } else if word.ends_with("ly") && word.len() > 4 {
        word[..word.len()-2].to_string()
    } else if word.ends_with("ed") && word.len() > 4 {
        word[..word.len()-2].to_string()
    } else if word.ends_with("er") && word.len() > 4 {
        word[..word.len()-2].to_string()
    } else if word.ends_with("ment") && word.len() > 6 {
        word[..word.len()-4].to_string()
    } else if word.ends_with("ness") && word.len() > 6 {
        word[..word.len()-4].to_string()
    } else if word.ends_with("tion") && word.len() > 6 {
        word[..word.len()-4].to_string()
    } else if word.ends_with("ational") && word.len() > 8 {
        format!("{}ate", &word[..word.len()-7])
    } else if word.ends_with("tional") && word.len() > 7 {
        format!("{}tion", &word[..word.len()-6])
    } else if word.ends_with("iveness") && word.len() > 8 {
        format!("{}ive", &word[..word.len()-7])
    } else if word.ends_with("ousness") && word.len() > 8 {
        format!("{}ous", &word[..word.len()-7])
    } else {
        word.to_string()
    };
    
    stemmed
}

/// Check if a word is a stop word for the given configuration
pub fn is_stop_word(word: &str, config: &str) -> bool {
    let config = config.to_lowercase();
    
    // Currently only English stop words are supported
    if config == "english" || config == "en" {
        ENGLISH_STOP_WORDS.contains(&word.to_lowercase().as_str())
    } else {
        // For other languages, no stop words (for now)
        false
    }
}

/// Tokenize text into words, preserving FTS operators
fn tokenize(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current_token = String::new();
    
    for c in text.chars() {
        if c.is_alphanumeric() {
            current_token.push(c);
        } else {
            // Flush current token if any
            if !current_token.is_empty() {
                tokens.push(current_token.to_lowercase());
                current_token.clear();
            }
            // Preserve FTS operators
            if ['&', '|', '!', '(', ')', '<', '>'].contains(&c) {
                tokens.push(c.to_string());
            }
        }
    }
    
    // Don't forget the last token
    if !current_token.is_empty() {
        tokens.push(current_token.to_lowercase());
    }
    
    tokens
}

/// Convert text to a tsvector-like representation
/// Returns a string in PostgreSQL's tsvector format: 'lexeme1':1 'lexeme2':2,3 ...
pub fn to_tsvector_impl(config: &str, text: &str) -> String {
    let tokens = tokenize(text);
    let mut lexeme_positions: std::collections::HashMap<String, Vec<usize>> = 
        std::collections::HashMap::new();
    
    for (pos, token) in tokens.iter().enumerate() {
        // Skip stop words
        if is_stop_word(token, config) {
            continue;
        }
        
        // Apply stemming
        let lexeme = stem_word(token);
        
        // Skip if lexeme is too short (usually noise)
        if lexeme.len() < 2 {
            continue;
        }
        
        // Record position (1-indexed in PostgreSQL)
        lexeme_positions
            .entry(lexeme)
            .or_default()
            .push(pos + 1);
    }
    
    // Format as tsvector: 'lexeme':pos1,pos2 ...
    let mut entries: Vec<String> = Vec::new();
    for (lexeme, positions) in lexeme_positions {
        let pos_str = positions
            .iter()
            .map(|p| p.to_string())
            .collect::<Vec<_>>()
            .join(",");
        entries.push(format!("'{}':{}", lexeme, pos_str));
    }
    
    // Sort lexemes alphabetically (PostgreSQL tsvector format)
    entries.sort();
    entries.join(" ")
}

/// Parse a tsquery string and convert to FTS5 query format
/// PostgreSQL syntax: term1 & term2 | term3 !term4 term5 <-> term6
/// FTS5 syntax: term1 AND term2 OR term3 NOT term4 "term5 term6"
pub fn to_tsquery_impl(config: &str, query: &str) -> String {
    let query = query.trim();
    
    // Process the query: apply stemming and stop word removal
    let tokens = tokenize(query);
    let mut processed_tokens: Vec<String> = Vec::new();
    
    for token in tokens {
        // Skip operators
        if ["&", "|", "!", "(", ")", "<", ">"].contains(&token.as_str()) {
            processed_tokens.push(token);
            continue;
        }
        
        // Skip stop words
        if is_stop_word(&token, config) {
            continue;
        }
        
        // Apply stemming
        let lexeme = stem_word(&token);
        if lexeme.len() >= 2 {
            processed_tokens.push(lexeme);
        }
    }
    
    // Convert PostgreSQL operators to FTS5 syntax
    convert_pg_tsquery_to_fts5(&processed_tokens.join(" "))
}

/// Convert plainto_tsquery (plain text query)
/// In PostgreSQL, this treats all words as ANDed together
pub fn plainto_tsquery_impl(config: &str, query: &str) -> String {
    let tokens = tokenize(query);
    let mut terms: Vec<String> = Vec::new();
    
    for token in tokens {
        if is_stop_word(&token, config) {
            continue;
        }
        
        let lexeme = stem_word(&token);
        if lexeme.len() >= 2 {
            terms.push(lexeme);
        }
    }
    
    // Join with AND (FTS5 default)
    terms.join(" ")
}

/// Convert phraseto_tsquery (phrase query)
/// In PostgreSQL, this uses the <-> (followed-by) operator
pub fn phraseto_tsquery_impl(config: &str, query: &str) -> String {
    let tokens = tokenize(query);
    let mut terms: Vec<String> = Vec::new();
    
    for token in tokens {
        if is_stop_word(&token, config) {
            continue;
        }
        
        let lexeme = stem_word(&token);
        if lexeme.len() >= 2 {
            terms.push(lexeme);
        }
    }
    
    // Quote the phrase for FTS5
    if terms.is_empty() {
        String::new()
    } else {
        format!("\"{}\"", terms.join(" "))
    }
}

/// Convert websearch_to_tsquery (Google-style query)
/// Supports: "quoted phrase", OR, -, *
pub fn websearch_to_tsquery_impl(config: &str, query: &str) -> String {
    let mut result = String::new();
    let mut in_quotes = false;
    let mut current_term = String::new();
    let mut current_phrase = String::new();
    let mut last_was_or = false;
    
    let chars: Vec<char> = query.chars().collect();
    let mut i = 0;
    
    while i < chars.len() {
        let c = chars[i];
        
        if c == '"' {
            if in_quotes {
                // End of phrase
                if !current_phrase.is_empty() {
                    let phrase_terms: Vec<String> = tokenize(&current_phrase)
                        .iter()
                        .filter(|t| !is_stop_word(t, config))
                        .filter_map(|t| {
                            let stemmed = stem_word(t);
                            if stemmed.len() >= 2 {
                                Some(stemmed)
                            } else {
                                None
                            }
                        })
                        .collect();
                    
                    if !phrase_terms.is_empty() {
                        if !result.is_empty() && !last_was_or {
                            result.push_str(" AND ");
                        }
                        result.push_str(&format!("\"{}\"", phrase_terms.join(" ")));
                    }
                }
                current_phrase.clear();
                last_was_or = false;
            } else {
                // Start of phrase
                in_quotes = true;
            }
            i += 1;
            continue;
        }
        
        if in_quotes {
            current_phrase.push(c);
            i += 1;
            continue;
        }
        
        if c == '-' {
            // NOT operator
            if !result.is_empty() && !last_was_or {
                result.push_str(" AND ");
            }
            result.push_str("NOT ");
            i += 1;
            continue;
        }
        
        if c.is_whitespace() {
            if !current_term.is_empty() {
                // Process the term
                let upper = current_term.to_uppercase();
                if upper == "OR" {
                    result.push_str(" OR ");
                    last_was_or = true;
                } else if !is_stop_word(&current_term, config) {
                    let lexeme = stem_word(&current_term);
                    if lexeme.len() >= 2 {
                        if !result.is_empty() && !last_was_or {
                            result.push_str(" AND ");
                        }
                        result.push_str(&lexeme);
                        last_was_or = false;
                    }
                }
                current_term.clear();
            }
            i += 1;
            continue;
        }
        
        current_term.push(c);
        i += 1;
    }
    
    // Process remaining term
    if !current_term.is_empty() {
        let upper = current_term.to_uppercase();
        if upper != "OR" && !is_stop_word(&current_term, config) {
            let lexeme = stem_word(&current_term);
            if lexeme.len() >= 2 {
                if !result.is_empty() {
                    result.push_str(" AND ");
                }
                result.push_str(&lexeme);
            }
        }
    }
    
    result
}

/// Convert PostgreSQL tsquery syntax to FTS5 syntax
fn convert_pg_tsquery_to_fts5(query: &str) -> String {
    let mut result = String::new();
    let mut chars = query.chars().peekable();
    let mut pending_and = false;
    
    while let Some(c) = chars.next() {
        match c {
            '&' => {
                result.push_str(" AND ");
                pending_and = false;
            }
            '|' => {
                result.push_str(" OR ");
                pending_and = false;
            }
            '!' => {
                result.push_str(" NOT ");
                pending_and = false;
            }
            '<' => {
                // Check for <-> (phrase) or <N> (proximity)
                if chars.peek() == Some(&'-') {
                    chars.next(); // consume '-'
                    if chars.peek() == Some(&'>') {
                        chars.next(); // consume '>'
                        // Phrase search - we'll collect terms and quote them
                        // For now, just use AND as approximation
                        result.push(' ');
                        pending_and = false;
                    }
                } else {
                    // Proximity <N> - treat as NEAR
                    let mut num_str = String::new();
                    while let Some(&d) = chars.peek() {
                        if d.is_ascii_digit() {
                            num_str.push(d);
                            chars.next();
                        } else {
                            break;
                        }
                    }
                    if chars.peek() == Some(&'>') {
                        chars.next(); // consume '>'
                        // NEAR with distance
                        result.push_str(" NEAR(");
                        pending_and = true;
                    }
                }
            }
            '(' => {
                result.push('(');
            }
            ')' => {
                result.push(')');
            }
            '\'' => {
                // Quoted term
                let mut term = String::new();
                while let Some(&t) = chars.peek() {
                    if t == '\'' {
                        chars.next();
                        break;
                    }
                    term.push(t);
                    chars.next();
                }
                result.push_str(&term);
            }
            c if c.is_whitespace() => {
                if pending_and {
                    result.push_str(" AND ");
                    pending_and = false;
                } else {
                    result.push(' ');
                }
            }
            _ => {
                result.push(c);
                pending_and = true;
            }
        }
    }
    
    // Clean up multiple spaces
    let cleaned: String = result
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    
    cleaned
}

/// Check if a tsvector matches a tsquery
/// This is the implementation of the @@ operator
pub fn tsvector_matches_tsquery(tsvector: &str, tsquery: &str) -> bool {
    // Extract lexemes from tsvector
    let vector_lexemes: HashSet<String> = tsvector
        .split_whitespace()
        .filter_map(|entry| {
            // Entry format: 'lexeme':positions
            entry.split('\'').nth(1).map(|s| s.to_lowercase())
        })
        .collect();
    
    // Extract required terms from tsquery
    let query_terms = extract_tsquery_terms(tsquery);
    
    // Check if all required terms are present
    for term in query_terms.positive {
        if !vector_lexemes.contains(&term.to_lowercase()) {
            return false;
        }
    }
    
    // Check that no excluded terms are present
    for term in query_terms.negative {
        if vector_lexemes.contains(&term.to_lowercase()) {
            return false;
        }
    }
    
    true
}

/// Extract positive and negative terms from a tsquery
pub struct TsQueryTerms {
    pub positive: Vec<String>,
    pub negative: Vec<String>,
}

pub fn extract_tsquery_terms(query: &str) -> TsQueryTerms {
    let mut positive = Vec::new();
    let mut negative = Vec::new();
    let mut is_negated = false;
    
    let tokens: Vec<&str> = query.split_whitespace().collect();
    let mut i = 0;
    
    while i < tokens.len() {
        let token = tokens[i];
        
        match token.to_uppercase().as_str() {
            "NOT" | "!" => {
                is_negated = true;
            }
            "AND" | "&" | "OR" | "|" => {
                is_negated = false;
            }
            _ => {
                // Clean up the term
                let term = token
                    .trim_matches('\'')
                    .trim_matches('"')
                    .trim_matches('(')
                    .trim_matches(')')
                    .to_lowercase();
                
                if !term.is_empty() && term.len() >= 2 {
                    if is_negated {
                        negative.push(term);
                    } else {
                        positive.push(term);
                    }
                }
                is_negated = false;
            }
        }
        i += 1;
    }
    
    TsQueryTerms { positive, negative }
}

/// Calculate ts_rank (text search rank)
/// Returns a rank value based on term frequency
pub fn ts_rank_impl(tsvector: &str, tsquery: &str) -> f64 {
    let vector_lexemes: HashSet<String> = tsvector
        .split_whitespace()
        .filter_map(|entry| {
            entry.split('\'').nth(1).map(|s| s.to_lowercase())
        })
        .collect();
    
    let query_terms = extract_tsquery_terms(tsquery);
    
    // Count matching terms
    let matches = query_terms
        .positive
        .iter()
        .filter(|term| vector_lexemes.contains(&term.to_lowercase()))
        .count();
    
    // Simple ranking: ratio of matching terms
    if query_terms.positive.is_empty() {
        0.0
    } else {
        matches as f64 / query_terms.positive.len() as f64
    }
}

/// Generate ts_headline (highlighted snippet)
/// Returns the text with matching terms highlighted
pub fn ts_headline_impl(_config: &str, text: &str, tsquery: &str, options: Option<&str>) -> String {
    // Parse options for start/stop markers
    let (start_sel, stop_sel) = parse_headline_options(options);
    
    let query_terms = extract_tsquery_terms(tsquery);
    let mut result = text.to_string();
    
    // Highlight each matching term
    for term in query_terms.positive {
        // Case-insensitive replacement
        let lower_result = result.to_lowercase();
        let lower_term = term.to_lowercase();
        
        let mut new_result = String::new();
        let mut last_end = 0;
        
        for (idx, _) in lower_result.match_indices(&lower_term) {
            new_result.push_str(&result[last_end..idx]);
            new_result.push_str(&start_sel);
            new_result.push_str(&result[idx..idx + term.len()]);
            new_result.push_str(&stop_sel);
            last_end = idx + term.len();
        }
        new_result.push_str(&result[last_end..]);
        result = new_result;
    }
    
    result
}

/// Parse ts_headline options
fn parse_headline_options(options: Option<&str>) -> (String, String) {
    let mut start_sel = "<b>".to_string();
    let mut stop_sel = "</b>".to_string();
    
    if let Some(opts) = options {
        // Parse option format: StartSel=<start>, StopSel=<stop>
        for opt in opts.split(',') {
            let parts: Vec<&str> = opt.splitn(2, '=').collect();
            if parts.len() == 2 {
                let key = parts[0].trim();
                let value = parts[1].trim();
                match key.to_lowercase().as_str() {
                    "startsel" => start_sel = value.to_string(),
                    "stopsel" => stop_sel = value.to_string(),
                    _ => {}
                }
            }
        }
    }
    
    (start_sel, stop_sel)
}

/// Set weight on a tsvector (for ranking)
/// Weights: 'A' = most important, 'D' = least important
pub fn setweight_impl(tsvector: &str, weight: char) -> String {
    let weight = weight.to_ascii_uppercase();
    if !"ABCD".contains(weight) {
        return tsvector.to_string();
    }
    
    // Add weight to each lexeme entry
    tsvector
        .split_whitespace()
        .map(|entry| {
            // Entry format: 'lexeme':positions
            if let Some(pos) = entry.find(':') {
                let (lexeme_part, pos_part) = entry.split_at(pos);
                format!("{}{}{}", lexeme_part, weight, pos_part)
            } else {
                entry.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Strip positions and weights from a tsvector
pub fn strip_impl(tsvector: &str) -> String {
    tsvector
        .split_whitespace()
        .filter_map(|entry| {
            // Entry format: 'lexeme':positions or 'lexeme'A:positions
            entry.split('\'').nth(1).map(|s| format!("'{}'", s.split(':').next().unwrap_or(s)))
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Count the number of nodes in a tsquery
pub fn numnode_impl(tsquery: &str) -> i32 {
    let tokens: Vec<&str> = tsquery.split_whitespace().collect();
    let mut count = 0;
    
    for token in tokens {
        match token.to_uppercase().as_str() {
            "&" | "|" | "AND" | "OR" => count += 1,
            "!" | "NOT" => count += 1,
            "<->" => count += 1,
            _ if !token.starts_with('(') && !token.ends_with(')') => count += 1,
            _ => {}
        }
    }
    
    count
}

/// Get the query tree representation of a tsquery
pub fn querytree_impl(tsquery: &str) -> String {
    // Return the positive part of the query (simplified)
    let terms = extract_tsquery_terms(tsquery);
    terms.positive.join(" | ")
}

/// Concatenate two tsvectors
pub fn tsvector_concat(left: &str, right: &str) -> String {
    let mut entries: Vec<String> = Vec::new();
    
    // Parse left vector
    for entry in left.split_whitespace() {
        entries.push(entry.to_string());
    }
    
    // Parse right vector with adjusted positions
    let left_max_pos = entries
        .iter()
        .filter_map(|e| {
            e.split(':')
                .nth(1)
                .and_then(|s| s.split(',').filter_map(|n| n.parse::<usize>().ok()).max())
        })
        .max()
        .unwrap_or(0);
    
    for entry in right.split_whitespace() {
        if let Some(pos) = entry.find(':') {
            let (lexeme, positions) = entry.split_at(pos);
            // Adjust positions
            let adjusted_positions: String = positions[1..]
                .split(',')
                .filter_map(|n| {
                    n.parse::<usize>()
                        .ok()
                        .map(|p| (p + left_max_pos).to_string())
                })
                .collect::<Vec<_>>()
                .join(",");
            entries.push(format!("{}:{}", lexeme, adjusted_positions));
        } else {
            entries.push(entry.to_string());
        }
    }
    
    entries.sort();
    entries.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stem_word() {
        assert_eq!(stem_word("running"), "runn");
        assert_eq!(stem_word("quickly"), "quick");
        assert_eq!(stem_word("studies"), "studie"); // Simplified stemmer
        assert_eq!(stem_word("cats"), "cat");
        // Note: Our simplified stemmer doesn't fully implement Porter stemmer
        // "database" -> "database" (no stemming)
        // "databases" -> "database" (removes trailing 's')
        assert_eq!(stem_word("databases"), "database");
    }

    #[test]
    fn test_is_stop_word() {
        assert!(is_stop_word("the", "english"));
        assert!(is_stop_word("and", "english"));
        assert!(!is_stop_word("hello", "english"));
        assert!(!is_stop_word("postgresql", "english"));
    }

    #[test]
    fn test_to_tsvector() {
        let result = to_tsvector_impl("english", "The quick brown fox jumps over the lazy dog");
        assert!(result.contains("'quick'"));
        assert!(result.contains("'brown'"));
        assert!(result.contains("'fox'"));
        assert!(result.contains("'jump'"));
        assert!(!result.contains("'the'")); // stop word
        assert!(!result.contains("'over'")); // stop word
    }

    #[test]
    fn test_to_tsquery() {
        let result = to_tsquery_impl("english", "postgresql & running");
        assert!(result.contains("postgresql"));
        assert!(result.contains("AND"));
        assert!(result.contains("runn")); // "running" -> "runn"
    }

    #[test]
    fn test_plainto_tsquery() {
        let result = plainto_tsquery_impl("english", "postgresql running");
        assert!(result.contains("postgresql"));
        assert!(result.contains("runn")); // "running" -> "runn"
    }

    #[test]
    fn test_phraseto_tsquery() {
        let result = phraseto_tsquery_impl("english", "postgresql database");
        assert!(result.starts_with('"'));
        assert!(result.ends_with('"'));
    }

    #[test]
    fn test_websearch_to_tsquery() {
        let result = websearch_to_tsquery_impl("english", "postgresql OR running");
        assert!(result.contains("postgresql"));
        assert!(result.contains("OR"));
        assert!(result.contains("runn")); // "running" -> "runn"
    }

    #[test]
    fn test_tsvector_matches_tsquery() {
        // Test with words that our stemmer actually transforms
        let vector = to_tsvector_impl("english", "postgresql is running quickly");
        let query = to_tsquery_impl("english", "postgresql & runn");
        assert!(tsvector_matches_tsquery(&vector, &query));
        
        // Test that non-matching query returns false
        let query2 = to_tsquery_impl("english", "mysql & runn");
        assert!(!tsvector_matches_tsquery(&vector, &query2));
        
        // Test with original word forms (stemmer transforms both)
        let vector2 = to_tsvector_impl("english", "cats and dogs");
        let query3 = to_tsquery_impl("english", "cat & dog");
        assert!(tsvector_matches_tsquery(&vector2, &query3));
    }

    #[test]
    fn test_ts_rank() {
        let vector = to_tsvector_impl("english", "postgresql running");
        let query = to_tsquery_impl("english", "postgresql & runn");
        let rank = ts_rank_impl(&vector, &query);
        assert!(rank > 0.0);
        assert!(rank <= 1.0);
    }

    #[test]
    fn test_ts_headline() {
        let text = "PostgreSQL is running quickly";
        let query = to_tsquery_impl("english", "postgresql & runn");
        let headline = ts_headline_impl("english", text, &query, None);
        assert!(headline.contains("<b>"));
        assert!(headline.contains("</b>"));
    }

    #[test]
    fn test_setweight() {
        let vector = to_tsvector_impl("english", "hello world");
        let weighted = setweight_impl(&vector, 'A');
        assert!(weighted.contains("A:"));
    }

    #[test]
    fn test_strip() {
        let vector = to_tsvector_impl("english", "hello world");
        let stripped = strip_impl(&vector);
        assert!(!stripped.contains(':'));
        assert!(stripped.contains("'hello'"));
        assert!(stripped.contains("'world'"));
    }

    #[test]
    fn test_tsvector_concat() {
        let left = "'hello':1 'world':2";
        let right = "'postgresql':1";
        let result = tsvector_concat(left, right);
        assert!(result.contains("'hello':1"));
        assert!(result.contains("'world':2"));
        assert!(result.contains("'postgresql':"));
    }

    #[test]
    fn test_numnode() {
        let query = "postgresql & database | mysql";
        let count = numnode_impl(query);
        assert!(count > 0);
    }

    #[test]
    fn test_convert_pg_tsquery_to_fts5() {
        let result = convert_pg_tsquery_to_fts5("postgresql & database");
        assert!(result.contains("AND"));
        
        let result = convert_pg_tsquery_to_fts5("postgresql | database");
        assert!(result.contains("OR"));
        
        let result = convert_pg_tsquery_to_fts5("! postgresql");
        assert!(result.contains("NOT"));
    }
}
