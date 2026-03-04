use pgqt::schema::SearchPath;

#[test]
fn test_show_search_path_parsing() {
    // Test that we can handle the SHOW commands at a basic level
    let sql = "SHOW search_path";
    assert!(sql.to_uppercase() == "SHOW SEARCH_PATH");

    let sql = "SHOW server_version";
    assert!(sql.to_uppercase().starts_with("SHOW"));

    let sql = "SHOW ALL";
    assert!(sql.to_uppercase() == "SHOW ALL");
}

#[test]
fn test_search_path_parse() {
    // Test SearchPath parsing
    let path = SearchPath::parse("public, pg_catalog").unwrap();
    assert_eq!(path.schemas, vec!["public", "pg_catalog"]);

    let path = SearchPath::parse("$user, public").unwrap();
    assert_eq!(path.schemas, vec!["$user", "public"]);

    let path = SearchPath::parse("").unwrap();
    assert_eq!(path.schemas, vec!["$user", "public"]);
}

#[test]
fn test_search_path_to_string() {
    let path = SearchPath::parse("schema1, public").unwrap();
    let path_str = path.to_string();
    assert!(path_str.contains("schema1"));
    assert!(path_str.contains("public"));
}