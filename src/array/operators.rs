//! Array operators: && (overlap), @> (contains), <@ (contained by)

use std::collections::HashSet;
use crate::array::utils::parse_array;

/// Check if two arrays overlap (have any elements in common)
/// PostgreSQL: arr1 && arr2
pub fn array_overlap(left: &str, right: &str) -> Result<bool, String> {
    let left_arr = parse_array(left)?;
    let right_arr = parse_array(right)?;

    let left_set: HashSet<Option<String>> = left_arr.flatten().into_iter().collect();
    let right_set: HashSet<Option<String>> = right_arr.flatten().into_iter().collect();

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
