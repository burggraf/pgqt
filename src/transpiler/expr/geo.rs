//! Geometric expression reconstruction
//!
//! Handles PostgreSQL geometric types and operators, converting them
//! to SQLite geometric function equivalents.

/// Check if operands indicate a geometric operation
pub(crate) fn is_geo_operation(lexpr_sql: &str, rexpr_sql: &str) -> bool {
    let lexpr_lower = lexpr_sql.to_lowercase();
    let rexpr_lower = rexpr_sql.to_lowercase();
    
    looks_like_geo(&lexpr_lower) || looks_like_geo(&rexpr_lower)
}

/// Check if a SQL value looks like a geometric type
pub(crate) fn looks_like_geo(val: &str) -> bool {
    let lower = val.to_lowercase();
    
    // Exclude integer type casts - these are definitely not geometric
    if lower.contains("::int") || lower.contains("::integer") ||
       lower.contains("::smallint") || lower.contains("::bigint") ||
       lower.contains("::int2") || lower.contains("::int4") || lower.contains("::int8") {
        return false;
    }
    
    // Exclude cast() to integer types
    if lower.contains("cast(") && 
       (lower.contains("as int") || lower.contains("as integer") || 
        lower.contains("as smallint") || lower.contains("as bigint")) {
        return false;
    }
    
    // Check for circle format: <(x,y),r>
    if lower.starts_with('<') && lower.ends_with('>') {
        return true;
    }
    
    // For cast expressions, extract the inner string literal and check it
    if lower.contains("cast(") && lower.contains("as text") {
        // Try to extract the string content from cast('...' as text)
        if let Some(start) = lower.find("'") {
            if let Some(end) = lower[start+1..].find("'") {
                let inner = &lower[start+1..start+1+end];
                // Check if the inner content looks like a geometric literal
                if looks_like_geo_literal(inner) {
                    return true;
                }
            }
        }
    }
    
    // Point: (x,y) - exactly one comma, no brackets
    // Box/lseg: (x1,y1),(x2,y2) - exactly 3 commas, no brackets
    if !lower.contains('[') && !lower.contains(']') {
        let comma_count = lower.matches(',').count();
        if lower.contains('(') && lower.contains(')') {
            return comma_count == 1 || comma_count == 3;
        }
    }
    
    false
}

/// Check if a string looks like a geometric literal (not a range)
pub(crate) fn looks_like_geo_literal(trimmed: &str) -> bool {
    // Geometric types: (x,y), ((x1,y1),(x2,y2)), <(x,y),r>
    // Ranges: [a,b), (a,b], [a,b], (a,b), empty
    // Points have 1 comma, boxes/lsegs have 3 commas, circles start with <
    let comma_count = trimmed.matches(',').count();
    let is_point = trimmed.starts_with('(') && 
        !trimmed.contains('[') && 
        !trimmed.contains(']') &&
        comma_count == 1;
    let is_box_or_lseg = trimmed.starts_with('(') && 
        !trimmed.contains('[') && 
        !trimmed.contains(']') &&
        comma_count == 3;
    let is_circle = trimmed.starts_with('<') && trimmed.ends_with('>');
    
    is_point || is_box_or_lseg || is_circle
}

/// Reconstruct geometric overlaps operator (&&)
pub(crate) fn geo_overlaps(lexpr_sql: &str, rexpr_sql: &str) -> String {
    format!("geo_overlaps({}, {})", lexpr_sql, rexpr_sql)
}

/// Reconstruct geometric contains operator (@>)
pub(crate) fn geo_contains(lexpr_sql: &str, rexpr_sql: &str) -> String {
    format!("geo_contains({}, {})", lexpr_sql, rexpr_sql)
}

/// Reconstruct geometric contained operator (<@)
pub(crate) fn geo_contained(lexpr_sql: &str, rexpr_sql: &str) -> String {
    format!("geo_contained({}, {})", lexpr_sql, rexpr_sql)
}

/// Reconstruct geometric left operator (<<)
pub(crate) fn geo_left(lexpr_sql: &str, rexpr_sql: &str) -> String {
    format!("geo_left({}, {})", lexpr_sql, rexpr_sql)
}

/// Reconstruct geometric right operator (>>)
pub(crate) fn geo_right(lexpr_sql: &str, rexpr_sql: &str) -> String {
    format!("geo_right({}, {})", lexpr_sql, rexpr_sql)
}

/// Reconstruct geometric below operator (<<|)
pub(crate) fn geo_below(lexpr_sql: &str, rexpr_sql: &str) -> String {
    format!("geo_below({}, {})", lexpr_sql, rexpr_sql)
}

/// Reconstruct geometric above operator (|>>)
pub(crate) fn geo_above(lexpr_sql: &str, rexpr_sql: &str) -> String {
    format!("geo_above({}, {})", lexpr_sql, rexpr_sql)
}

/// Reconstruct geometric distance operator (<->)
pub(crate) fn geo_distance(lexpr_sql: &str, rexpr_sql: &str) -> String {
    format!("geo_distance({}, {})", lexpr_sql, rexpr_sql)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_looks_like_geo_with_integer_cast() {
        assert!(!looks_like_geo("(-1::int2"));
        assert!(!looks_like_geo("(-1::int4"));
        assert!(looks_like_geo("(1,2)"));
        assert!(looks_like_geo("<(1,2),3>"));
    }
}