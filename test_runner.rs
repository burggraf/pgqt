// Simple test runner for insert tests
use pgqt::transpiler::transpile;

fn main() {
    // Test basic multi-row INSERT
    let sql = "INSERT INTO test (id, name) VALUES (1, 'a'), (2, 'b'), (3, 'c')";
    let result = transpile(sql);
    println!("Input:  {}", sql);
    println!("Output: {}", result.sql);
    println!("Errors: {:?}", result.errors);
    println!();

    // Test DEFAULT in multi-row
    let sql2 = "INSERT INTO test (id, name) VALUES (1, DEFAULT), (2, 'a')";
    let result2 = transpile(sql2);
    println!("Input:  {}", sql2);
    println!("Output: {}", result2.sql);
    println!("Errors: {:?}", result2.errors);
    println!();

    // Test expressions in multi-row
    let sql3 = "INSERT INTO test (id, name) VALUES (1+1, UPPER('a')), (2, 'b')";
    let result3 = transpile(sql3);
    println!("Input:  {}", sql3);
    println!("Output: {}", result3.sql);
    println!("Errors: {:?}", result3.errors);
    println!();

    // Test mixed values
    let sql4 = "INSERT INTO test (id, name) VALUES (1, 'a'), (DEFAULT, 'b')";
    let result4 = transpile(sql4);
    println!("Input:  {}", sql4);
    println!("Output: {}", result4.sql);
    println!("Errors: {:?}", result4.errors);
}
