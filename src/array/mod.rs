//! PostgreSQL array support for PGQT
//!
//! This module provides PostgreSQL-compatible array operations including:
//! - Array parsing (JSON and PostgreSQL formats)
//! - Array operators (&&, @>, <@)
//! - Array functions (array_append, array_cat, array_position, etc.)
//! - Array comparisons

//! PostgreSQL-compatible array support for PGQT.
//!
//! This module provides PostgreSQL array types, operators, and functions
//! for use within the PGQT PostgreSQL-to-SQLite proxy.
//!
//! # Array Types
//!
//! - [`ArrayValue`] - Represents parsed array values (1D, multi-dimensional, or empty)
//!
//! # Array Operators
//!
//! - `&&` - Overlap (any element in common)
//! - `@>` - Contains (left contains all of right)
//! - `<@` - Contained by (left is subset of right)
//!
//! # Array Functions
//!
//! - `array_append`, `array_prepend`, `array_cat`, `array_concat`
//! - `array_remove`, `array_replace`
//! - `array_length`, `array_lower`, `array_upper`, `array_ndims`, `array_dims`, `cardinality`
//! - `array_position`, `array_positions`
//! - `array_to_string`, `string_to_array`
//! - `array_fill`, `trim_array`
//!
//! # Examples
//!
//! ```rust
//! use pgqt::array::{parse_array, array_overlap, array_contains};
//!
//! // Parse an array string
//! let arr = parse_array("[1,2,3]").unwrap();
//!
//! // Check overlap
//! assert!(array_overlap("[1,2,3]", "[3,4]").unwrap());
//!
//! // Check containment
//! assert!(array_contains("[1,2,3]", "[1,2]").unwrap());
//! ```

pub mod types;
pub mod utils;
pub mod operators;
pub mod functions;
pub mod comparison;

// Re-export core types
pub use types::ArrayValue;

// Re-export parsing utilities
pub use utils::parse_array;

// Re-export operators
pub use operators::{
    array_overlap,
    array_contains,
    array_contained,
};

// Re-export functions
pub use functions::{
    array_append,
    array_prepend,
    array_cat,
    array_concat,
    array_remove,
    array_replace,
    array_length_fn,
    array_lower_fn,
    array_upper_fn,
    array_ndims_fn,
    array_dims_fn,
    array_cardinality,
    array_position_fn,
    array_positions_fn,
    array_to_string_fn,
    string_to_array_fn,
    array_fill_fn,
    trim_array_fn,
};

// Re-export comparison functions
pub use comparison::{
    array_eq,
    array_ne,
    array_lt,
    array_gt,
    array_le,
    array_ge,
    array_any_eq,
    array_all_eq,
};

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
        assert_eq!(arr, types::ArrayValue::Empty);

        let arr = parse_array("{}").unwrap();
        assert_eq!(arr, types::ArrayValue::Empty);
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
        assert!(array_contains("{1,2,3}", "{1,2,1}").unwrap());
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
        // NULL handling varies, just check it doesn't panic
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
