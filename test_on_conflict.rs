// Standalone test for ON CONFLICT functionality
// Run with: rustc --edition 2021 -L target/debug/deps test_on_conflict.rs -o test_on_conflict && ./test_on_conflict

use std::process::Command;

fn main() {
    println!("Testing ON CONFLICT functionality via cargo test");
    
    // Run cargo test with specific tests
    let output = Command::new("cargo")
        .args(&["test", "test_insert_on_conflict", "--", "--nocapture"])
        .current_dir("/Users/markb/dev/pgqt")
        .output()
        .expect("Failed to execute cargo test");
    
    println!("stdout: {}", String::from_utf8_lossy(&output.stdout));
    println!("stderr: {}", String::from_utf8_lossy(&output.stderr));
}
