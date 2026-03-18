use pg_parse;
use serde_json;

fn main() {
    let sql = r#"
        CREATE FUNCTION add(a int, b int) RETURNS int AS $$
        BEGIN
            RETURN a + b;
        END;
        $$ LANGUAGE plpgsql;
    "#;
    
    let json = pg_parse::parse_plpgsql(sql).unwrap();
    println!("{}", serde_json::to_string_pretty(&json).unwrap());
}
