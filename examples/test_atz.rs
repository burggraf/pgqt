use pg_query;

fn main() {
    // Test AT TIME ZONE parsing
    let sql = "SELECT '2024-01-01'::timestamp AT TIME ZONE 'UTC'";
    let result = pg_query::parse(sql).unwrap();
    println!("SQL: {}", sql);
    println!("{:#?}", result);
}
