use postgresqlite::transpiler::transpile_with_metadata;

#[test]
fn test_copy_from_transpilation() {
    let sql = "COPY users FROM STDIN";
    let res = transpile_with_metadata(sql);
    assert!(res.sql.contains("-- COPY From \"users\""));
}

#[test]
fn test_copy_from_columns_transpilation() {
    let sql = "COPY users (id, name) FROM STDIN";
    let res = transpile_with_metadata(sql);
    assert!(res.sql.contains("-- COPY From \"users\""));
}

#[test]
fn test_copy_to_transpilation() {
    let sql = "COPY users TO STDOUT";
    let res = transpile_with_metadata(sql);
    assert!(res.sql.contains("-- COPY To \"users\""));
}

#[test]
fn test_copy_to_query_transpilation() {
    let sql = "COPY (SELECT * FROM users WHERE id > 10) TO STDOUT";
    let res = transpile_with_metadata(sql);
    assert!(res.sql.contains("-- COPY To \"QUERY\""));
}

#[test]
fn test_copy_with_options_transpilation() {
    let sql = "COPY users FROM STDIN WITH (FORMAT CSV, HEADER, DELIMITER ',')";
    let res = transpile_with_metadata(sql);
    assert!(res.sql.contains("-- COPY From \"users\""));
}
