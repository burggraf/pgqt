// Test script for ON CONFLICT functionality
use pgqt::transpiler::{transpile, transpile_with_metadata};

fn main() {
    println!("=== Testing ON CONFLICT functionality ===\n");
    
    // Test 1: Simple ON CONFLICT DO NOTHING
    let sql1 = "INSERT INTO users (id, name) VALUES (1, 'John') ON CONFLICT (id) DO NOTHING";
    let result1 = transpile_with_metadata(sql1);
    println!("Test 1 - Simple ON CONFLICT DO NOTHING:");
    println!("  Input:  {}", sql1);
    println!("  Output: {}", result1.sql);
    println!();
    
    // Test 2: ON CONFLICT with DO UPDATE
    let sql2 = "INSERT INTO users (id, name) VALUES (1, 'John') ON CONFLICT (id) DO UPDATE SET name = EXCLUDED.name";
    let result2 = transpile_with_metadata(sql2);
    println!("Test 2 - ON CONFLICT DO UPDATE:");
    println!("  Input:  {}", sql2);
    println!("  Output: {}", result2.sql);
    println!();
    
    // Test 3: Multiple conflict targets
    let sql3 = "INSERT INTO users (id, email, name) VALUES (1, 'john@example.com', 'John') ON CONFLICT (id, email) DO NOTHING";
    let result3 = transpile_with_metadata(sql3);
    println!("Test 3 - Multiple conflict targets:");
    println!("  Input:  {}", sql3);
    println!("  Output: {}", result3.sql);
    println!();
    
    // Test 4: DO UPDATE with WHERE clause
    let sql4 = "INSERT INTO users (id, name, updated) VALUES (1, 'John', true) ON CONFLICT (id) DO UPDATE SET name = EXCLUDED.name WHERE users.name IS NULL";
    let result4 = transpile_with_metadata(sql4);
    println!("Test 4 - DO UPDATE with WHERE clause:");
    println!("  Input:  {}", sql4);
    println!("  Output: {}", result4.sql);
    println!();
    
    // Test 5: Subquery in DO UPDATE SET
    let sql5 = "INSERT INTO users (id, name) VALUES (1, 'John') ON CONFLICT (id) DO UPDATE SET name = (SELECT max(name) FROM other_table)";
    let result5 = transpile_with_metadata(sql5);
    println!("Test 5 - Subquery in DO UPDATE SET:");
    println!("  Input:  {}", sql5);
    println!("  Output: {}", result5.sql);
    println!();
    
    // Test 6: ON CONFLICT with RETURNING
    let sql6 = "INSERT INTO users (id, name) VALUES (1, 'John') ON CONFLICT (id) DO UPDATE SET name = EXCLUDED.name RETURNING *";
    let result6 = transpile_with_metadata(sql6);
    println!("Test 6 - ON CONFLICT with RETURNING:");
    println!("  Input:  {}", sql6);
    println!("  Output: {}", result6.sql);
    println!();
    
    // Test 7: Complex case - multiple targets, WHERE, and RETURNING
    let sql7 = "INSERT INTO users (id, email, name) VALUES (1, 'john@example.com', 'John') ON CONFLICT (id, email) DO UPDATE SET name = EXCLUDED.name WHERE users.name < EXCLUDED.name RETURNING id, name";
    let result7 = transpile_with_metadata(sql7);
    println!("Test 7 - Complex: multiple targets + WHERE + RETURNING:");
    println!("  Input:  {}", sql7);
    println!("  Output: {}", result7.sql);
    println!();
}
