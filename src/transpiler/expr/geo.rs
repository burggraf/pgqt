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
    // Geometric types: (x,y), ((x1,y1),(x2,y2)), <(x,y),r>
    // Points have 1 comma, boxes/lsegs have 3 commas, circles start with <
    val.contains('<') ||
    (!val.contains('[') && val.contains('(') && val.contains(',') && val.contains(')'))
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