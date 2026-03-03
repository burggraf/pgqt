//! Array parsing utilities and helpers

use super::types::ArrayValue;

/// Parse a JSON array string
pub fn parse_json_array(input: &str) -> Result<ArrayValue, String> {
    let trimmed = input.trim();

    if !trimmed.starts_with('[') || !trimmed.ends_with(']') {
        return Err("Invalid JSON array format".to_string());
    }

    let inner = &trimmed[1..trimmed.len() - 1].trim();

    if inner.is_empty() {
        return Ok(ArrayValue::Empty);
    }

    let elements = parse_json_elements(inner)?;

    // Check if all elements are arrays (multi-dimensional)
    let all_arrays = elements.iter().all(|e| {
        let e = e.trim();
        (e.starts_with('[') && e.ends_with(']')) || (e.starts_with('{') && e.ends_with('}'))
    });

    if all_arrays && elements.len() > 0 {
        let inner_arrays: Result<Vec<ArrayValue>, String> = elements
            .iter()
            .map(|e| parse_array(e))
            .collect();
        return Ok(ArrayValue::MultiD(inner_arrays?));
    }

    let values: Vec<Option<String>> = elements
        .iter()
        .map(|e| {
            let e = e.trim();
            if e == "null" || e.is_empty() {
                None
            } else if e.starts_with('"') && e.ends_with('"') {
                // Unescape JSON string
                let s = &e[1..e.len() - 1];
                Some(unescape_json_string(s))
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

    let content = if trimmed.starts_with('[') {
        if let Some(eq_pos) = trimmed.find("]={") {
            &trimmed[eq_pos + 2..]
        } else {
            trimmed
        }
    } else {
        trimmed
    };

    if !content.starts_with('{') || !content.ends_with('}') {
        return Err("Invalid PostgreSQL array format".to_string());
    }

    let inner = &content[1..content.len() - 1].trim();

    if inner.is_empty() {
        return Ok(ArrayValue::Empty);
    }

    // Check for multi-dimensional array
    if inner.starts_with('{') {
        return parse_pg_multi_dim_array(content);
    }

    let elements = parse_pg_elements(inner)?;
    Ok(ArrayValue::OneD(elements))
}

/// Parse a PostgreSQL multi-dimensional array
fn parse_pg_multi_dim_array(input: &str) -> Result<ArrayValue, String> {
    let trimmed = input.trim();
    let inner = &trimmed[1..trimmed.len() - 1];

    let mut arrays = Vec::new();
    let mut current = String::new();
    let mut depth = 0;
    let mut in_string = false;

    for c in inner.chars() {
        match c {
            '"' if !in_string || depth == 0 => {
                in_string = !in_string;
                current.push(c);
            }
            '{' if !in_string => {
                depth += 1;
                current.push(c);
            }
            '}' if !in_string => {
                depth -= 1;
                current.push(c);
                if depth == 0 {
                    arrays.push(current.trim().to_string());
                    current.clear();
                }
            }
            ',' if !in_string && depth == 0 => {
                // Skip comma between arrays
            }
            _ => {
                current.push(c);
            }
        }
    }

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
            '\\' => {
                current.push(c);
                escape_next = true;
            }
            '"' => {
                in_quotes = !in_quotes;
                // Don't include quotes in the output
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

/// Parse an array from either JSON or PostgreSQL format
pub fn parse_array(input: &str) -> Result<ArrayValue, String> {
    let trimmed = input.trim();

    if trimmed.starts_with('[') {
        parse_json_array(trimmed)
    } else if trimmed.starts_with('{') || (trimmed.starts_with('[') && trimmed.contains("]={")) {
        parse_pg_array(trimmed)
    } else if trimmed.is_empty() {
        Ok(ArrayValue::Empty)
    } else {
        Err(format!("Unknown array format: {}", input))
    }
}
