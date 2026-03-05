//! SQL value function reconstruction
//!
//! Handles PostgreSQL SQL value functions like CURRENT_TIMESTAMP, CURRENT_DATE, etc.

use pg_query::protobuf::SqlValueFunction;

/// Reconstruct a SQL value function (CURRENT_TIMESTAMP, CURRENT_DATE, etc.)
pub(crate) fn reconstruct_sql_value_function(sql_val: &SqlValueFunction) -> String {
    use pg_query::protobuf::SqlValueFunctionOp;

    match sql_val.op() {
        SqlValueFunctionOp::SvfopCurrentTimestamp | SqlValueFunctionOp::SvfopCurrentTimestampN => {
            "CURRENT_TIMESTAMP".to_string()
        }
        SqlValueFunctionOp::SvfopCurrentDate => {
            "date('now')".to_string()
        }
        SqlValueFunctionOp::SvfopCurrentTime | SqlValueFunctionOp::SvfopCurrentTimeN => {
            "time('now')".to_string()
        }
        SqlValueFunctionOp::SvfopLocaltime | SqlValueFunctionOp::SvfopLocaltimeN => {
            "time('now', 'localtime')".to_string()
        }
        SqlValueFunctionOp::SvfopLocaltimestamp | SqlValueFunctionOp::SvfopLocaltimestampN => {
            "datetime('now', 'localtime')".to_string()
        }
        SqlValueFunctionOp::SvfopCurrentUser | SqlValueFunctionOp::SvfopUser => {
            "'current_user'".to_string()
        }
        SqlValueFunctionOp::SvfopCurrentRole => {
            "'current_role'".to_string()
        }
        SqlValueFunctionOp::SvfopSessionUser => {
            "'session_user'".to_string()
        }
        SqlValueFunctionOp::SvfopCurrentCatalog => {
            "'current_catalog'".to_string()
        }
        SqlValueFunctionOp::SvfopCurrentSchema => {
            "'main'".to_string()
        }
        _ => {
            "NULL".to_string()
        }
    }
}