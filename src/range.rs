//! PostgreSQL Range Support for SQLite
//!
//! This module provides PostgreSQL-compatible range operations by storing ranges
//! as strings (TEXT) in SQLite. It supports the standard PostgreSQL range types:
//! - int4range (integer)
//! - int8range (bigint)
//! - numrange (numeric)
//! - tsrange (timestamp)
//! - tstzrange (timestamptz)
//! - daterange (date)
//!
//! Canonicalization for discrete types (int4range, int8range, daterange) is
//! automatically performed, normalizing to `[low, high)` format.

use std::cmp::{Ordering, PartialOrd};
use std::fmt;

/// Represents a boundary value of a range
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RangeBound<T> {
    /// Finite boundary value
    Value(T),
    /// Infinite boundary value (unbounded)
    Infinite,
}

impl<T: PartialOrd> PartialOrd for RangeBound<T> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        match (self, other) {
            (RangeBound::Infinite, RangeBound::Infinite) => Some(Ordering::Equal),
            (RangeBound::Infinite, _) => Some(Ordering::Greater),
            (_, RangeBound::Infinite) => Some(Ordering::Less),
            (RangeBound::Value(a), RangeBound::Value(b)) => a.partial_cmp(b),
        }
    }
}

/// Type of range (discrete or continuous)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RangeType {
    Int4,
    Int8,
    Numeric,
    Timestamp,
    TimestampTz,
    Date,
}

/// Represents a PostgreSQL range value
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RangeValue {
    /// A range with bounds
    Range {
        low: RangeBound<String>,
        high: RangeBound<String>,
        low_inc: bool,
        high_inc: bool,
        rtype: RangeType,
    },
    /// An empty range
    Empty(RangeType),
}

impl RangeValue {
    /// Create an empty range of a given type
    pub fn empty(rtype: RangeType) -> Self {
        RangeValue::Empty(rtype)
    }

    /// Check if the range is empty
    pub fn is_empty(&self) -> bool {
        matches!(self, RangeValue::Empty(_))
    }

    /// Get the range type
    pub fn rtype(&self) -> RangeType {
        match self {
            RangeValue::Range { rtype, .. } => *rtype,
            RangeValue::Empty(rtype) => *rtype,
        }
    }

    /// Convert to PostgreSQL string format (canonicalized for discrete types)
    pub fn to_postgres_string(&self) -> String {
        match self {
            RangeValue::Empty(_) => "empty".to_string(),
            RangeValue::Range { low, high, low_inc, high_inc, rtype: _ } => {
                let low_str = match low {
                    RangeBound::Value(s) => s.clone(),
                    RangeBound::Infinite => "".to_string(),
                };
                let high_str = match high {
                    RangeBound::Value(s) => s.clone(),
                    RangeBound::Infinite => "".to_string(),
                };
                let l_bracket = if *low_inc { "[" } else { "(" };
                let r_bracket = if *high_inc { "]" } else { ")" };
                format!("{}{},{}{}", l_bracket, low_str, high_str, r_bracket)
            }
        }
    }

    /// Canonicalize the range based on its type
    pub fn canonicalize(self) -> Self {
        if self.is_empty() {
            return self;
        }

        let (mut low, mut high, mut low_inc, mut high_inc, rtype) = match self {
            RangeValue::Range { low, high, low_inc, high_inc, rtype } => (low, high, low_inc, high_inc, rtype),
            RangeValue::Empty(_) => unreachable!(),
        };

        match rtype {
            RangeType::Int4 | RangeType::Int8 | RangeType::Date => {
                // Discrete canonicalization: [low, high)
                
                // 1. Normalize low bound
                if !low_inc {
                    if let RangeBound::Value(v) = low {
                        low = RangeBound::Value(increment_discrete(&v, rtype));
                        low_inc = true;
                    }
                }

                // 2. Normalize high bound
                if high_inc {
                    if let RangeBound::Value(v) = high {
                        high = RangeBound::Value(increment_discrete(&v, rtype));
                        high_inc = false;
                    }
                }

                // 3. Check for empty
                if let (RangeBound::Value(l), RangeBound::Value(h)) = (&low, &high) {
                    if compare_discrete(l, h, rtype) == Ordering::Greater || 
                       (compare_discrete(l, h, rtype) == Ordering::Equal && (!low_inc || !high_inc)) {
                        return RangeValue::Empty(rtype);
                    }
                }
                
                RangeValue::Range { low, high, low_inc, high_inc, rtype }
            }
            _ => {
                // Continuous ranges: keep as is, but check for empty
                if let (RangeBound::Value(l), RangeBound::Value(h)) = (&low, &high) {
                    // Strings comparison for simplicity, though not 100% correct for numeric/timestamp
                    // Real implementation would parse numbers/dates
                    match l.partial_cmp(h) {
                        Some(Ordering::Greater) => RangeValue::Empty(rtype),
                        Some(Ordering::Equal) if !low_inc || !high_inc => RangeValue::Empty(rtype),
                        _ => RangeValue::Range { low, high, low_inc, high_inc, rtype },
                    }
                } else {
                    RangeValue::Range { low, high, low_inc, high_inc, rtype }
                }
            }
        }
    }
}

/// Increment a discrete value (int4, int8, date)
fn increment_discrete(v: &str, rtype: RangeType) -> String {
    match rtype {
        RangeType::Int4 | RangeType::Int8 => {
            if let Ok(i) = v.parse::<i64>() {
                (i + 1).to_string()
            } else {
                v.to_string()
            }
        }
        RangeType::Date => {
            // For simplicity, just handle numeric dates or ISO strings
            // A full implementation would use a date library
            if let Ok(d) = chrono::NaiveDate::parse_from_str(v, "%Y-%m-%d") {
                if let Some(next) = d.succ_opt() {
                    return next.format("%Y-%m-%d").to_string();
                }
            }
            v.to_string()
        }
        _ => v.to_string(),
    }
}

/// Compare discrete values (int4, int8, date)
fn compare_discrete(a: &str, b: &str, rtype: RangeType) -> Ordering {
    match rtype {
        RangeType::Int4 | RangeType::Int8 => {
            let ai = a.parse::<i64>().unwrap_or(0);
            let bi = b.parse::<i64>().unwrap_or(0);
            ai.cmp(&bi)
        }
        RangeType::Date => {
            let ad = chrono::NaiveDate::parse_from_str(a, "%Y-%m-%d").unwrap_or_else(|_| chrono::NaiveDate::from_ymd_opt(1, 1, 1).unwrap());
            let bd = chrono::NaiveDate::parse_from_str(b, "%Y-%m-%d").unwrap_or_else(|_| chrono::NaiveDate::from_ymd_opt(1, 1, 1).unwrap());
            ad.cmp(&bd)
        }
        _ => a.cmp(b),
    }
}

/// Parse a PostgreSQL range literal
pub fn parse_range(input: &str, rtype: RangeType) -> Result<RangeValue, String> {
    let trimmed = input.trim();
    if trimmed.to_lowercase() == "empty" {
        return Ok(RangeValue::Empty(rtype));
    }

    if !((trimmed.starts_with('[') || trimmed.starts_with('(')) && (trimmed.ends_with(']') || trimmed.ends_with(')'))) {
        return Err(format!("Invalid range format: {}", input));
    }

    let low_inc = trimmed.starts_with('[');
    let high_inc = trimmed.ends_with(']');

    let content = &trimmed[1..trimmed.len() - 1];
    let parts: Vec<&str> = content.splitn(2, ',').collect();

    if parts.len() != 2 {
        return Err(format!("Invalid range format (missing comma): {}", input));
    }

    let low_raw = parts[0].trim();
    let high_raw = parts[1].trim();

    let low = if low_raw.is_empty() {
        RangeBound::Infinite
    } else {
        RangeBound::Value(low_raw.to_string())
    };

    let high = if high_raw.is_empty() {
        RangeBound::Infinite
    } else {
        RangeBound::Value(high_raw.to_string())
    };

    Ok(RangeValue::Range {
        low,
        high,
        low_inc,
        high_inc,
        rtype,
    }.canonicalize())
}

// ============================================================================
// Range Operators
// ============================================================================

/// Check if range contains an element
/// PostgreSQL: range @> element
pub fn range_contains_elem(range: &str, elem: &str, rtype: RangeType) -> Result<bool, String> {
    let rv = parse_range(range, rtype)?;
    if rv.is_empty() {
        return Ok(false);
    }

    match rv {
        RangeValue::Range { low, high, low_inc, high_inc, rtype } => {
            let l_ok = match low {
                RangeBound::Infinite => true,
                RangeBound::Value(v) => {
                    match compare_discrete(elem, &v, rtype) {
                        Ordering::Greater => true,
                        Ordering::Equal => low_inc,
                        Ordering::Less => false,
                    }
                }
            };

            let h_ok = match high {
                RangeBound::Infinite => true,
                RangeBound::Value(v) => {
                    match compare_discrete(elem, &v, rtype) {
                        Ordering::Less => true,
                        Ordering::Equal => high_inc,
                        Ordering::Greater => false,
                    }
                }
            };

            Ok(l_ok && h_ok)
        }
        _ => unreachable!(),
    }
}

/// Check if left range contains right range
/// PostgreSQL: range1 @> range2
pub fn range_contains(left: &str, right: &str, rtype: RangeType) -> Result<bool, String> {
    let lv = parse_range(left, rtype)?;
    let rv = parse_range(right, rtype)?;

    if rv.is_empty() {
        return Ok(true);
    }
    if lv.is_empty() {
        return Ok(false);
    }

    match (lv, rv) {
        (RangeValue::Range { low: l_low, high: l_high, low_inc: l_low_inc, high_inc: l_high_inc, rtype: _ },
         RangeValue::Range { low: r_low, high: r_high, low_inc: r_low_inc, high_inc: r_high_inc, rtype: _ }) => {
            
            // Check lower bound
            let l_low_ok = match (&l_low, &r_low) {
                (RangeBound::Infinite, _) => true,
                (_, RangeBound::Infinite) => false,
                (RangeBound::Value(lv), RangeBound::Value(rv)) => {
                    match compare_discrete(lv, rv, rtype) {
                        Ordering::Less => true,
                        Ordering::Equal => l_low_inc || !r_low_inc,
                        Ordering::Greater => false,
                    }
                }
            };

            // Check upper bound
            let l_high_ok = match (&l_high, &r_high) {
                (RangeBound::Infinite, _) => true,
                (_, RangeBound::Infinite) => false,
                (RangeBound::Value(lv), RangeBound::Value(rv)) => {
                    match compare_discrete(lv, rv, rtype) {
                        Ordering::Greater => true,
                        Ordering::Equal => l_high_inc || !r_high_inc,
                        Ordering::Less => false,
                    }
                }
            };

            Ok(l_low_ok && l_high_ok)
        }
        _ => unreachable!(),
    }
}

/// Check if left range is contained by right range
/// PostgreSQL: range1 <@ range2
pub fn range_contained(left: &str, right: &str, rtype: RangeType) -> Result<bool, String> {
    range_contains(right, left, rtype)
}

/// Check if two ranges overlap
/// PostgreSQL: range1 && range2
pub fn range_overlaps(left: &str, right: &str, rtype: RangeType) -> Result<bool, String> {
    let lv = parse_range(left, rtype)?;
    let rv = parse_range(right, rtype)?;

    if lv.is_empty() || rv.is_empty() {
        return Ok(false);
    }

    match (lv, rv) {
        (RangeValue::Range { low: l_low, high: l_high, low_inc: l_low_inc, high_inc: l_high_inc, rtype: _ },
         RangeValue::Range { low: r_low, high: r_high, low_inc: r_low_inc, high_inc: r_high_inc, rtype: _ }) => {
            
            // Overlap if NOT (left strictly left of right OR right strictly left of left)
            
            // l << r if l_high < r_low
            let l_left_of_r = match (&l_high, &r_low) {
                (RangeBound::Infinite, _) => false,
                (_, RangeBound::Infinite) => false,
                (RangeBound::Value(lh), RangeBound::Value(rl)) => {
                    match compare_discrete(lh, rl, rtype) {
                        Ordering::Less => true,
                        Ordering::Equal => !l_high_inc || !r_low_inc,
                        Ordering::Greater => false,
                    }
                }
            };

            // r << l if r_high < l_low
            let r_left_of_l = match (&r_high, &l_low) {
                (RangeBound::Infinite, _) => false,
                (_, RangeBound::Infinite) => false,
                (RangeBound::Value(rh), RangeBound::Value(ll)) => {
                    match compare_discrete(rh, ll, rtype) {
                        Ordering::Less => true,
                        Ordering::Equal => !r_high_inc || !l_low_inc,
                        Ordering::Greater => false,
                    }
                }
            };

            Ok(!(l_left_of_r || r_left_of_l))
        }
        _ => unreachable!(),
    }
}

/// Check if left range is strictly left of right range
/// PostgreSQL: range1 << range2
pub fn range_left(left: &str, right: &str, rtype: RangeType) -> Result<bool, String> {
    let lv = parse_range(left, rtype)?;
    let rv = parse_range(right, rtype)?;

    if lv.is_empty() || rv.is_empty() {
        return Ok(false);
    }

    match (lv, rv) {
        (RangeValue::Range { high: l_high, high_inc: l_high_inc, .. },
         RangeValue::Range { low: r_low, low_inc: r_low_inc, .. }) => {
            match (&l_high, &r_low) {
                (RangeBound::Infinite, _) => Ok(false),
                (_, RangeBound::Infinite) => Ok(false),
                (RangeBound::Value(lh), RangeBound::Value(rl)) => {
                    match compare_discrete(lh, rl, rtype) {
                        Ordering::Less => Ok(true),
                        Ordering::Equal => Ok(!l_high_inc || !r_low_inc),
                        Ordering::Greater => Ok(false),
                    }
                }
            }
        }
        _ => unreachable!(),
    }
}

/// Check if left range is strictly right of right range
/// PostgreSQL: range1 >> range2
pub fn range_right(left: &str, right: &str, rtype: RangeType) -> Result<bool, String> {
    range_left(right, left, rtype)
}

/// Check if two ranges are adjacent
/// PostgreSQL: range1 -|- range2
pub fn range_adjacent(left: &str, right: &str, rtype: RangeType) -> Result<bool, String> {
    let lv = parse_range(left, rtype)?;
    let rv = parse_range(right, rtype)?;

    if lv.is_empty() || rv.is_empty() {
        return Ok(false);
    }

    match (lv, rv) {
        (RangeValue::Range { low: l_low, high: l_high, low_inc: l_low_inc, high_inc: l_high_inc, .. },
         RangeValue::Range { low: r_low, high: r_high, low_inc: r_low_inc, high_inc: r_high_inc, .. }) => {
            
            // Check if l_high matches r_low or r_high matches l_low
            let adj1 = match (&l_high, &r_low) {
                (RangeBound::Value(lh), RangeBound::Value(rl)) => {
                    match compare_discrete(lh, rl, rtype) {
                        Ordering::Equal => l_high_inc != r_low_inc,
                        _ => false,
                    }
                }
                _ => false,
            };

            let adj2 = match (&r_high, &l_low) {
                (RangeBound::Value(rh), RangeBound::Value(ll)) => {
                    match compare_discrete(rh, ll, rtype) {
                        Ordering::Equal => r_high_inc != l_low_inc,
                        _ => false,
                    }
                }
                _ => false,
            };

            Ok(adj1 || adj2)
        }
        _ => unreachable!(),
    }
}

// ============================================================================
// Range Metadata Functions
// ============================================================================

pub fn lower(range: &str, rtype: RangeType) -> Result<Option<String>, String> {
    let rv = parse_range(range, rtype)?;
    match rv {
        RangeValue::Range { low: RangeBound::Value(v), .. } => Ok(Some(v)),
        _ => Ok(None),
    }
}

pub fn upper(range: &str, rtype: RangeType) -> Result<Option<String>, String> {
    let rv = parse_range(range, rtype)?;
    match rv {
        RangeValue::Range { high: RangeBound::Value(v), .. } => Ok(Some(v)),
        _ => Ok(None),
    }
}

pub fn lower_inc(range: &str, rtype: RangeType) -> Result<bool, String> {
    let rv = parse_range(range, rtype)?;
    match rv {
        RangeValue::Range { low_inc, .. } => Ok(low_inc),
        _ => Ok(false),
    }
}

pub fn upper_inc(range: &str, rtype: RangeType) -> Result<bool, String> {
    let rv = parse_range(range, rtype)?;
    match rv {
        RangeValue::Range { high_inc, .. } => Ok(high_inc),
        _ => Ok(false),
    }
}

pub fn lower_inf(range: &str, rtype: RangeType) -> Result<bool, String> {
    let rv = parse_range(range, rtype)?;
    match rv {
        RangeValue::Range { low: RangeBound::Infinite, .. } => Ok(true),
        _ => Ok(false),
    }
}

pub fn upper_inf(range: &str, rtype: RangeType) -> Result<bool, String> {
    let rv = parse_range(range, rtype)?;
    match rv {
        RangeValue::Range { high: RangeBound::Infinite, .. } => Ok(true),
        _ => Ok(false),
    }
}

pub fn isempty(range: &str, rtype: RangeType) -> Result<bool, String> {
    let rv = parse_range(range, rtype)?;
    Ok(rv.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_int4range() {
        let r = parse_range("[10,20]", RangeType::Int4).unwrap();
        assert_eq!(r.to_postgres_string(), "[10,21)");
        
        let r = parse_range("(10,20)", RangeType::Int4).unwrap();
        assert_eq!(r.to_postgres_string(), "[11,20)");
    }

    #[test]
    fn test_empty_range() {
        let r = parse_range("empty", RangeType::Int4).unwrap();
        assert!(r.is_empty());
        assert_eq!(r.to_postgres_string(), "empty");

        let r = parse_range("[10,10)", RangeType::Int4).unwrap();
        assert!(r.is_empty());
    }

    #[test]
    fn test_contains_elem() {
        assert!(range_contains_elem("[10,20)", "15", RangeType::Int4).unwrap());
        assert!(range_contains_elem("[10,20)", "10", RangeType::Int4).unwrap());
        assert!(!range_contains_elem("[10,20)", "20", RangeType::Int4).unwrap());
        assert!(range_contains_elem("(,)", "100", RangeType::Int4).unwrap());
    }

    #[test]
    fn test_contains_range() {
        assert!(range_contains("[10,30)", "[15,25)", RangeType::Int4).unwrap());
        assert!(range_contains("[10,30)", "[10,30)", RangeType::Int4).unwrap());
        assert!(!range_contains("[15,25)", "[10,30)", RangeType::Int4).unwrap());
    }

    #[test]
    fn test_overlaps() {
        assert!(range_overlaps("[10,20)", "[15,25)", RangeType::Int4).unwrap());
        assert!(range_overlaps("[10,20)", "[5,15)", RangeType::Int4).unwrap());
        assert!(!range_overlaps("[10,20)", "[20,30)", RangeType::Int4).unwrap());
    }

    #[test]
    fn test_adjacent() {
        assert!(range_adjacent("[10,20)", "[20,30)", RangeType::Int4).unwrap());
        assert!(range_adjacent("[10,20)", "[5,10)", RangeType::Int4).unwrap());
        assert!(!range_adjacent("[10,19)", "[20,30)", RangeType::Int4).unwrap());
    }

    #[test]
    fn test_daterange() {
        let r = parse_range("[2023-01-01,2023-01-01]", RangeType::Date).unwrap();
        assert_eq!(r.to_postgres_string(), "[2023-01-01,2023-01-02)");
    }
}
