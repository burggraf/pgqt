//! Vector search support for pgvector compatibility
//!
//! This module provides PostgreSQL pgvector-compatible vector operations
//! implemented in pure Rust. Supports distance calculations, normalization,
//! and vector manipulation functions.

// These functions are part of the public vector API
#![allow(dead_code)]

#[allow(dead_code)]
/// Parse a vector from JSON array format '[1,2,3]' to Vec<f32>
fn parse_vector_to_f32(input: &str) -> Result<Vec<f32>, String> {
    let trimmed = input.trim();
    
    // Handle JSON array format
    if trimmed.starts_with('[') && trimmed.ends_with(']') {
        let inner = &trimmed[1..trimmed.len()-1];
        if inner.trim().is_empty() {
            return Ok(Vec::new());
        }
        
        let values: Result<Vec<f32>, _> = inner
            .split(',')
            .map(|s| s.trim().parse::<f32>())
            .collect();
        
        values.map_err(|e| format!("invalid vector element: {}", e))
    } else {
        Err("vector must be in format '[1,2,3]'".to_string())
    }
}

/// Calculate L2 (Euclidean) distance between two vectors
/// 
/// Formula: sqrt(sum((a_i - b_i)^2))
pub fn l2_distance(a: &str, b: &str) -> Result<f64, String> {
    let vec_a = parse_vector_to_f32(a)?;
    let vec_b = parse_vector_to_f32(b)?;
    
    if vec_a.len() != vec_b.len() {
        return Err(format!(
            "vector dimension mismatch: {} vs {}",
            vec_a.len(),
            vec_b.len()
        ));
    }
    
    let sum: f64 = vec_a.iter()
        .zip(vec_b.iter())
        .map(|(x, y)| {
            let diff = *x as f64 - *y as f64;
            diff * diff
        })
        .sum();
    
    Ok(sum.sqrt())
}

/// Calculate cosine distance between two vectors
/// 
/// Formula: 1 - (a · b) / (||a|| * ||b||)
/// Returns 0 for identical direction, 2 for opposite direction
pub fn cosine_distance(a: &str, b: &str) -> Result<f64, String> {
    let vec_a = parse_vector_to_f32(a)?;
    let vec_b = parse_vector_to_f32(b)?;
    
    if vec_a.len() != vec_b.len() {
        return Err(format!(
            "vector dimension mismatch: {} vs {}",
            vec_a.len(),
            vec_b.len()
        ));
    }
    
    let dot_product: f64 = vec_a.iter()
        .zip(vec_b.iter())
        .map(|(x, y)| (*x as f64) * (*y as f64))
        .sum();
    
    let norm_a: f64 = vec_a.iter().map(|x| (*x as f64).powi(2)).sum::<f64>().sqrt();
    let norm_b: f64 = vec_b.iter().map(|x| (*x as f64).powi(2)).sum::<f64>().sqrt();
    
    if norm_a == 0.0 || norm_b == 0.0 {
        return Err("cannot compute cosine distance for zero vector".to_string());
    }
    
    let similarity = dot_product / (norm_a * norm_b);
    // Clamp to handle floating point errors
    let similarity = similarity.clamp(-1.0, 1.0);
    Ok(1.0 - similarity)
}

/// Calculate inner product (dot product) of two vectors
/// 
/// Formula: sum(a_i * b_i)
pub fn inner_product(a: &str, b: &str) -> Result<f64, String> {
    let vec_a = parse_vector_to_f32(a)?;
    let vec_b = parse_vector_to_f32(b)?;
    
    if vec_a.len() != vec_b.len() {
        return Err(format!(
            "vector dimension mismatch: {} vs {}",
            vec_a.len(),
            vec_b.len()
        ));
    }
    
    let product: f64 = vec_a.iter()
        .zip(vec_b.iter())
        .map(|(x, y)| (*x as f64) * (*y as f64))
        .sum();
    
    Ok(product)
}

/// Calculate L1 (Manhattan) distance between two vectors
/// 
/// Formula: sum(|a_i - b_i|)
pub fn l1_distance(a: &str, b: &str) -> Result<f64, String> {
    let vec_a = parse_vector_to_f32(a)?;
    let vec_b = parse_vector_to_f32(b)?;
    
    if vec_a.len() != vec_b.len() {
        return Err(format!(
            "vector dimension mismatch: {} vs {}",
            vec_a.len(),
            vec_b.len()
        ));
    }
    
    let sum: f64 = vec_a.iter()
        .zip(vec_b.iter())
        .map(|(x, y)| (*x as f64 - *y as f64).abs())
        .sum();
    
    Ok(sum)
}

/// Get the number of dimensions in a vector
pub fn vector_dims(v: &str) -> Result<i32, String> {
    let vec = parse_vector_to_f32(v)?;
    Ok(vec.len() as i32)
}

/// Calculate L2 norm (magnitude) of a vector
/// 
/// Formula: sqrt(sum(x_i^2))
pub fn l2_norm(v: &str) -> Result<f64, String> {
    let vec = parse_vector_to_f32(v)?;
    let sum: f64 = vec.iter().map(|x| (*x as f64).powi(2)).sum();
    Ok(sum.sqrt())
}

/// Normalize a vector to unit length (L2 normalization)
/// 
/// Returns a vector with the same direction but magnitude 1
pub fn l2_normalize(v: &str) -> Result<String, String> {
    let vec = parse_vector_to_f32(v)?;
    let norm = l2_norm(v)?;
    
    if norm == 0.0 {
        return Err("cannot normalize zero vector".to_string());
    }
    
    let normalized: Vec<String> = vec.iter()
        .map(|x| {
            let val = *x as f64 / norm;
            // Format with reasonable precision
            if val == val.floor() {
                format!("{}", val as i64)
            } else {
                format!("{}", val)
            }
        })
        .collect();
    
    Ok(format!("[{}]", normalized.join(",")))
}

/// Extract a subvector from start (1-indexed) with given length
/// 
/// PostgreSQL uses 1-based indexing for subvector
pub fn subvector(v: &str, start: i32, length: i32) -> Result<String, String> {
    let vec = parse_vector_to_f32(v)?;
    
    if start < 1 {
        return Err("start index must be >= 1".to_string());
    }
    
    // PostgreSQL uses 1-based indexing
    let start_idx = (start as usize) - 1;
    let end_idx = std::cmp::min(start_idx + (length as usize), vec.len());
    
    if start_idx >= vec.len() {
        return Err("start index out of bounds".to_string());
    }
    
    let subset: Vec<String> = vec[start_idx..end_idx]
        .iter()
        .map(|x| {
            let val = *x as f64;
            if val == val.floor() {
                format!("{}", val as i64)
            } else {
                format!("{}", val)
            }
        })
        .collect();
    
    Ok(format!("[{}]", subset.join(",")))
}

/// Add two vectors element-wise
pub fn vector_add(a: &str, b: &str) -> Result<String, String> {
    let vec_a = parse_vector_to_f32(a)?;
    let vec_b = parse_vector_to_f32(b)?;
    
    if vec_a.len() != vec_b.len() {
        return Err(format!(
            "vector dimension mismatch: {} vs {}",
            vec_a.len(),
            vec_b.len()
        ));
    }
    
    let result: Vec<String> = vec_a.iter()
        .zip(vec_b.iter())
        .map(|(x, y)| {
            let val = *x as f64 + *y as f64;
            if val == val.floor() {
                format!("{}", val as i64)
            } else {
                format!("{}", val)
            }
        })
        .collect();
    
    Ok(format!("[{}]", result.join(",")))
}

/// Subtract two vectors element-wise
pub fn vector_sub(a: &str, b: &str) -> Result<String, String> {
    let vec_a = parse_vector_to_f32(a)?;
    let vec_b = parse_vector_to_f32(b)?;
    
    if vec_a.len() != vec_b.len() {
        return Err(format!(
            "vector dimension mismatch: {} vs {}",
            vec_a.len(),
            vec_b.len()
        ));
    }
    
    let result: Vec<String> = vec_a.iter()
        .zip(vec_b.iter())
        .map(|(x, y)| {
            let val = *x as f64 - *y as f64;
            if val == val.floor() {
                format!("{}", val as i64)
            } else {
                format!("{}", val)
            }
        })
        .collect();
    
    Ok(format!("[{}]", result.join(",")))
}

/// Multiply a vector by a scalar
#[allow(dead_code)]
pub fn vector_mul(v: &str, scalar: f64) -> Result<String, String> {
    let vec = parse_vector_to_f32(v)?;
    
    let result: Vec<String> = vec.iter()
        .map(|x| {
            let val = *x as f64 * scalar;
            if val == val.floor() && val.abs() < 1e15 {
                format!("{}", val as i64)
            } else {
                format!("{}", val)
            }
        })
        .collect();
    
    Ok(format!("[{}]", result.join(",")))
}

/// Calculate the negative inner product for pgvector <#> operator
/// This is used for ordering: smaller values = more similar
#[allow(dead_code)]
pub fn negative_inner_product(a: &str, b: &str) -> Result<f64, String> {
    let product = inner_product(a, b)?;
    Ok(-product)
}

/// Calculate Hamming distance between two vectors
/// 
/// Formula: number of positions at which the corresponding elements are different
pub fn hamming_distance(a: &str, b: &str) -> Result<f64, String> {
    let vec_a = parse_vector_to_f32(a)?;
    let vec_b = parse_vector_to_f32(b)?;
    
    if vec_a.len() != vec_b.len() {
        return Err(format!(
            "vector dimension mismatch: {} vs {}",
            vec_a.len(),
            vec_b.len()
        ));
    }
    
    let distance: f64 = vec_a.iter()
        .zip(vec_b.iter())
        .filter(|(x, y)| x != y)
        .count() as f64;
    
    Ok(distance)
}

/// Calculate Jaccard distance between two vectors
/// 
/// Formula: 1 - (|A ∩ B| / |A ∪ B|)
/// Returns 0 for identical vectors, 1 for completely different vectors
pub fn jaccard_distance(a: &str, b: &str) -> Result<f64, String> {
    let vec_a = parse_vector_to_f32(a)?;
    let vec_b = parse_vector_to_f32(b)?;
    
    if vec_a.len() != vec_b.len() {
        return Err(format!(
            "vector dimension mismatch: {} vs {}",
            vec_a.len(),
            vec_b.len()
        ));
    }
    
    // Count intersection (positions where both have the same non-zero value)
    let intersection: f64 = vec_a.iter()
        .zip(vec_b.iter())
        .filter(|(x, y)| x == y && **x != 0.0)
        .count() as f64;
    
    // Count union (positions where at least one has non-zero value)
    let union: f64 = vec_a.iter()
        .zip(vec_b.iter())
        .filter(|(x, y)| **x != 0.0 || **y != 0.0)
        .count() as f64;
    
    if union == 0.0 {
        return Ok(0.0); // Both vectors are all zeros
    }
    
    let similarity = intersection / union;
    Ok(1.0 - similarity)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_l2_distance_basic() {
        let result = l2_distance("[1, 1]", "[2, 2]").unwrap();
        assert!((result - std::f64::consts::SQRT_2).abs() < 0.0001);
    }

    #[test]
    fn test_l2_distance_zero() {
        let result = l2_distance("[1, 2, 3]", "[1, 2, 3]").unwrap();
        assert!(result.abs() < 0.0001);
    }

    #[test]
    fn test_l2_distance_negative() {
        let result = l2_distance("[1, 2, 3]", "[-1, -2, -3]").unwrap();
        // sqrt(4 + 16 + 36) = sqrt(56) ≈ 7.48
        assert!((result - 7.48331477).abs() < 0.001);
    }

    #[test]
    fn test_cosine_distance_identical() {
        let result = cosine_distance("[1, 2, 3]", "[1, 2, 3]").unwrap();
        assert!(result.abs() < 0.0001);
    }

    #[test]
    fn test_cosine_distance_opposite() {
        let result = cosine_distance("[1, 0]", "[-1, 0]").unwrap();
        assert!((result - 2.0).abs() < 0.0001);
    }

    #[test]
    fn test_cosine_distance_orthogonal() {
        let result = cosine_distance("[1, 0]", "[0, 1]").unwrap();
        assert!((result - 1.0).abs() < 0.0001);
    }

    #[test]
    fn test_cosine_distance_scaled() {
        // Cosine distance should be invariant to scaling
        let r1 = cosine_distance("[1, 2, 3]", "[4, 5, 6]").unwrap();
        let r2 = cosine_distance("[2, 4, 6]", "[8, 10, 12]").unwrap();
        assert!((r1 - r2).abs() < 0.0001);
    }

    #[test]
    fn test_inner_product_basic() {
        let result = inner_product("[1, 2, 3]", "[4, 5, 6]").unwrap();
        assert!((result - 32.0).abs() < 0.0001); // 1*4 + 2*5 + 3*6
    }

    #[test]
    fn test_inner_product_orthogonal() {
        let result = inner_product("[1, 0]", "[0, 1]").unwrap();
        assert!(result.abs() < 0.0001);
    }

    #[test]
    fn test_inner_product_negative() {
        let result = inner_product("[1, 2, 3]", "[-1, -2, -3]").unwrap();
        assert!((result - (-14.0)).abs() < 0.0001);
    }

    #[test]
    fn test_l1_distance_basic() {
        let result = l1_distance("[1, 2, 3]", "[4, 5, 6]").unwrap();
        assert!((result - 9.0).abs() < 0.0001); // |1-4| + |2-5| + |3-6|
    }

    #[test]
    fn test_l1_distance_negative() {
        let result = l1_distance("[-1, -2]", "[1, 2]").unwrap();
        assert!((result - 6.0).abs() < 0.0001);
    }

    #[test]
    fn test_vector_dims_basic() {
        assert_eq!(vector_dims("[1, 2, 3]").unwrap(), 3);
        assert_eq!(vector_dims("[1]").unwrap(), 1);
        assert_eq!(vector_dims("[1, 2, 3, 4, 5]").unwrap(), 5);
    }

    #[test]
    fn test_vector_dims_empty() {
        assert_eq!(vector_dims("[]").unwrap(), 0);
    }

    #[test]
    fn test_l2_norm_basic() {
        let result = l2_norm("[3, 4]").unwrap();
        assert!((result - 5.0).abs() < 0.0001);
    }

    #[test]
    fn test_l2_norm_unit() {
        let result = l2_norm("[1, 0, 0]").unwrap();
        assert!((result - 1.0).abs() < 0.0001);
    }

    #[test]
    fn test_l2_normalize_basic() {
        let result = l2_normalize("[3, 4]").unwrap();
        // Should be [0.6, 0.8]
        let vals: Vec<f64> = result
            .trim_matches(|c| c == '[' || c == ']')
            .split(',')
            .map(|s| s.trim().parse().unwrap())
            .collect();
        assert!((vals[0] - 0.6).abs() < 0.0001);
        assert!((vals[1] - 0.8).abs() < 0.0001);
    }

    #[test]
    fn test_l2_normalize_unit() {
        let result = l2_normalize("[1, 0, 0]").unwrap();
        let vals: Vec<f64> = result
            .trim_matches(|c| c == '[' || c == ']')
            .split(',')
            .map(|s| s.trim().parse().unwrap())
            .collect();
        assert!((vals[0] - 1.0).abs() < 0.0001);
        assert!(vals[1].abs() < 0.0001);
        assert!(vals[2].abs() < 0.0001);
    }

    #[test]
    fn test_subvector_basic() {
        let result = subvector("[1, 2, 3, 4, 5]", 1, 3).unwrap();
        assert!(result.contains("1"));
        assert!(result.contains("2"));
        assert!(result.contains("3"));
        assert!(!result.contains("4"));
        assert!(!result.contains("5"));
    }

    #[test]
    fn test_subvector_from_middle() {
        let result = subvector("[1, 2, 3, 4, 5]", 2, 2).unwrap();
        assert!(result.contains("2"));
        assert!(result.contains("3"));
        assert!(!result.contains("1"));
        assert!(!result.contains("4"));
    }

    #[test]
    fn test_dimension_mismatch_error() {
        let result = l2_distance("[1, 2]", "[1, 2, 3]");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("mismatch"));
    }

    #[test]
    fn test_zero_vector_error() {
        let result = cosine_distance("[0, 0]", "[1, 2]");
        assert!(result.is_err());
        
        let result = l2_normalize("[0, 0]");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_invalid_vector() {
        let result = l2_distance("not a vector", "[1, 2]");
        assert!(result.is_err());
    }

    #[test]
    fn test_vector_add() {
        let result = vector_add("[1, 2, 3]", "[4, 5, 6]").unwrap();
        let vals: Vec<f64> = result
            .trim_matches(|c| c == '[' || c == ']')
            .split(',')
            .map(|s| s.trim().parse().unwrap())
            .collect();
        assert!((vals[0] - 5.0).abs() < 0.0001);
        assert!((vals[1] - 7.0).abs() < 0.0001);
        assert!((vals[2] - 9.0).abs() < 0.0001);
    }

    #[test]
    fn test_vector_sub() {
        let result = vector_sub("[4, 5, 6]", "[1, 2, 3]").unwrap();
        let vals: Vec<f64> = result
            .trim_matches(|c| c == '[' || c == ']')
            .split(',')
            .map(|s| s.trim().parse().unwrap())
            .collect();
        assert!((vals[0] - 3.0).abs() < 0.0001);
        assert!((vals[1] - 3.0).abs() < 0.0001);
        assert!((vals[2] - 3.0).abs() < 0.0001);
    }

    #[test]
    fn test_vector_mul() {
        let result = vector_mul("[1, 2, 3]", 2.0).unwrap();
        let vals: Vec<f64> = result
            .trim_matches(|c| c == '[' || c == ']')
            .split(',')
            .map(|s| s.trim().parse().unwrap())
            .collect();
        assert!((vals[0] - 2.0).abs() < 0.0001);
        assert!((vals[1] - 4.0).abs() < 0.0001);
        assert!((vals[2] - 6.0).abs() < 0.0001);
    }

    #[test]
    fn test_negative_inner_product() {
        let result = negative_inner_product("[1, 2, 3]", "[4, 5, 6]").unwrap();
        assert!((result - (-32.0)).abs() < 0.0001);
    }

    #[test]
    fn test_whitespace_handling() {
        let r1 = l2_distance("[1,2,3]", "[4,5,6]").unwrap();
        let r2 = l2_distance("[ 1, 2, 3 ]", "[ 4, 5, 6 ]").unwrap();
        assert!((r1 - r2).abs() < 0.0001);
    }

    #[test]
    fn test_floating_point_values() {
        let result = l2_distance("[0.1, 0.2]", "[0.3, 0.4]").unwrap();
        assert!(result > 0.0);
    }
}
