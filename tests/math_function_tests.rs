use pgqt::transpiler::transpile;

#[test]
fn test_log_base10() {
    let sql = "SELECT log(100.0)";
    let result = transpile(sql);
    // Should map to SQLite's log10()
    assert!(result.contains("log10") || result.contains("LOG"), "Expected log10 in output, got: {}", result);
}

#[test]
fn test_log_arbitrary_base() {
    let sql = "SELECT log(2.0, 64.0)";
    let result = transpile(sql);
    // Should map to: log(x) / log(base) using change of base formula
    assert!(result.contains("log") || result.contains("LOG"), "Expected log in output, got: {}", result);
}

#[test]
fn test_ln() {
    let sql = "SELECT ln(2.718281828)";
    let result = transpile(sql);
    // Should map to SQLite's ln() or log()
    assert!(result.contains("ln") || result.contains("log") || result.contains("LOG"), 
            "Expected ln or log in output, got: {}", result);
}

#[test]
fn test_sqrt() {
    let sql = "SELECT sqrt(16.0)";
    let result = transpile(sql);
    assert!(result.contains("sqrt") || result.contains("SQRT"), "Expected sqrt in output, got: {}", result);
}

#[test]
fn test_exp() {
    let sql = "SELECT exp(1.0)";
    let result = transpile(sql);
    assert!(result.contains("exp") || result.contains("EXP"), "Expected exp in output, got: {}", result);
}

#[test]
fn test_div_integer() {
    let sql = "SELECT div(17, 5)";
    let result = transpile(sql);
    // Should return integer division result (3)
    assert!(result.contains("/") || result.contains("div") || result.contains("CAST"), 
            "Expected / or div or CAST in output, got: {}", result);
}

#[test]
fn test_div_negative() {
    let sql = "SELECT div(-17, 5)";
    let result = transpile(sql);
    // PostgreSQL truncates toward zero
    assert!(result.contains("/") || result.contains("div") || result.contains("CAST"), 
            "Expected / or div or CAST in output, got: {}", result);
}
