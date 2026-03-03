use pgqt::plpgsql::parse_plpgsql_function;

fn main() {
    let sql = r#"
        CREATE FUNCTION add(a int, b int) RETURNS int AS $$
        BEGIN
            RETURN a + b;
        END;
        $$ LANGUAGE plpgsql;
    "#;
    
    match parse_plpgsql_function(sql) {
        Ok(func) => {
            println!("Function name: {:?}", func.fn_name);
            println!("Body statements: {}", func.fn_body().len());
        }
        Err(e) => {
            println!("Error: {}", e);
        }
    }
}
