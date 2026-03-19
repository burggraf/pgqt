//! Integration tests for statistical accumulator functions
//!
//! These tests verify the statistical accumulator functions work correctly
//! when used through the SQLite connection.

use rusqlite::Connection;

fn setup_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    
    // Register the statistical accumulator functions
    pgqt::stats_accum::register_stats_accum_functions(&conn).unwrap();
    
    conn
}

#[test]
fn test_float8_accum_basic() {
    let conn = setup_db();

    let result: String = conn
        .query_row("SELECT float8_accum('[]', 5.0)", [], |r| r.get(0))
        .unwrap();

    let accum: Vec<f64> = serde_json::from_str(&result).unwrap();
    assert_eq!(accum.len(), 3);
    assert_eq!(accum[0], 1.0);  // count
    assert_eq!(accum[1], 5.0);  // sum
    assert_eq!(accum[2], 25.0); // sum of squares
}

#[test]
fn test_float8_accum_chained() {
    let conn = setup_db();

    // Start with empty array
    let result: String = conn
        .query_row("SELECT float8_accum('[]', 2.0)", [], |r| r.get(0))
        .unwrap();

    // Add second value
    let result: String = conn
        .query_row("SELECT float8_accum(?1, 3.0)", [&result], |r| r.get(0))
        .unwrap();

    // Add third value
    let result: String = conn
        .query_row("SELECT float8_accum(?1, 4.0)", [&result], |r| r.get(0))
        .unwrap();

    let accum: Vec<f64> = serde_json::from_str(&result).unwrap();
    assert_eq!(accum[0], 3.0);                    // count = 3
    assert_eq!(accum[1], 9.0);                    // sum = 2+3+4 = 9
    assert_eq!(accum[2], 4.0 + 9.0 + 16.0);       // sum of squares = 29
}

#[test]
fn test_float8_accum_with_prepopulated() {
    let conn = setup_db();

    // Start with pre-populated accumulator [n=3, sum=15, sum_sqr=55]
    let result: String = conn
        .query_row("SELECT float8_accum('[3.0, 15.0, 55.0]', 10.0)", [], |r| r.get(0))
        .unwrap();

    let accum: Vec<f64> = serde_json::from_str(&result).unwrap();
    assert_eq!(accum[0], 4.0);    // count = 3+1
    assert_eq!(accum[1], 25.0);   // sum = 15+10
    assert_eq!(accum[2], 155.0);  // sum_sqr = 55+100
}

#[test]
fn test_float8_accum_negative_values() {
    let conn = setup_db();

    let result: String = conn
        .query_row("SELECT float8_accum('[]', -5.0)", [], |r| r.get(0))
        .unwrap();

    let accum: Vec<f64> = serde_json::from_str(&result).unwrap();
    assert_eq!(accum[0], 1.0);
    assert_eq!(accum[1], -5.0);
    assert_eq!(accum[2], 25.0); // (-5)^2 = 25
}

#[test]
fn test_float8_accum_floating_point() {
    let conn = setup_db();

    let result: String = conn
        .query_row("SELECT float8_accum('[]', 3.14159)", [], |r| r.get(0))
        .unwrap();

    let accum: Vec<f64> = serde_json::from_str(&result).unwrap();
    assert!((accum[0] - 1.0).abs() < 1e-10);
    assert!((accum[1] - 3.14159).abs() < 1e-10);
    assert!((accum[2] - 9.8695877281).abs() < 1e-6);
}

#[test]
fn test_float8_combine_basic() {
    let conn = setup_db();

    let result: String = conn
        .query_row(
            "SELECT float8_combine('[2.0, 10.0, 50.0]', '[3.0, 15.0, 75.0]')",
            [],
            |r| r.get(0)
        )
        .unwrap();

    let accum: Vec<f64> = serde_json::from_str(&result).unwrap();
    assert_eq!(accum[0], 5.0);    // 2+3
    assert_eq!(accum[1], 25.0);   // 10+15
    assert_eq!(accum[2], 125.0);  // 50+75
}

#[test]
fn test_float8_combine_empty() {
    let conn = setup_db();

    let result: String = conn
        .query_row("SELECT float8_combine('[]', '[]')", [], |r| r.get(0))
        .unwrap();

    let accum: Vec<f64> = serde_json::from_str(&result).unwrap();
    assert!(accum.is_empty());
}

#[test]
fn test_float8_combine_unequal_lengths() {
    let conn = setup_db();

    let result: String = conn
        .query_row("SELECT float8_combine('[1.0, 2.0]', '[3.0, 4.0, 5.0, 6.0]')", [], |r| r.get(0))
        .unwrap();

    let accum: Vec<f64> = serde_json::from_str(&result).unwrap();
    assert_eq!(accum.len(), 4);
    assert_eq!(accum[0], 4.0); // 1+3
    assert_eq!(accum[1], 6.0); // 2+4
    assert_eq!(accum[2], 5.0); // 0+5
    assert_eq!(accum[3], 6.0); // 0+6
}

#[test]
fn test_float8_combine_multi_step() {
    let conn = setup_db();

    // Simulate parallel aggregation with three workers
    let worker1 = "[1.0, 5.0, 25.0]";
    let worker2 = "[2.0, 10.0, 50.0]";
    let worker3 = "[3.0, 15.0, 75.0]";

    // Combine worker1 and worker2
    let combined: String = conn
        .query_row(
            "SELECT float8_combine(?1, ?2)",
            [worker1, worker2],
            |r| r.get(0)
        )
        .unwrap();

    // Combine with worker3
    let final_result: String = conn
        .query_row(
            "SELECT float8_combine(?1, ?2)",
            [&combined, worker3],
            |r| r.get(0)
        )
        .unwrap();

    let accum: Vec<f64> = serde_json::from_str(&final_result).unwrap();
    assert_eq!(accum[0], 6.0);    // 1+2+3
    assert_eq!(accum[1], 30.0);   // 5+10+15
    assert_eq!(accum[2], 150.0);  // 25+50+75
}

#[test]
fn test_float8_regr_accum_basic() {
    let conn = setup_db();

    let result: String = conn
        .query_row("SELECT float8_regr_accum('[]', 10.0, 2.0)", [], |r| r.get(0))
        .unwrap();

    let accum: Vec<f64> = serde_json::from_str(&result).unwrap();
    assert_eq!(accum.len(), 8);
    assert_eq!(accum[0], 1.0);   // n
    assert_eq!(accum[1], 2.0);   // sum_x
    assert_eq!(accum[2], 4.0);   // sum_x2 = 2^2
    assert_eq!(accum[3], 10.0);  // sum_y
    assert_eq!(accum[4], 100.0); // sum_y2 = 10^2
    assert_eq!(accum[5], 20.0);  // sum_xy = 2*10
    assert_eq!(accum[6], 0.0);   // spare
    assert_eq!(accum[7], 0.0);   // spare
}

#[test]
fn test_float8_regr_accum_chained() {
    let conn = setup_db();

    // First point: (x=1, y=2)
    let result: String = conn
        .query_row("SELECT float8_regr_accum('[]', 2.0, 1.0)", [], |r| r.get(0))
        .unwrap();

    // Second point: (x=2, y=4)
    let result: String = conn
        .query_row("SELECT float8_regr_accum(?1, 4.0, 2.0)", [&result], |r| r.get(0))
        .unwrap();

    // Third point: (x=3, y=6)
    let result: String = conn
        .query_row("SELECT float8_regr_accum(?1, 6.0, 3.0)", [&result], |r| r.get(0))
        .unwrap();

    let accum: Vec<f64> = serde_json::from_str(&result).unwrap();
    assert_eq!(accum[0], 3.0);   // n = 3
    assert_eq!(accum[1], 6.0);   // sum_x = 1+2+3 = 6
    assert_eq!(accum[2], 14.0);  // sum_x2 = 1+4+9 = 14
    assert_eq!(accum[3], 12.0);  // sum_y = 2+4+6 = 12
    assert_eq!(accum[4], 56.0);  // sum_y2 = 4+16+36 = 56
    assert_eq!(accum[5], 28.0);  // sum_xy = 1*2 + 2*4 + 3*6 = 28
}

#[test]
fn test_float8_regr_combine_basic() {
    let conn = setup_db();

    let result: String = conn
        .query_row(
            "SELECT float8_regr_combine('[2.0, 6.0, 20.0, 10.0, 50.0, 30.0, 0.0, 0.0]', '[3.0, 12.0, 50.0, 15.0, 75.0, 60.0, 0.0, 0.0]')",
            [],
            |r| r.get(0)
        )
        .unwrap();

    let accum: Vec<f64> = serde_json::from_str(&result).unwrap();
    assert_eq!(accum[0], 5.0);    // n = 2+3
    assert_eq!(accum[1], 18.0);   // sum_x = 6+12
    assert_eq!(accum[2], 70.0);   // sum_x2 = 20+50
    assert_eq!(accum[3], 25.0);   // sum_y = 10+15
    assert_eq!(accum[4], 125.0);  // sum_y2 = 50+75
    assert_eq!(accum[5], 90.0);   // sum_xy = 30+60
    assert_eq!(accum[6], 0.0);
    assert_eq!(accum[7], 0.0);
}

#[test]
fn test_float8_regr_combine_empty() {
    let conn = setup_db();

    let result: String = conn
        .query_row("SELECT float8_regr_combine('[]', '[]')", [], |r| r.get(0))
        .unwrap();

    let accum: Vec<f64> = serde_json::from_str(&result).unwrap();
    assert!(accum.is_empty());
}

#[test]
fn test_float8_regr_combine_partial() {
    let conn = setup_db();

    // Combine with one empty array
    let result: String = conn
        .query_row(
            "SELECT float8_regr_combine('[3.0, 6.0, 14.0, 12.0, 56.0, 28.0, 0.0, 0.0]', '[]')",
            [],
            |r| r.get(0)
        )
        .unwrap();

    let accum: Vec<f64> = serde_json::from_str(&result).unwrap();
    assert_eq!(accum[0], 3.0);
    assert_eq!(accum[1], 6.0);
    assert_eq!(accum[2], 14.0);
    assert_eq!(accum[3], 12.0);
    assert_eq!(accum[4], 56.0);
    assert_eq!(accum[5], 28.0);
}

#[test]
fn test_parallel_aggregation_simulation() {
    let conn = setup_db();

    // Simulate parallel aggregation of values 1, 2, 3, 4, 5, 6
    // Worker 1 processes 1, 2
    let worker1: String = conn
        .query_row("SELECT float8_accum('[]', 1.0)", [], |r| r.get(0))
        .unwrap();
    let worker1: String = conn
        .query_row("SELECT float8_accum(?1, 2.0)", [&worker1], |r| r.get(0))
        .unwrap();

    // Worker 2 processes 3, 4
    let worker2: String = conn
        .query_row("SELECT float8_accum('[]', 3.0)", [], |r| r.get(0))
        .unwrap();
    let worker2: String = conn
        .query_row("SELECT float8_accum(?1, 4.0)", [&worker2], |r| r.get(0))
        .unwrap();

    // Worker 3 processes 5, 6
    let worker3: String = conn
        .query_row("SELECT float8_accum('[]', 5.0)", [], |r| r.get(0))
        .unwrap();
    let worker3: String = conn
        .query_row("SELECT float8_accum(?1, 6.0)", [&worker3], |r| r.get(0))
        .unwrap();

    // Combine all workers
    let combined: String = conn
        .query_row("SELECT float8_combine(?1, ?2)", [&worker1, &worker2], |r| r.get(0))
        .unwrap();
    let final_result: String = conn
        .query_row("SELECT float8_combine(?1, ?2)", [&combined, &worker3], |r| r.get(0))
        .unwrap();

    let accum: Vec<f64> = serde_json::from_str(&final_result).unwrap();
    assert_eq!(accum[0], 6.0);   // n = 6
    assert_eq!(accum[1], 21.0);  // sum = 1+2+3+4+5+6 = 21
    assert_eq!(accum[2], 91.0);  // sum_sqr = 1+4+9+16+25+36 = 91
}

#[test]
fn test_regression_parallel_simulation() {
    let conn = setup_db();

    // Simulate parallel regression accumulation
    // Points: (1,2), (2,4), (3,6), (4,8)
    
    // Worker 1: (1,2), (2,4)
    let worker1: String = conn
        .query_row("SELECT float8_regr_accum('[]', 2.0, 1.0)", [], |r| r.get(0))
        .unwrap();
    let worker1: String = conn
        .query_row("SELECT float8_regr_accum(?1, 4.0, 2.0)", [&worker1], |r| r.get(0))
        .unwrap();

    // Worker 2: (3,6), (4,8)
    let worker2: String = conn
        .query_row("SELECT float8_regr_accum('[]', 6.0, 3.0)", [], |r| r.get(0))
        .unwrap();
    let worker2: String = conn
        .query_row("SELECT float8_regr_accum(?1, 8.0, 4.0)", [&worker2], |r| r.get(0))
        .unwrap();

    // Combine
    let combined: String = conn
        .query_row("SELECT float8_regr_combine(?1, ?2)", [&worker1, &worker2], |r| r.get(0))
        .unwrap();

    let accum: Vec<f64> = serde_json::from_str(&combined).unwrap();
    assert_eq!(accum[0], 4.0);   // n = 4
    assert_eq!(accum[1], 10.0);  // sum_x = 1+2+3+4 = 10
    assert_eq!(accum[2], 30.0);  // sum_x2 = 1+4+9+16 = 30
    assert_eq!(accum[3], 20.0);  // sum_y = 2+4+6+8 = 20
    assert_eq!(accum[4], 120.0); // sum_y2 = 4+16+36+64 = 120
    assert_eq!(accum[5], 60.0);  // sum_xy = 2+8+18+32 = 60
}

#[test]
fn test_variance_calculation_using_accumulator() {
    let conn = setup_db();

    // Calculate variance for values: 2, 4, 4, 4, 5, 5, 7, 9
    // Using the formula: variance = (sum_sqr/n) - (mean)^2
    
    let values = [2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0];
    let mut accum = "[]".to_string();
    
    for val in &values {
        accum = conn
            .query_row("SELECT float8_accum(?1, ?2)", rusqlite::params![&accum, *val], |r| r.get(0))
            .unwrap();
    }

    let accum_vec: Vec<f64> = serde_json::from_str(&accum).unwrap();
    let n = accum_vec[0];
    let sum = accum_vec[1];
    let sum_sqr = accum_vec[2];
    
    let mean = sum / n;
    let variance = (sum_sqr / n) - (mean * mean);
    
    // Population variance for this data is 4.0
    assert!((variance - 4.0).abs() < 1e-10);
}