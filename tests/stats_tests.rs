use pgqt::transpiler::transpile;

#[test]
fn test_regr_functions_transpilation() {
    // Verify the functions are recognized and not modified
    let sql = "SELECT regr_slope(y, x) FROM data";
    let result = transpile(sql);
    assert!(result.contains("regr_slope"));
}

#[test]
fn test_corr_transpilation() {
    let sql = "SELECT corr(y, x) FROM data";
    let result = transpile(sql);
    assert!(result.contains("corr"));
}

#[test]
fn test_covar_transpilation() {
    let sql = "SELECT covar_pop(y, x), covar_samp(y, x) FROM data";
    let result = transpile(sql);
    assert!(result.contains("covar_pop"));
    assert!(result.contains("covar_samp"));
}

#[test]
fn test_all_regr_transpilation() {
    let funcs = vec![
        "regr_count", "regr_sxx", "regr_syy", "regr_sxy", 
        "regr_avgx", "regr_avgy", "regr_r2", "regr_slope", "regr_intercept"
    ];
    for func in funcs {
        let sql = format!("SELECT {}(y, x) FROM data", func);
        let result = transpile(&sql);
        assert!(result.contains(func));
    }
}
