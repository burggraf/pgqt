use pgqt::transpiler::transpile;

#[test]
fn test_param_ref_transpilation() {
    let sql = "SELECT * FROM pg_class WHERE oid = $1";
    let transpiled = transpile(sql);
    assert!(transpiled.contains("?1"));
}

#[test]
fn test_param_ref_with_cast() {
    let sql = "SELECT * FROM pg_class WHERE oid = $1::regclass";
    let transpiled = transpile(sql);
    assert!(transpiled.contains("?1"));
    // Use lowercase for comparison as transpile() might lowercase everything
    let expected = "select oid from pg_class where relname = ?1 or oid = cast(?1 as integer) limit 1";
    assert!(transpiled.to_lowercase().contains(&expected.to_lowercase()));
}
