fn main() {
    use pgqt::transpiler::transpile;
    
    let tests = vec![
        "SELECT sum(salary) OVER (PARTITION BY department) FROM employees",
        "SELECT sum(amount) OVER (ORDER BY order_date ROWS UNBOUNDED PRECEDING) FROM orders",
        "SELECT lag(amount) OVER (ORDER BY order_date) FROM orders",
        "SELECT lead(amount) OVER (ORDER BY order_date) FROM orders",
        "SELECT percent_rank() OVER (ORDER BY salary) FROM employees",
        "SELECT cume_dist() OVER (ORDER BY salary) FROM employees",
    ];
    
    for sql in tests {
        let result = transpile(sql);
        println!("Input:  {}", sql);
        println!("Output: {}", result);
        println!();
    }
}
