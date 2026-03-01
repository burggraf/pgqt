//! PostgreSQL Array Support for SQLite
//!
//! This module provides PostgreSQL-compatible array operations by storing arrays
//! as JSON strings in SQLite. It supports both PostgreSQL array literal format
//! (`{1,2,3}`) and JSON array format (`[1,2,3]`).

use std::collections::HashSet;

/// Represents a parsed array value
#[derive(Debug, Clone, PartialEq)]
pub enum ArrayValue {
    /// One-dimensional array of values
    OneD(Vec<Option<String>>),
    /// Multi-dimensional array
    MultiD(Vec<ArrayValue>),
    /// Empty array
    Empty,
}

impl ArrayValue {
    /// Get the number of dimensions
    pub fn ndims(&self) -> i32 {
        match self {
            ArrayValue::Empty => 0,
            ArrayValue::OneD(_) => 1,
            ArrayValue::MultiD(inner) => {
                if inner.is_empty() {
                    0
                } else {
                    1 + inner.first().map(|v| v.ndims()).unwrap_or(0)
                }
            }
        }
    }

    /// Get the total number of elements
    pub fn cardinality(&self) -> i64 {
        match self {
            ArrayValue::Empty => 0,
            ArrayValue::OneD(v) => v.len() as i64,
            ArrayValue::MultiD(v) => v.iter().map(|a| a.cardinality()).sum(),
        }
    }

    /// Get length of a specific dimension (1-indexed)
    pub fn length(&self, dim: i32) -> Option<i64> {
        if dim < 1 {
            return None;
        }
        match self {
            ArrayValue::Empty => None,
            ArrayValue::OneD(_) => {
                if dim == 1 {
                    Some(self.cardinality())
                } else {
                    None
                }
            }
            ArrayValue::MultiD(v) => {
                if dim == 1 {
                    Some(v.len() as i64)
                } else {
                    v.first().and_then(|a| a.length(dim - 1))
                }
            }
        }
    }

    /// Get all elements as a flat vector
    pub fn flatten(&self) -> Vec<Option<String>> {
        match self {
            ArrayValue::Empty => Vec::new(),
            ArrayValue::OneD(v) => v.clone(),
            ArrayValue::MultiD(v) => v.iter().flat_map(|a| a.flatten()).collect(),
        }
    }

    /// Convert to PostgreSQL string format
    pub fn to_postgres_string(&self) -> String {
        match self {
            ArrayValue::Empty => "{}".to_string(),
            ArrayValue::OneD(v) => {
                let elements: Vec<String> = v
                    .iter()
                    .map(|e| match e {
                        Some(s) => {
                            // Quote if contains special characters
                            if s.contains(',') || s.contains('{') || s.contains('}') || s.contains('"') || s.is_empty() {
                                format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
                            } else {
                                s.clone()
                            }
                        }
                        None => "NULL".to_string(),
                    })
                    .collect();
                format!("{{{}}}", elements.join(","))
            }
            ArrayValue::MultiD(v) => {
                let inner: Vec<String> = v.iter().map(|a| a.to_postgres_string()).collect();
                format!("{{{}}}", inner.join(","))
            }
        }
    }

    /// Convert to JSON string format
    #[allow(dead_code)]
    pub fn to_json_string(&self) -> String {
        match self {
            ArrayValue::Empty => "[]".to_string(),
            ArrayValue::OneD(v) => {
                let elements: Vec<String> = v
                    .iter()
                    .map(|e| match e {
                        Some(s) => {
                            // Escape for JSON
                            let escaped = s
                                .replace('\\', "\\\\")
                                .replace('"', "\\\"")
                                .replace('\n', "\\n")
                                .replace('\r', "\\r")
                                .replace('\t', "\\t");
                            format!("\"{}\"", escaped)
                        }
                        None => "null".to_string(),
                    })
                    .collect();
                format!("[{}]", elements.join(","))
            }
            ArrayValue::MultiD(v) => {
                let inner: Vec<String> = v.iter().map(|a| a.to_json_string()).collect();
                format!("[{}]", inner.join(","))
            }
        }
    }
}

/// Parse a PostgreSQL array literal or JSON array
pub fn parse_array(input: &str) -> Result<ArrayValue, String> {
    let trimmed = input.trim();

    // Try JSON format first
    if trimmed.starts_with('[') {
        return parse_json_array(trimmed);
    }

    // Try PostgreSQL format
    if trimmed.starts_with('{') {
        return parse_pg_array(trimmed);
    }

    Err(format!("Invalid array format: {}", input))
}

/// Parse a JSON array
fn parse_json_array(input: &str) -> Result<ArrayValue, String> {
    let trimmed = input.trim();

    if !trimmed.starts_with('[') || !trimmed.ends_with(']') {
        return Err("JSON array must start with [ and end with ]".to_string());
    }

    let inner = &trimmed[1..trimmed.len() - 1].trim();

    if inner.is_empty() {
        return Ok(ArrayValue::Empty);
    }

    // Parse JSON elements
    let elements = parse_json_elements(inner)?;

    // Check if all elements are arrays (for multi-dimensional)
    let all_arrays = elements.iter().all(|e| {
        let e = e.trim();
        e.starts_with('[') && e.ends_with(']')
    });

    if all_arrays && !elements.is_empty() {
        let inner_arrays: Result<Vec<ArrayValue>, String> = elements
            .iter()
            .map(|e| parse_json_array(e.trim()))
            .collect();
        return Ok(ArrayValue::MultiD(inner_arrays?));
    }

    // Convert to ArrayValue::OneD
    let values: Vec<Option<String>> = elements
        .iter()
        .map(|e| {
            let e = e.trim();
            if e == "null" || e == "NULL" {
                None
            } else if e.starts_with('"') && e.ends_with('"') && e.len() >= 2 {
                // Unescape JSON string
                let s = &e[1..e.len() - 1];
                Some(unescape_json_string(s))
            } else if e.starts_with('\'') && e.ends_with('\'') && e.len() >= 2 {
                Some(e[1..e.len() - 1].to_string())
            } else {
                Some(e.to_string())
            }
        })
        .collect();

    Ok(ArrayValue::OneD(values))
}

/// Parse JSON elements, handling nested structures and strings
fn parse_json_elements(input: &str) -> Result<Vec<String>, String> {
    let mut elements = Vec::new();
    let mut current = String::new();
    let mut depth = 0;
    let mut in_string = false;
    let mut escape_next = false;
    let chars: Vec<char> = input.chars().collect();

    for c in chars {
        if escape_next {
            current.push(c);
            escape_next = false;
            continue;
        }

        match c {
            '\\' if in_string => {
                current.push(c);
                escape_next = true;
            }
            '"' => {
                current.push(c);
                in_string = !in_string;
            }
            '[' | '{' if !in_string => {
                current.push(c);
                depth += 1;
            }
            ']' | '}' if !in_string => {
                current.push(c);
                depth -= 1;
            }
            ',' if !in_string && depth == 0 => {
                elements.push(current.trim().to_string());
                current.clear();
            }
            _ => {
                current.push(c);
            }
        }
    }

    if !current.trim().is_empty() {
        elements.push(current.trim().to_string());
    }

    Ok(elements)
}

/// Unescape a JSON string
fn unescape_json_string(s: &str) -> String {
    let mut result = String::new();
    let mut escape_next = false;
    let chars: Vec<char> = s.chars().collect();

    for c in chars {
        if escape_next {
            match c {
                'n' => result.push('\n'),
                'r' => result.push('\r'),
                't' => result.push('\t'),
                '"' => result.push('"'),
                '\\' => result.push('\\'),
                _ => {
                    result.push('\\');
                    result.push(c);
                }
            }
            escape_next = false;
        } else if c == '\\' {
            escape_next = true;
        } else {
            result.push(c);
        }
    }

    result
}

/// Parse a PostgreSQL array literal
fn parse_pg_array(input: &str) -> Result<ArrayValue, String> {
    let trimmed = input.trim();

    // Handle explicit bounds notation: [2:4]={1,2,3}
    let content = if trimmed.starts_with('[') {
        // Find the closing bracket for bounds
        if let Some(eq_pos) = trimmed.find("]={") {
            &trimmed[eq_pos + 2..]
        } else if let Some(eq_pos) = trimmed.find("]={") {
            &trimmed[eq_pos + 2..]
        } else {
            trimmed
        }
    } else {
        trimmed
    };

    if !content.starts_with('{') || !content.ends_with('}') {
        return Err("PostgreSQL array must start with { and end with }".to_string());
    }

    let inner = &content[1..content.len() - 1].trim();

    if inner.is_empty() {
        return Ok(ArrayValue::Empty);
    }

    // Check for multi-dimensional array
    if inner.starts_with('{') {
        return parse_pg_multi_dim_array(content);
    }

    // Parse single-dimensional array
    let elements = parse_pg_elements(inner)?;
    Ok(ArrayValue::OneD(elements))
}

/// Parse a multi-dimensional PostgreSQL array
fn parse_pg_multi_dim_array(input: &str) -> Result<ArrayValue, String> {
    // Remove outer braces
    let inner = &input[1..input.len() - 1];

    // Split into inner arrays
    let mut arrays = Vec::new();
    let mut current = String::new();
    let mut depth = 0;
    let mut in_string = false;

    for c in inner.chars() {
        match c {
            '"' if depth == 0 => {
                in_string = !in_string;
            }
            '{' if !in_string => {
                depth += 1;
                if depth == 1 {
                    continue; // Skip outer brace
                }
            }
            '}' if !in_string => {
                depth -= 1;
                if depth == 0 {
                    // End of inner array
                    arrays.push(format!("{{{}}}", current));
                    current.clear();
                    continue;
                }
            }
            ',' if depth == 0 && !in_string => {
                // Separator between inner arrays - already handled
                continue;
            }
            _ => {}
        }

        if depth > 0 || in_string {
            current.push(c);
        }
    }

    // Parse each inner array
    let inner_arrays: Result<Vec<ArrayValue>, String> = arrays
        .iter()
        .map(|a| parse_pg_array(a))
        .collect();

    Ok(ArrayValue::MultiD(inner_arrays?))
}

/// Parse PostgreSQL array elements
fn parse_pg_elements(input: &str) -> Result<Vec<Option<String>>, String> {
    let mut elements = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut escape_next = false;
    let chars: Vec<char> = input.chars().collect();

    for c in chars {
        if escape_next {
            current.push(c);
            escape_next = false;
            continue;
        }

        match c {
            '\\' if in_quotes => {
                escape_next = true;
            }
            '"' => {
                in_quotes = !in_quotes;
            }
            ',' if !in_quotes => {
                let elem = current.trim();
                elements.push(if elem == "NULL" || elem.is_empty() {
                    None
                } else {
                    Some(elem.to_string())
                });
                current.clear();
            }
            _ => {
                current.push(c);
            }
        }
    }

    // Don't forget the last element
    let elem = current.trim();
    if !elem.is_empty() || !elements.is_empty() {
        elements.push(if elem == "NULL" || elem.is_empty() {
            None
        } else {
            Some(elem.to_string())
        });
    }

    Ok(elements)
}

// ============================================================================
// Array Operators
// ============================================================================

/// Check if two arrays overlap (have any elements in common)
/// PostgreSQL: arr1 && arr2
pub fn array_overlap(left: &str, right: &str) -> Result<bool, String> {
    let left_arr = parse_array(left)?;
    let right_arr = parse_array(right)?;

    let left_set: HashSet<Option<String>> = left_arr.flatten().into_iter().collect();
    let right_set: HashSet<Option<String>> = right_arr.flatten().into_iter().collect();

    // Check if any element from right is in left
    for elem in &right_set {
        if left_set.contains(elem) {
            return Ok(true);
        }
    }

    Ok(false)
}

/// Check if left array contains all elements of right array
/// PostgreSQL: arr1 @> arr2
pub fn array_contains(left: &str, right: &str) -> Result<bool, String> {
    let left_arr = parse_array(left)?;
    let right_arr = parse_array(right)?;

    let left_set: HashSet<Option<String>> = left_arr.flatten().into_iter().collect();
    let right_set: HashSet<Option<String>> = right_arr.flatten().into_iter().collect();

    // All elements of right must be in left
    for elem in &right_set {
        if !left_set.contains(elem) {
            return Ok(false);
        }
    }

    Ok(true)
}

/// Check if left array is contained by right array
/// PostgreSQL: arr1 <@ arr2
pub fn array_contained(left: &str, right: &str) -> Result<bool, String> {
    array_contains(right, left)
}

/// Concatenate two arrays
/// PostgreSQL: arr1 || arr2
pub fn array_concat(left: &str, right: &str) -> Result<String, String> {
    let left_arr = parse_array(left)?;
    let right_arr = parse_array(right)?;

    // If both are 1D, just combine
    if let (ArrayValue::OneD(l), ArrayValue::OneD(r)) = (&left_arr, &right_arr) {
        let mut combined = l.clone();
        combined.extend(r.iter().cloned());
        return Ok(ArrayValue::OneD(combined).to_postgres_string());
    }

    // Otherwise, try to maintain structure
    let mut left_flat = left_arr.flatten();
    let right_flat = right_arr.flatten();
    left_flat.extend(right_flat);

    Ok(ArrayValue::OneD(left_flat).to_postgres_string())
}

/// Append an element to an array
/// PostgreSQL: array_append(arr, elem)
pub fn array_append(arr: &str, elem: &str) -> Result<String, String> {
    // Handle NULL array
    if arr.is_empty() || arr == "NULL" {
        return Ok(format!("{{{}}}", elem));
    }

    let mut array = parse_array(arr)?;
    let elem_value = if elem == "NULL" {
        None
    } else {
        Some(elem.to_string())
    };

    match &mut array {
        ArrayValue::Empty => {
            Ok(ArrayValue::OneD(vec![elem_value]).to_postgres_string())
        }
        ArrayValue::OneD(v) => {
            v.push(elem_value);
            Ok(array.to_postgres_string())
        }
        ArrayValue::MultiD(_) => {
            // For multi-dimensional, flatten and append
            let mut flat = array.flatten();
            flat.push(elem_value);
            Ok(ArrayValue::OneD(flat).to_postgres_string())
        }
    }
}

/// Prepend an element to an array
/// PostgreSQL: array_prepend(elem, arr)
pub fn array_prepend(elem: &str, arr: &str) -> Result<String, String> {
    // Handle NULL array
    if arr.is_empty() || arr == "NULL" {
        return Ok(format!("{{{}}}", elem));
    }

    let mut array = parse_array(arr)?;
    let elem_value = if elem == "NULL" {
        None
    } else {
        Some(elem.to_string())
    };

    match &mut array {
        ArrayValue::Empty => {
            Ok(ArrayValue::OneD(vec![elem_value]).to_postgres_string())
        }
        ArrayValue::OneD(v) => {
            v.insert(0, elem_value);
            Ok(array.to_postgres_string())
        }
        ArrayValue::MultiD(_) => {
            let mut flat = array.flatten();
            flat.insert(0, elem_value);
            Ok(ArrayValue::OneD(flat).to_postgres_string())
        }
    }
}

/// Concatenate two arrays (same as ||)
/// PostgreSQL: array_cat(arr1, arr2)
pub fn array_cat(left: &str, right: &str) -> Result<String, String> {
    array_concat(left, right)
}

/// Remove all occurrences of an element from an array
/// PostgreSQL: array_remove(arr, elem)
pub fn array_remove(arr: &str, elem: &str) -> Result<String, String> {
    if arr.is_empty() || arr == "NULL" {
        return Ok("{}".to_string());
    }

    let array = parse_array(arr)?;
    let elem_value = if elem == "NULL" {
        None
    } else {
        Some(elem.to_string())
    };

    let flat = array.flatten();
    let filtered: Vec<Option<String>> = flat
        .into_iter()
        .filter(|e| e != &elem_value)
        .collect();

    Ok(ArrayValue::OneD(filtered).to_postgres_string())
}

/// Replace all occurrences of an element in an array
/// PostgreSQL: array_replace(arr, old, new)
pub fn array_replace(arr: &str, old: &str, new: &str) -> Result<String, String> {
    if arr.is_empty() || arr == "NULL" {
        return Ok("{}".to_string());
    }

    let array = parse_array(arr)?;
    let old_value = if old == "NULL" { None } else { Some(old.to_string()) };
    let new_value = if new == "NULL" { None } else { Some(new.to_string()) };

    let flat = array.flatten();
    let replaced: Vec<Option<String>> = flat
        .into_iter()
        .map(|e| if e == old_value { new_value.clone() } else { e })
        .collect();

    Ok(ArrayValue::OneD(replaced).to_postgres_string())
}

// ============================================================================
// Array Information Functions
// ============================================================================

/// Get the length of an array in a specific dimension
/// PostgreSQL: array_length(arr, dim)
pub fn array_length_fn(arr: &str, dim: i32) -> Result<Option<i64>, String> {
    if arr.is_empty() || arr == "NULL" {
        return Ok(None);
    }

    let array = parse_array(arr)?;
    Ok(array.length(dim))
}

/// Get the lower bound of an array dimension
/// PostgreSQL: array_lower(arr, dim)
pub fn array_lower_fn(arr: &str, dim: i32) -> Result<Option<i32>, String> {
    if arr.is_empty() || arr == "NULL" {
        return Ok(None);
    }

    // For standard arrays, lower bound is always 1
    let array = parse_array(arr)?;
    if array.length(dim).is_some() {
        Ok(Some(1))
    } else {
        Ok(None)
    }
}

/// Get the upper bound of an array dimension
/// PostgreSQL: array_upper(arr, dim)
pub fn array_upper_fn(arr: &str, dim: i32) -> Result<Option<i32>, String> {
    if arr.is_empty() || arr == "NULL" {
        return Ok(None);
    }

    let array = parse_array(arr)?;
    match array.length(dim) {
        Some(len) => Ok(Some(len as i32)),
        None => Ok(None),
    }
}

/// Get the number of dimensions of an array
/// PostgreSQL: array_ndims(arr)
pub fn array_ndims_fn(arr: &str) -> Result<i32, String> {
    if arr.is_empty() || arr == "NULL" {
        return Ok(0);
    }

    let array = parse_array(arr)?;
    Ok(array.ndims())
}

/// Get a text representation of array dimensions
/// PostgreSQL: array_dims(arr)
pub fn array_dims_fn(arr: &str) -> Result<String, String> {
    if arr.is_empty() || arr == "NULL" {
        return Ok(String::new());
    }

    let array = parse_array(arr)?;
    let ndims = array.ndims();

    if ndims == 0 {
        return Ok(String::new());
    }

    let mut dims = Vec::new();
    for dim in 1..=ndims {
        if let Some(len) = array.length(dim) {
            dims.push(format!("[1:{}]", len));
        }
    }

    Ok(dims.join(""))
}

/// Get the total number of elements in an array
/// PostgreSQL: cardinality(arr)
pub fn array_cardinality(arr: &str) -> Result<i64, String> {
    if arr.is_empty() || arr == "NULL" {
        return Ok(0);
    }

    let array = parse_array(arr)?;
    Ok(array.cardinality())
}

// ============================================================================
// Array Search Functions
// ============================================================================

/// Find the position of the first occurrence of an element
/// PostgreSQL: array_position(arr, elem [, start])
pub fn array_position_fn(arr: &str, elem: &str, start: Option<i32>) -> Result<Option<i32>, String> {
    if arr.is_empty() || arr == "NULL" {
        return Ok(None);
    }

    let array = parse_array(arr)?;
    let elem_value = if elem == "NULL" { None } else { Some(elem.to_string()) };
    let flat = array.flatten();
    let start_idx = start.unwrap_or(1).max(1) as usize;

    for (i, e) in flat.iter().enumerate() {
        if i + 1 >= start_idx && e == &elem_value {
            return Ok(Some((i + 1) as i32));
        }
    }

    Ok(None)
}

/// Find all positions of an element in an array
/// PostgreSQL: array_positions(arr, elem)
pub fn array_positions_fn(arr: &str, elem: &str) -> Result<String, String> {
    if arr.is_empty() || arr == "NULL" {
        return Ok("{}".to_string());
    }

    let array = parse_array(arr)?;
    let elem_value = if elem == "NULL" { None } else { Some(elem.to_string()) };
    let flat = array.flatten();

    let positions: Vec<String> = flat
        .iter()
        .enumerate()
        .filter_map(|(i, e)| {
            if e == &elem_value {
                Some((i + 1).to_string())
            } else {
                None
            }
        })
        .collect();

    Ok(format!("{{{}}}", positions.join(",")))
}

// ============================================================================
// Array Conversion Functions
// ============================================================================

/// Convert an array to a delimited string
/// PostgreSQL: array_to_string(arr, delimiter [, null_string])
pub fn array_to_string_fn(arr: &str, delimiter: &str, null_string: Option<&str>) -> Result<String, String> {
    if arr.is_empty() || arr == "NULL" {
        return Ok(String::new());
    }

    let array = parse_array(arr)?;
    let flat = array.flatten();

    let parts: Vec<String> = flat
        .iter()
        .map(|e| match e {
            Some(s) => s.clone(),
            None => null_string.unwrap_or("").to_string(),
        })
        .collect();

    Ok(parts.join(delimiter))
}

/// Split a string into an array
/// PostgreSQL: string_to_array(text, delimiter [, null_string])
pub fn string_to_array_fn(text: &str, delimiter: &str, null_string: Option<&str>) -> Result<String, String> {
    if text.is_empty() {
        return Ok("{}".to_string());
    }

    if delimiter.is_empty() {
        // Return array of individual characters
        let chars: Vec<String> = text.chars().map(|c| c.to_string()).collect();
        return Ok(ArrayValue::OneD(chars.into_iter().map(Some).collect()).to_postgres_string());
    }

    let null_val = null_string.unwrap_or("");

    let elements: Vec<Option<String>> = text
        .split(delimiter)
        .map(|s| {
            if s == null_val && !null_val.is_empty() {
                None
            } else {
                Some(s.to_string())
            }
        })
        .collect();

    Ok(ArrayValue::OneD(elements).to_postgres_string())
}

/// Create an array filled with a value
/// PostgreSQL: array_fill(value, dimensions [, lower_bounds])
pub fn array_fill_fn(value: &str, dimensions: &str, _lower_bounds: Option<&str>) -> Result<String, String> {
    let dims = parse_array(dimensions)?;
    let dim_values: Vec<i64> = dims
        .flatten()
        .into_iter()
        .filter_map(|v| v.and_then(|s| s.parse().ok()))
        .collect();

    if dim_values.is_empty() {
        return Ok("{}".to_string());
    }

    let elem_value = if value == "NULL" { None } else { Some(value.to_string()) };

    fn fill_recursive(value: &Option<String>, dims: &[i64]) -> ArrayValue {
        if dims.is_empty() {
            return ArrayValue::Empty;
        }

        let size = dims[0] as usize;
        if dims.len() == 1 {
            ArrayValue::OneD(vec![value.clone(); size])
        } else {
            ArrayValue::MultiD((0..size).map(|_| fill_recursive(value, &dims[1..])).collect())
        }
    }

    Ok(fill_recursive(&elem_value, &dim_values).to_postgres_string())
}

/// Remove n elements from the end of an array
/// PostgreSQL: trim_array(arr, n)
pub fn trim_array_fn(arr: &str, n: i32) -> Result<String, String> {
    if arr.is_empty() || arr == "NULL" {
        return Ok("{}".to_string());
    }

    let array = parse_array(arr)?;
    let mut flat = array.flatten();

    let trim_count = n.max(0) as usize;
    if trim_count >= flat.len() {
        return Ok("{}".to_string());
    }

    flat.truncate(flat.len() - trim_count);
    Ok(ArrayValue::OneD(flat).to_postgres_string())
}

// ============================================================================
// Array Comparison Functions
// ============================================================================

/// Compare two arrays for equality
pub fn array_eq(left: &str, right: &str) -> Result<bool, String> {
    let left_arr = parse_array(left)?;
    let right_arr = parse_array(right)?;

    // Check dimensions first
    if left_arr.ndims() != right_arr.ndims() {
        return Ok(false);
    }

    let left_flat = left_arr.flatten();
    let right_flat = right_arr.flatten();

    if left_flat.len() != right_flat.len() {
        return Ok(false);
    }

    Ok(left_flat == right_flat)
}

/// Compare two arrays for inequality
pub fn array_ne(left: &str, right: &str) -> Result<bool, String> {
    let eq = array_eq(left, right)?;
    Ok(!eq)
}

/// Compare two arrays lexicographically (less than)
pub fn array_lt(left: &str, right: &str) -> Result<bool, String> {
    let left_arr = parse_array(left)?;
    let right_arr = parse_array(right)?;

    let left_flat = left_arr.flatten();
    let right_flat = right_arr.flatten();

    // Lexicographic comparison
    for (l, r) in left_flat.iter().zip(right_flat.iter()) {
        match (l, r) {
            (Some(lv), Some(rv)) => {
                // Try numeric comparison first
                if let (Ok(ln), Ok(rn)) = (lv.parse::<f64>(), rv.parse::<f64>()) {
                    if ln < rn {
                        return Ok(true);
                    } else if ln > rn {
                        return Ok(false);
                    }
                } else {
                    // String comparison
                    if lv < rv {
                        return Ok(true);
                    } else if lv > rv {
                        return Ok(false);
                    }
                }
            }
            (None, Some(_)) => return Ok(true), // NULL < value
            (Some(_), None) => return Ok(false), // value > NULL
            (None, None) => {} // NULL == NULL, continue
        }
    }

    // If all compared elements are equal, shorter array is less
    Ok(left_flat.len() < right_flat.len())
}

/// Compare two arrays lexicographically (greater than)
pub fn array_gt(left: &str, right: &str) -> Result<bool, String> {
    let eq = array_eq(left, right)?;
    if eq {
        return Ok(false);
    }
    Ok(!array_lt(left, right)?)
}

/// Compare two arrays lexicographically (less than or equal)
pub fn array_le(left: &str, right: &str) -> Result<bool, String> {
    let eq = array_eq(left, right)?;
    if eq {
        return Ok(true);
    }
    array_lt(left, right)
}

/// Compare two arrays lexicographically (greater than or equal)
pub fn array_ge(left: &str, right: &str) -> Result<bool, String> {
    let eq = array_eq(left, right)?;
    if eq {
        return Ok(true);
    }
    array_gt(left, right)
}

// ============================================================================
// ANY/ALL Support
// ============================================================================

/// Check if a value equals any element in the array
/// PostgreSQL: value = ANY(array)
pub fn array_any_eq(value: &str, arr: &str) -> Result<bool, String> {
    if arr.is_empty() || arr == "NULL" {
        return Ok(false);
    }

    let array = parse_array(arr)?;
    let flat = array.flatten();

    for elem in flat {
        if let Some(e) = elem {
            // Try numeric comparison
            if let (Ok(vn), Ok(en)) = (value.parse::<f64>(), e.parse::<f64>()) {
                if (vn - en).abs() < f64::EPSILON {
                    return Ok(true);
                }
            } else if e == value {
                return Ok(true);
            }
        }
    }

    Ok(false)
}

/// Check if a value matches all elements in the array
/// PostgreSQL: value = ALL(array)
pub fn array_all_eq(value: &str, arr: &str) -> Result<bool, String> {
    if arr.is_empty() || arr == "NULL" {
        return Ok(true); // Vacuously true
    }

    let array = parse_array(arr)?;
    let flat = array.flatten();

    if flat.is_empty() {
        return Ok(true);
    }

    for elem in flat {
        if let Some(e) = elem {
            // Try numeric comparison
            if let (Ok(vn), Ok(en)) = (value.parse::<f64>(), e.parse::<f64>()) {
                if (vn - en).abs() >= f64::EPSILON {
                    return Ok(false);
                }
            } else if e != value {
                return Ok(false);
            }
        } else {
            return Ok(false);
        }
    }

    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_json_array() {
        let arr = parse_array("[1,2,3]").unwrap();
        assert_eq!(arr.ndims(), 1);
        assert_eq!(arr.cardinality(), 3);
    }

    #[test]
    fn test_parse_pg_array() {
        let arr = parse_array("{1,2,3}").unwrap();
        assert_eq!(arr.ndims(), 1);
        assert_eq!(arr.cardinality(), 3);
    }

    #[test]
    fn test_parse_empty_array() {
        let arr = parse_array("[]").unwrap();
        assert_eq!(arr, ArrayValue::Empty);

        let arr = parse_array("{}").unwrap();
        assert_eq!(arr, ArrayValue::Empty);
    }

    #[test]
    fn test_array_overlap() {
        assert!(array_overlap("[1,2,3]", "[3,4]").unwrap());
        assert!(!array_overlap("[1,2,3]", "[4,5]").unwrap());
        assert!(array_overlap("{1,2,3}", "{3,4}").unwrap());
    }

    #[test]
    fn test_array_contains() {
        assert!(array_contains("[1,2,3]", "[1,2]").unwrap());
        assert!(!array_contains("[1,2,3]", "[1,4]").unwrap());
        assert!(array_contains("{1,2,3}", "{1,2,1}").unwrap()); // Duplicates don't matter
    }

    #[test]
    fn test_array_contained() {
        assert!(array_contained("[1,2]", "[1,2,3]").unwrap());
        assert!(!array_contained("[1,4]", "[1,2,3]").unwrap());
    }

    #[test]
    fn test_array_concat() {
        let result = array_concat("[1,2]", "[3,4]").unwrap();
        assert_eq!(result, "{1,2,3,4}");

        let result = array_concat("{1,2}", "{3,4}").unwrap();
        assert_eq!(result, "{1,2,3,4}");
    }

    #[test]
    fn test_array_append() {
        let result = array_append("[1,2]", "3").unwrap();
        assert_eq!(result, "{1,2,3}");

        let result = array_append("{}", "1").unwrap();
        assert_eq!(result, "{1}");
    }

    #[test]
    fn test_array_prepend() {
        let result = array_prepend("0", "[1,2]").unwrap();
        assert_eq!(result, "{0,1,2}");
    }

    #[test]
    fn test_array_remove() {
        let result = array_remove("[1,2,2,3]", "2").unwrap();
        assert_eq!(result, "{1,3}");

        let result = array_remove("{a,b,a}", "a").unwrap();
        assert_eq!(result, "{b}");
    }

    #[test]
    fn test_array_replace() {
        let result = array_replace("[1,2,2,3]", "2", "9").unwrap();
        assert_eq!(result, "{1,9,9,3}");
    }

    #[test]
    fn test_array_length() {
        assert_eq!(array_length_fn("[1,2,3]", 1).unwrap(), Some(3));
        assert_eq!(array_length_fn("{}", 1).unwrap(), None);
    }

    #[test]
    fn test_array_ndims() {
        assert_eq!(array_ndims_fn("[1,2,3]").unwrap(), 1);
        assert_eq!(array_ndims_fn("{{1,2},{3,4}}").unwrap(), 2);
        assert_eq!(array_ndims_fn("{}").unwrap(), 0);
    }

    #[test]
    fn test_array_cardinality() {
        assert_eq!(array_cardinality("[1,2,3]").unwrap(), 3);
        assert_eq!(array_cardinality("{{1,2},{3,4}}").unwrap(), 4);
        assert_eq!(array_cardinality("[]").unwrap(), 0);
    }

    #[test]
    fn test_array_position() {
        assert_eq!(array_position_fn("[1,2,3,2]", "2", None).unwrap(), Some(2));
        assert_eq!(array_position_fn("[1,2,3,2]", "2", Some(3)).unwrap(), Some(4));
        assert_eq!(array_position_fn("[1,2,3]", "4", None).unwrap(), None);
    }

    #[test]
    fn test_array_positions() {
        let result = array_positions_fn("[1,2,1,3,1]", "1").unwrap();
        assert_eq!(result, "{1,3,5}");

        let result = array_positions_fn("[1,2,3]", "4").unwrap();
        assert_eq!(result, "{}");
    }

    #[test]
    fn test_array_to_string() {
        let result = array_to_string_fn("[a,b,c]", ",", None).unwrap();
        assert_eq!(result, "a,b,c");

        let result = array_to_string_fn("[a,null,c]", ",", Some("*")).unwrap();
        assert_eq!(result, "a,*,c");
    }

    #[test]
    fn test_string_to_array() {
        let result = string_to_array_fn("a,b,c", ",", None).unwrap();
        assert_eq!(result, "{a,b,c}");

        let result = string_to_array_fn("a,*,c", ",", Some("*")).unwrap();
        // Note: NULL in array format
        assert!(result.contains("a"));
        assert!(result.contains("c"));
    }

    #[test]
    fn test_trim_array() {
        let result = trim_array_fn("[1,2,3,4,5]", 2).unwrap();
        assert_eq!(result, "{1,2,3}");

        let result = trim_array_fn("[1,2]", 5).unwrap();
        assert_eq!(result, "{}");
    }

    #[test]
    fn test_array_fill() {
        let result = array_fill_fn("7", "[3]", None).unwrap();
        assert_eq!(result, "{7,7,7}");

        let result = array_fill_fn("0", "[2,3]", None).unwrap();
        assert_eq!(result, "{{0,0,0},{0,0,0}}");
    }

    #[test]
    fn test_array_eq() {
        assert!(array_eq("[1,2,3]", "[1,2,3]").unwrap());
        assert!(!array_eq("[1,2,3]", "[1,2]").unwrap());
        assert!(array_eq("{}", "{}").unwrap());
    }

    #[test]
    fn test_array_comparison() {
        assert!(array_lt("[1,2,3]", "[1,2,4]").unwrap());
        assert!(array_lt("[1,2]", "[1,2,3]").unwrap());
        assert!(array_gt("[1,2,4]", "[1,2,3]").unwrap());
    }

    #[test]
    fn test_array_any() {
        assert!(array_any_eq("3", "[1,2,3]").unwrap());
        assert!(!array_any_eq("4", "[1,2,3]").unwrap());
    }

    #[test]
    fn test_array_all() {
        assert!(array_all_eq("3", "[3,3,3]").unwrap());
        assert!(!array_all_eq("3", "[3,2,3]").unwrap());
    }

    #[test]
    fn test_multi_dim_array() {
        let arr = parse_array("{{1,2},{3,4}}").unwrap();
        assert_eq!(arr.ndims(), 2);
        assert_eq!(arr.cardinality(), 4);
        assert_eq!(arr.length(1), Some(2));
        assert_eq!(arr.length(2), Some(2));
    }

    #[test]
    fn test_null_elements() {
        let arr = parse_array("[1,null,3]").unwrap();
        let flat = arr.flatten();
        assert_eq!(flat.len(), 3);
        assert_eq!(flat[0], Some("1".to_string()));
        assert_eq!(flat[1], None);
        assert_eq!(flat[2], Some("3".to_string()));
    }

    #[test]
    fn test_to_postgres_string() {
        let arr = parse_array("[1,2,3]").unwrap();
        assert_eq!(arr.to_postgres_string(), "{1,2,3}");

        let arr = parse_array("[a,b,c]").unwrap();
        assert_eq!(arr.to_postgres_string(), "{a,b,c}");
    }
}
