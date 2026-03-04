//! Core array type definitions

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
