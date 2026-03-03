use pg_parse;
use serde_json;

fn main() {
    let sql = r#"
        CREATE FUNCTION counter() RETURNS int AS $$
        DECLARE
            i int := 0;
            total int := 0;
        BEGIN
            FOR i IN 1..10 LOOP
                total := total + i;
            END LOOP;
            RETURN total;
        END;
        $$ LANGUAGE plpgsql;
    "#;
    
    let json = pg_parse::parse_plpgsql(sql).unwrap();
    println!("{}", serde_json::to_string_pretty(&json).unwrap());
}
