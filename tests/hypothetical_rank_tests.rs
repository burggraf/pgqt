use pgqt::transpiler::transpile_with_metadata;

#[test]
fn test_rank_hypothetical() {
    let sql = "SELECT rank(3) WITHIN GROUP (ORDER BY x) FROM t";
    let result = transpile_with_metadata(sql);
    assert_eq!(result.sql, "select __pg_hypothetical_rank(3, x) AS \"rank\" from t");
}

#[test]
fn test_dense_rank_hypothetical() {
    let sql = "SELECT dense_rank(4) WITHIN GROUP (ORDER BY x) FROM t";
    let result = transpile_with_metadata(sql);
    assert_eq!(result.sql, "select __pg_hypothetical_dense_rank(4, x) AS \"dense_rank\" from t");
}

#[test]
fn test_percent_rank_hypothetical() {
    let sql = "SELECT percent_rank(0.5) WITHIN GROUP (ORDER BY x) FROM t";
    let result = transpile_with_metadata(sql);
    assert_eq!(result.sql, "select __pg_hypothetical_percent_rank(0.5, x) AS \"percent_rank\" from t");
}

#[test]
fn test_cume_dist_hypothetical() {
    let sql = "SELECT cume_dist(3) WITHIN GROUP (ORDER BY x) FROM t";
    let result = transpile_with_metadata(sql);
    assert_eq!(result.sql, "select __pg_hypothetical_cume_dist(3, x) AS \"cume_dist\" from t");
}

#[test]
fn test_print_ast_simple() {
    let sql = "SELECT rank(3)";
    let result = pg_query::parse(sql).unwrap();
    println!("{:#?}", result);
}
