use pgwire::api::results::Response;
use std::fs;
use pgqt::handler::SqliteHandler;
use pgqt::handler::query::QueryExecution;

fn temp_db_path(name: &str) -> String {
    let temp_dir = std::env::temp_dir();
    temp_dir.join(name).to_str().unwrap().to_string()
}

fn cleanup_db(path: &str) {
    let _ = fs::remove_file(path);
}

fn assert_tag(response: &Response, expected_tag: &str) {
    match response {
        Response::Execution(tag) | Response::TransactionStart(tag) | Response::TransactionEnd(tag) => {
            let tag_str = format!("{:?}", tag);
            assert!(tag_str.contains(expected_tag), "Expected tag {} in {:?}", expected_tag, tag_str);
        },
        _ => panic!("Expected Execution/TransactionStart/TransactionEnd response, got {:?}", response),
    }
}

#[test]
fn test_transaction_rollback() {
    let db_path = temp_db_path("test_tx_rollback.db");
    cleanup_db(&db_path);
    let handler = SqliteHandler::new(&db_path).unwrap();

    handler.execute_query(0, "CREATE TABLE tx_test (id INT)").unwrap();

    let res = handler.execute_query(0, "BEGIN").unwrap();
    assert_tag(&res[0], "BEGIN");

    let res = handler.execute_query(0, "INSERT INTO tx_test VALUES (1)").unwrap();
    assert_tag(&res[0], "INSERT 0");

    let res = handler.execute_query(0, "ROLLBACK").unwrap();
    assert_tag(&res[0], "ROLLBACK");

    // Verify row does not exist
    let res = handler.execute_query(0, "SELECT * FROM tx_test").unwrap();
    match &res[0] {
        Response::Query(_q) => {
            // Can't easily count stream items here without async, 
            // but we can query raw connection to verify
            let conn = handler.conn.lock().unwrap();
            let count: i64 = conn.query_row("SELECT count(*) FROM tx_test", [], |row| row.get(0)).unwrap();
            assert_eq!(count, 0);
        },
        _ => panic!("Expected Query response"),
    }

    cleanup_db(&db_path);
}

#[test]
fn test_transaction_error_state() {
    let db_path = temp_db_path("test_tx_error.db");
    cleanup_db(&db_path);
    let handler = SqliteHandler::new(&db_path).unwrap();

    handler.execute_query(0, "CREATE TABLE tx_error_test (id INT)").unwrap();

    handler.execute_query(0, "BEGIN").unwrap();

    // Invalid SQL
    let res = handler.execute_query(0, "INSERT INTO non_existent_table VALUES (1)");
    assert!(res.is_err());

    // Valid SQL should now fail because transaction is in error state
    let res = handler.execute_query(0, "INSERT INTO tx_error_test VALUES (2)");
    assert!(res.is_err());
    let err_msg = res.unwrap_err().to_string();
    assert!(err_msg.contains("25P02") || err_msg.contains("current transaction is aborted"));

    // ROLLBACK should succeed
    let res = handler.execute_query(0, "ROLLBACK").unwrap();
    assert_tag(&res[0], "ROLLBACK");

    // Now valid SQL should succeed again
    let res = handler.execute_query(0, "INSERT INTO tx_error_test VALUES (3)").unwrap();
    assert_tag(&res[0], "INSERT 0");

    cleanup_db(&db_path);
}

#[test]
fn test_transaction_savepoint() {
    let db_path = temp_db_path("test_tx_savepoint.db");
    cleanup_db(&db_path);
    let handler = SqliteHandler::new(&db_path).unwrap();

    handler.execute_query(0, "CREATE TABLE tx_sp_test (id INT)").unwrap();

    handler.execute_query(0, "BEGIN").unwrap();
    handler.execute_query(0, "INSERT INTO tx_sp_test VALUES (1)").unwrap();
    
    handler.execute_query(0, "SAVEPOINT my_sp").unwrap();
    handler.execute_query(0, "INSERT INTO tx_sp_test VALUES (2)").unwrap();
    
    handler.execute_query(0, "ROLLBACK TO SAVEPOINT my_sp").unwrap();
    handler.execute_query(0, "COMMIT").unwrap();

    let conn = handler.conn.lock().unwrap();
    let count: i64 = conn.query_row("SELECT count(*) FROM tx_sp_test", [], |row| row.get(0)).unwrap();
    assert_eq!(count, 1);
    
    let val: i64 = conn.query_row("SELECT id FROM tx_sp_test LIMIT 1", [], |row| row.get(0)).unwrap();
    assert_eq!(val, 1);

    cleanup_db(&db_path);
}
