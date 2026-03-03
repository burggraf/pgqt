//! Array comparison functions: array_eq, array_ne, array_lt, array_gt, etc.

use crate::array::types::ArrayValue;
use crate::array::utils::parse_array;

/// Compare two arrays for equality
pub fn array_eq(left: &str, right: &str) -> Result<bool, String> {
    let left_arr = parse_array(left)?;
    let right_arr = parse_array(right)?;

    // Check dimensions match
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

    // Compare element by element
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
            (None, Some(_)) => return Ok(true),  // NULL is less than non-NULL
            (Some(_), None) => return Ok(false),
            (None, None) => {}  // Equal, continue
        }
    }

    // If all compared elements are equal, shorter array is less
    Ok(left_flat.len() < right_flat.len())
}

/// Compare two arrays lexicographically (greater than)
pub fn array_gt(left: &str, right: &str) -> Result<bool, String> {
    let eq = array_eq(left, right)?;
    let lt = array_lt(left, right)?;
    Ok(!eq && !lt)
}

/// Compare two arrays lexicographically (less than or equal)
pub fn array_le(left: &str, right: &str) -> Result<bool, String> {
    let eq = array_eq(left, right)?;
    let lt = array_lt(left, right)?;
    Ok(eq || lt)
}

/// Compare two arrays lexicographically (greater than or equal)
pub fn array_ge(left: &str, right: &str) -> Result<bool, String> {
    let eq = array_eq(left, right)?;
    let lt = array_lt(left, right)?;
    Ok(eq || !lt)
}

/// Check if any element in array equals value
pub fn array_any_eq(value: &str, arr: &str) -> Result<bool, String> {
    if arr.is_empty() || arr == "NULL" {
        return Ok(false);
    }

    let array = parse_array(arr)?;
    let flat = array.flatten();

    let target = if value == "NULL" { None } else { Some(value.to_string()) };

    for e in flat {
        if e == target {
            return Ok(true);
        }
    }

    Ok(false)
}

/// Check if all elements in array equal value
pub fn array_all_eq(value: &str, arr: &str) -> Result<bool, String> {
    if arr.is_empty() || arr == "NULL" {
        return Ok(true);
    }

    let array = parse_array(arr)?;
    let flat = array.flatten();

    let target = if value == "NULL" { None } else { Some(value.to_string()) };

    for e in flat {
        if e != target {
            return Ok(false);
        }
    }

    Ok(true)
}
