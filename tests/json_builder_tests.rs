use pgqt::transpiler::transpile;

#[test]
fn test_json_build_object_basic() {
    let sql = "SELECT json_build_object('name', 'John', 'age', 30)";
    let result = transpile(sql);
    // Should map to SQLite's json_object()
    assert!(result.contains("json_object"), "Expected json_object in: {}", result);
}

#[test]
fn test_json_build_object_nested() {
    let sql = "SELECT json_build_object('user', json_build_object('name', 'Jane'))";
    let result = transpile(sql);
    assert!(result.contains("json_object"), "Expected json_object in: {}", result);
}

#[test]
fn test_jsonb_build_object() {
    let sql = "SELECT jsonb_build_object('key', 'value')";
    let result = transpile(sql);
    // jsonb_build_object maps to same json_object in SQLite
    assert!(result.contains("json_object"), "Expected json_object in: {}", result);
}

#[test]
fn test_json_build_object_empty() {
    let sql = "SELECT json_build_object()";
    let result = transpile(sql);
    assert!(result.contains("json_object()"), "Expected json_object() in: {}", result);
}

#[test]
fn test_json_build_array_basic() {
    let sql = "SELECT json_build_array(1, 2, 3)";
    let result = transpile(sql);
    // Should map to SQLite's json_array()
    assert!(result.contains("json_array"), "Expected json_array in: {}", result);
}

#[test]
fn test_json_build_array_empty() {
    let sql = "SELECT json_build_array()";
    let result = transpile(sql);
    assert!(result.contains("json_array()"), "Expected json_array() in: {}", result);
}

#[test]
fn test_json_build_array_mixed_types() {
    let sql = "SELECT json_build_array(1, 'text', true, NULL)";
    let result = transpile(sql);
    assert!(result.contains("json_array"), "Expected json_array in: {}", result);
}

#[test]
fn test_jsonb_build_array() {
    let sql = "SELECT jsonb_build_array(1, 2, 3)";
    let result = transpile(sql);
    // jsonb_build_array maps to same json_array in SQLite
    assert!(result.contains("json_array"), "Expected json_array in: {}", result);
}

#[test]
fn test_json_build_object_with_null() {
    let sql = "SELECT json_build_object('a', NULL, 'b', 2)";
    let result = transpile(sql);
    // PostgreSQL includes null values by default
    assert!(result.contains("json_object"), "Expected json_object in: {}", result);
}

#[test]
fn test_json_build_array_with_null() {
    let sql = "SELECT json_build_array(1, NULL, 3)";
    let result = transpile(sql);
    assert!(result.contains("json_array"), "Expected json_array in: {}", result);
}
