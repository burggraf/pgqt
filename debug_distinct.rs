use postgresqlite::transpiler::debug_ast;

fn main() {
    println!("--- SELECT DISTINCT ---");
    debug_ast("SELECT DISTINCT id FROM users");
    
    println!("\n--- SELECT DISTINCT ON ---");
    debug_ast("SELECT DISTINCT ON (id) id, name FROM users ORDER BY id, name");
}
