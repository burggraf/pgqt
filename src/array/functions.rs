//! Array functions: array_append, array_prepend, array_cat, array_position, array_remove, etc.

// These functions are part of the public array API
#![allow(dead_code)]

use crate::array::types::ArrayValue;
use crate::array::utils::parse_array;

/// Concatenate two arrays
/// PostgreSQL: arr1 || arr2
pub fn array_concat(left: &str, right: &str) -> Result<String, String> {
    let left_arr = parse_array(left)?;
    let right_arr = parse_array(right)?;

    // If both are 1D, keep them as 1D
    if let (ArrayValue::OneD(l), ArrayValue::OneD(r)) = (&left_arr, &right_arr) {
        let mut combined = l.clone();
        combined.extend(r.iter().cloned());
        return Ok(ArrayValue::OneD(combined).to_postgres_string());
    }

    // Otherwise flatten and combine
    let mut left_flat = left_arr.flatten();
    let right_flat = right_arr.flatten();
    left_flat.extend(right_flat);

    Ok(ArrayValue::OneD(left_flat).to_postgres_string())
}

/// Append an element to an array
/// PostgreSQL: array_append(arr, elem)
pub fn array_append(arr: &str, elem: &str) -> Result<String, String> {
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
        ArrayValue::OneD(v) => {
            v.push(elem_value);
            Ok(array.to_postgres_string())
        }
        _ => {
            let mut flat = array.flatten();
            flat.push(elem_value);
            Ok(ArrayValue::OneD(flat).to_postgres_string())
        }
    }
}

/// Prepend an element to an array
/// PostgreSQL: array_prepend(elem, arr)
pub fn array_prepend(elem: &str, arr: &str) -> Result<String, String> {
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
        ArrayValue::OneD(v) => {
            v.insert(0, elem_value);
            Ok(array.to_postgres_string())
        }
        _ => {
            let mut flat = array.flatten();
            flat.insert(0, elem_value);
            Ok(ArrayValue::OneD(flat).to_postgres_string())
        }
    }
}

/// Concatenate two arrays (alias for array_concat)
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

/// Replace all occurrences of an element with another
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

    // PostgreSQL arrays are 1-indexed by default
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

    for (i, e) in flat.iter().enumerate().skip(start_idx - 1) {
        if e == &elem_value {
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
        .filter(|(_, e)| e == &&elem_value)
        .map(|(i, _)| (i + 1).to_string())
        .collect();

    Ok(format!("{{{}}}", positions.join(",")))
}

/// Convert an array to a string with delimiter
/// PostgreSQL: array_to_string(arr, delimiter [, null_string])
pub fn array_to_string_fn(arr: &str, delimiter: &str, null_string: Option<&str>) -> Result<String, String> {
    if arr.is_empty() || arr == "NULL" {
        return Ok(String::new());
    }

    let array = parse_array(arr)?;
    let flat = array.flatten();

    let parts: Vec<String> = flat
        .iter()
        .filter_map(|e| match e {
            Some(s) => Some(s.clone()),
            None => null_string.map(|s| s.to_string()),
        })
        .collect();

    Ok(parts.join(delimiter))
}

/// Convert a string to an array using delimiter
/// PostgreSQL: string_to_array(text, delimiter [, null_string])
pub fn string_to_array_fn(text: &str, delimiter: &str, null_string: Option<&str>) -> Result<String, String> {
    if text.is_empty() || text == "NULL" {
        return Ok("{}".to_string());
    }

    if delimiter.is_empty() {
        // Split into individual characters
        let chars: Vec<String> = text.chars().map(|c| c.to_string()).collect();
        let elements: Vec<Option<String>> = chars.into_iter().map(Some).collect();
        return Ok(ArrayValue::OneD(elements).to_postgres_string());
    }

    let null_val = null_string.unwrap_or("");
    let elements: Vec<Option<String>> = text
        .split(delimiter)
        .map(|s| {
            if s == null_val {
                None
            } else {
                // If the element is quoted, strip quotes
                let s = s.trim();
                if s.starts_with('"') && s.ends_with('"') && s.len() >= 2 {
                    Some(s[1..s.len() - 1].to_string())
                } else {
                    Some(s.to_string())
                }
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
