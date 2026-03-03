use pg_parse;
use serde_json;

fn main() {
    let sql = r#"
        CREATE FUNCTION log_message(msg text) RETURNS void AS $$
        BEGIN
            RAISE NOTICE 'Message: %', msg;
        END;
        $$ LANGUAGE plpgsql;
    "#;
    
    let json = pg_parse::parse_plpgsql(sql).unwrap();
    println!("{}", serde_json::to_string_pretty(&json).unwrap());
}
