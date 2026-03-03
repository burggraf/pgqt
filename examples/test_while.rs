use pg_parse;
use serde_json;

fn main() {
    let sql = r#"
        CREATE FUNCTION counter() RETURNS int AS $$
        DECLARE
            i int := 0;
            total int := 0;
        BEGIN
            WHILE i < 10 LOOP
                total := total + i;
                i := i + 1;
            END LOOP;
            RETURN total;
        END;
        $$ LANGUAGE plpgsql;
    "#;
    
    let json = pg_parse::parse_plpgsql(sql).unwrap();
    println!("{}", serde_json::to_string_pretty(&json).unwrap());
}
