//! PL/pgSQL AST type definitions
//!
//! These types represent the JSON AST output from pg_parse::parse_plpgsql()
//! and are used for transpilation to Lua.

// AST type definitions
#![allow(dead_code)]

use serde::{Deserialize, Deserializer};
use serde_json::Value;

/// Top-level wrapper for the action field
#[derive(Debug, Clone, Deserialize)]
pub struct PlPgSQLAction {
    #[serde(rename = "PLpgSQL_stmt_block")]
    pub block: PlPgSQLStmtBlock,
}

/// Top-level PL/pgSQL function structure
#[derive(Debug, Clone, Deserialize)]
pub struct PlpgsqlFunction {
    #[serde(rename = "fn_name")]
    pub fn_name: Option<String>,
    #[serde(rename = "fn_argnames")]
    pub fn_argnames: Option<Vec<String>>,
    #[allow(dead_code)]
    #[serde(rename = "fn_argtypes")]
    pub fn_argtypes: Option<Vec<i64>>,
    #[allow(dead_code)]
    #[serde(rename = "fn_rettype")]
    pub fn_rettype: Option<i64>,
    /// The main action/body of the function
    #[serde(rename = "action")]
    pub action: PlPgSQLAction,
}

impl PlpgsqlFunction {
    /// Get the function body statements (convenience method)
    #[allow(dead_code)]
    pub fn fn_body(&self) -> &Vec<PlPgSQLStmt> {
        &self.action.block.body
    }
}

/// PL/pgSQL statement types
/// Uses a custom deserializer to handle the wrapper object structure
#[derive(Debug, Clone)]
pub enum PlPgSQLStmt {
    Block(PlPgSQLStmtBlock),
    Assign(PlPgSQLStmtAssign),
    If(PlPgSQLStmtIf),
    Loop(PlPgSQLStmtLoop),
    While(PlPgSQLStmtWhile),
    ForI(PlPgSQLStmtForI),
    ForS(PlPgSQLStmtForS),
    Exit(PlPgSQLStmtExit),
    Return(PlPgSQLStmtReturn),
    ReturnNext(PlPgSQLStmtReturnNext),
    Raise(PlPgSQLStmtRaise),
    ExecSql(PlPgSQLStmtExecSql),
    DynExecute(PlPgSQLStmtDynExecute),
    GetDiag(PlPgSQLStmtGetDiag),
    Perform(PlPgSQLStmtPerform),
    Case(PlPgSQLStmtCase),
    Open(PlPgSQLStmtOpen),
    Fetch(PlPgSQLStmtFetch),
    Close(PlPgSQLStmtClose),
    Move(PlPgSQLStmtMove),
}

impl<'de> Deserialize<'de> for PlPgSQLStmt {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        
        if let Some(obj) = value.as_object() {
            if let Some((key, inner)) = obj.iter().next() {
                return match key.as_str() {
                    "PLpgSQL_stmt_block" => {
                        let block: PlPgSQLStmtBlock = serde_json::from_value(inner.clone())
                            .map_err(|e| serde::de::Error::custom(e.to_string()))?;
                        Ok(PlPgSQLStmt::Block(block))
                    }
                    "PLpgSQL_stmt_assign" => {
                        let assign: PlPgSQLStmtAssign = serde_json::from_value(inner.clone())
                            .map_err(|e| serde::de::Error::custom(e.to_string()))?;
                        Ok(PlPgSQLStmt::Assign(assign))
                    }
                    "PLpgSQL_stmt_if" => {
                        let if_stmt: PlPgSQLStmtIf = serde_json::from_value(inner.clone())
                            .map_err(|e| serde::de::Error::custom(e.to_string()))?;
                        Ok(PlPgSQLStmt::If(if_stmt))
                    }
                    "PLpgSQL_stmt_loop" => {
                        let loop_stmt: PlPgSQLStmtLoop = serde_json::from_value(inner.clone())
                            .map_err(|e| serde::de::Error::custom(e.to_string()))?;
                        Ok(PlPgSQLStmt::Loop(loop_stmt))
                    }
                    "PLpgSQL_stmt_while" => {
                        let while_stmt: PlPgSQLStmtWhile = serde_json::from_value(inner.clone())
                            .map_err(|e| serde::de::Error::custom(e.to_string()))?;
                        Ok(PlPgSQLStmt::While(while_stmt))
                    }
                    "PLpgSQL_stmt_fori" => {
                        let for_i: PlPgSQLStmtForI = serde_json::from_value(inner.clone())
                            .map_err(|e| serde::de::Error::custom(e.to_string()))?;
                        Ok(PlPgSQLStmt::ForI(for_i))
                    }
                    "PLpgSQL_stmt_fors" => {
                        let for_s: PlPgSQLStmtForS = serde_json::from_value(inner.clone())
                            .map_err(|e| serde::de::Error::custom(e.to_string()))?;
                        Ok(PlPgSQLStmt::ForS(for_s))
                    }
                    "PLpgSQL_stmt_exit" => {
                        let exit: PlPgSQLStmtExit = serde_json::from_value(inner.clone())
                            .map_err(|e| serde::de::Error::custom(e.to_string()))?;
                        Ok(PlPgSQLStmt::Exit(exit))
                    }
                    "PLpgSQL_stmt_return" => {
                        let ret: PlPgSQLStmtReturn = serde_json::from_value(inner.clone())
                            .map_err(|e| serde::de::Error::custom(e.to_string()))?;
                        Ok(PlPgSQLStmt::Return(ret))
                    }
                    "PLpgSQL_stmt_return_next" => {
                        let ret_next: PlPgSQLStmtReturnNext = serde_json::from_value(inner.clone())
                            .map_err(|e| serde::de::Error::custom(e.to_string()))?;
                        Ok(PlPgSQLStmt::ReturnNext(ret_next))
                    }
                    "PLpgSQL_stmt_raise" => {
                        let raise: PlPgSQLStmtRaise = serde_json::from_value(inner.clone())
                            .map_err(|e| serde::de::Error::custom(e.to_string()))?;
                        Ok(PlPgSQLStmt::Raise(raise))
                    }
                    "PLpgSQL_stmt_execsql" => {
                        let exec: PlPgSQLStmtExecSql = serde_json::from_value(inner.clone())
                            .map_err(|e| serde::de::Error::custom(e.to_string()))?;
                        Ok(PlPgSQLStmt::ExecSql(exec))
                    }
                    "PLpgSQL_stmt_dynexecute" => {
                        let dyn_exec: PlPgSQLStmtDynExecute = serde_json::from_value(inner.clone())
                            .map_err(|e| serde::de::Error::custom(e.to_string()))?;
                        Ok(PlPgSQLStmt::DynExecute(dyn_exec))
                    }
                    "PLpgSQL_stmt_getdiag" => {
                        let diag: PlPgSQLStmtGetDiag = serde_json::from_value(inner.clone())
                            .map_err(|e| serde::de::Error::custom(e.to_string()))?;
                        Ok(PlPgSQLStmt::GetDiag(diag))
                    }
                    "PLpgSQL_stmt_perform" => {
                        let perform: PlPgSQLStmtPerform = serde_json::from_value(inner.clone())
                            .map_err(|e| serde::de::Error::custom(e.to_string()))?;
                        Ok(PlPgSQLStmt::Perform(perform))
                    }
                    "PLpgSQL_stmt_case" => {
                        let case: PlPgSQLStmtCase = serde_json::from_value(inner.clone())
                            .map_err(|e| serde::de::Error::custom(e.to_string()))?;
                        Ok(PlPgSQLStmt::Case(case))
                    }
                    "PLpgSQL_stmt_open" => {
                        let open: PlPgSQLStmtOpen = serde_json::from_value(inner.clone())
                            .map_err(|e| serde::de::Error::custom(e.to_string()))?;
                        Ok(PlPgSQLStmt::Open(open))
                    }
                    "PLpgSQL_stmt_fetch" => {
                        let fetch: PlPgSQLStmtFetch = serde_json::from_value(inner.clone())
                            .map_err(|e| serde::de::Error::custom(e.to_string()))?;
                        Ok(PlPgSQLStmt::Fetch(fetch))
                    }
                    "PLpgSQL_stmt_close" => {
                        let close: PlPgSQLStmtClose = serde_json::from_value(inner.clone())
                            .map_err(|e| serde::de::Error::custom(e.to_string()))?;
                        Ok(PlPgSQLStmt::Close(close))
                    }
                    "PLpgSQL_stmt_move" => {
                        let move_stmt: PlPgSQLStmtMove = serde_json::from_value(inner.clone())
                            .map_err(|e| serde::de::Error::custom(e.to_string()))?;
                        Ok(PlPgSQLStmt::Move(move_stmt))
                    }
                    _ => Err(serde::de::Error::custom(format!("Unknown statement type: {}", key)))
                };
            }
        }
        
        Err(serde::de::Error::custom("Expected object with statement type key"))
    }
}

/// BEGIN/END block
#[derive(Debug, Clone, Deserialize)]
pub struct PlPgSQLStmtBlock {
    pub body: Vec<PlPgSQLStmt>,
    #[serde(default)]
    pub exceptions: Option<Vec<PlPgSQLException>>,
}

/// Variable assignment
#[derive(Debug, Clone)]
pub struct PlPgSQLStmtAssign {
    pub varname: String,
    pub expr: PlPgSQLExpr,
}

impl<'de> Deserialize<'de> for PlPgSQLStmtAssign {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        
        if let Some(obj) = value.as_object() {
            // varname can be a string or we might need to look up varno
            let varname = match obj.get("varname") {
                Some(Value::String(s)) => s.clone(),
                _ => {
                    // If varname is not present, use varno as placeholder
                    // In a real implementation, we'd look up the variable name from datums
                    obj.get("varno")
                        .and_then(|v| v.as_i64())
                        .map(|n| format!("var_{}", n))
                        .ok_or_else(|| serde::de::Error::missing_field("varname or varno"))?
                }
            };
            
            let expr = obj.get("expr")
                .ok_or_else(|| serde::de::Error::missing_field("expr"))
                .and_then(|v| serde_json::from_value(v.clone()).map_err(|e| serde::de::Error::custom(e.to_string())))?;
            
            return Ok(PlPgSQLStmtAssign {
                varname,
                expr,
            });
        }
        
        Err(serde::de::Error::custom("Expected object for assignment"))
    }
}

/// IF/THEN/ELSE statement
#[derive(Debug, Clone, Deserialize)]
pub struct PlPgSQLStmtIf {
    #[serde(rename = "cond")]
    pub cond: PlPgSQLExpr,
    #[serde(rename = "then_body")]
    pub then_body: Vec<PlPgSQLStmt>,
    #[serde(default)]
    pub elsif_list: Option<Vec<PlPgSQLStmtIfElsif>>,
    #[serde(default)]
    pub else_body: Option<Vec<PlPgSQLStmt>>,
}

/// ELSIF branch
#[derive(Debug, Clone, Deserialize)]
pub struct PlPgSQLStmtIfElsif {
    #[serde(rename = "cond")]
    pub cond: PlPgSQLExpr,
    #[serde(rename = "stmts")]
    pub stmts: Vec<PlPgSQLStmt>,
}

/// LOOP statement
#[derive(Debug, Clone, Deserialize)]
pub struct PlPgSQLStmtLoop {
    #[serde(rename = "body")]
    pub body: Vec<PlPgSQLStmt>,
}

/// WHILE loop
#[derive(Debug, Clone, Deserialize)]
pub struct PlPgSQLStmtWhile {
    #[serde(rename = "cond")]
    pub cond: PlPgSQLExpr,
    #[serde(rename = "body")]
    pub body: Vec<PlPgSQLStmt>,
}

/// FOR i IN start..end loop
#[derive(Debug, Clone)]
pub struct PlPgSQLStmtForI {
    pub varname: String,
    pub lower: PlPgSQLExpr,
    pub upper: PlPgSQLExpr,
    #[allow(dead_code)]
    pub byval: Option<PlPgSQLExpr>,
    pub reverse: bool,
    pub body: Vec<PlPgSQLStmt>,
}

impl<'de> Deserialize<'de> for PlPgSQLStmtForI {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        
        if let Some(obj) = value.as_object() {
            // var can be a string (varname) or an object (PLpgSQL_var)
            let varname = match obj.get("var") {
                Some(Value::String(s)) => s.clone(),
                Some(Value::Object(var_obj)) => {
                    // Extract from PLpgSQL_var
                    var_obj.get("PLpgSQL_var")
                        .and_then(|v| v.get("refname"))
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                        .ok_or_else(|| serde::de::Error::missing_field("refname"))?
                }
                _ => return Err(serde::de::Error::missing_field("var")),
            };
            
            let lower = obj.get("lower")
                .ok_or_else(|| serde::de::Error::missing_field("lower"))
                .and_then(|v| serde_json::from_value(v.clone()).map_err(|e| serde::de::Error::custom(e.to_string())))?;
            
            let upper = obj.get("upper")
                .ok_or_else(|| serde::de::Error::missing_field("upper"))
                .and_then(|v| serde_json::from_value(v.clone()).map_err(|e| serde::de::Error::custom(e.to_string())))?;
            
            let byval = obj.get("byval")
                .and_then(|v| serde_json::from_value(v.clone()).ok());
            
            let reverse = obj.get("reverse")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            
            let body = obj.get("body")
                .ok_or_else(|| serde::de::Error::missing_field("body"))
                .and_then(|v| serde_json::from_value(v.clone()).map_err(|e| serde::de::Error::custom(e.to_string())))?;
            
            return Ok(PlPgSQLStmtForI {
                varname,
                lower,
                upper,
                byval,
                reverse,
                body,
            });
        }
        
        Err(serde::de::Error::custom("Expected object for FOR loop"))
    }
}

/// FOR row IN SELECT loop
#[derive(Debug, Clone, Deserialize)]
pub struct PlPgSQLStmtForS {
    #[serde(rename = "varname")]
    pub varname: String,
    #[serde(rename = "query")]
    pub query: PlPgSQLExpr,
    #[serde(rename = "body")]
    pub body: Vec<PlPgSQLStmt>,
}

/// EXIT/LEAVE statement
#[derive(Debug, Clone, Deserialize)]
pub struct PlPgSQLStmtExit {
    #[serde(default)]
    pub cond: Option<PlPgSQLExpr>,
    #[allow(dead_code)]
    #[serde(default)]
    pub label: Option<String>,
}

/// RETURN statement
#[derive(Debug, Clone, Deserialize)]
pub struct PlPgSQLStmtReturn {
    #[serde(default)]
    pub expr: Option<PlPgSQLExpr>,
}

/// RETURN NEXT statement (for SETOF functions)
#[derive(Debug, Clone, Deserialize)]
pub struct PlPgSQLStmtReturnNext {
    #[serde(rename = "expr")]
    pub expr: PlPgSQLExpr,
}

/// RAISE statement
#[derive(Debug, Clone)]
pub struct PlPgSQLStmtRaise {
    pub elog_level: String,
    pub message: Option<String>,
    pub params: Option<Vec<PlPgSQLExpr>>,
    pub options: Option<Vec<PlPgSQLRaiseOption>>,
}

impl<'de> Deserialize<'de> for PlPgSQLStmtRaise {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        
        if let Some(obj) = value.as_object() {
            // elog_level can be integer or string
            let elog_level = match obj.get("elog_level") {
                Some(Value::Number(n)) => {
                    // Map integer levels to string names
                    // PostgreSQL log levels: DEBUG=10, LOG=14, INFO=17, NOTICE=18, WARNING=19
                    match n.as_i64() {
                        Some(10) => "DEBUG",
                        Some(14) => "LOG",
                        Some(17) => "INFO",
                        Some(18) => "NOTICE",
                        Some(19) => "WARNING",
                        _ => "NOTICE", // default
                    }.to_string()
                }
                Some(Value::String(s)) => s.clone(),
                _ => "NOTICE".to_string(),
            };
            
            let message = obj.get("message")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            
            // Parse params if present
            let params = obj.get("params")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| serde_json::from_value(v.clone()).ok())
                        .collect::<Vec<PlPgSQLExpr>>()
                });
            
            // Parse options if present
            let options = obj.get("options")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| serde_json::from_value(v.clone()).ok())
                        .collect::<Vec<PlPgSQLRaiseOption>>()
                });
            
            return Ok(PlPgSQLStmtRaise {
                elog_level,
                message,
                params,
                options,
            });
        }
        
        Err(serde::de::Error::custom("Expected object for RAISE statement"))
    }
}

/// RAISE option (ERRCODE, MESSAGE, etc.)
#[derive(Debug, Clone, Deserialize)]
pub struct PlPgSQLRaiseOption {
    #[serde(rename = "opt_type")]
    pub opt_type: String,
    #[serde(rename = "expr")]
    pub expr: PlPgSQLExpr,
}

/// SQL expression
/// Uses custom deserializer to handle the PLpgSQL_expr wrapper
#[derive(Debug, Clone)]
pub struct PlPgSQLExpr {
    pub query: String,
    #[allow(dead_code)]
    pub parse_mode: Option<i64>,
}

impl<'de> Deserialize<'de> for PlPgSQLExpr {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        
        // Check if it's wrapped in PLpgSQL_expr
        if let Some(obj) = value.as_object() {
            if let Some(inner) = obj.get("PLpgSQL_expr") {
                let query = inner.get("query")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| serde::de::Error::missing_field("query"))?;
                let parse_mode = inner.get("parseMode").and_then(|v| v.as_i64());
                return Ok(PlPgSQLExpr {
                    query: query.to_string(),
                    parse_mode,
                });
            }
        }
        
        // Try direct deserialization
        #[derive(Deserialize)]
        struct Raw {
            query: String,
            #[serde(default, rename = "parseMode")]
            parse_mode: Option<i64>,
        }
        
        let raw: Raw = serde_json::from_value(value)
            .map_err(|e| serde::de::Error::custom(e.to_string()))?;
        
        Ok(PlPgSQLExpr {
            query: raw.query,
            parse_mode: raw.parse_mode,
        })
    }
}

/// EXECUTE SQL statement
#[derive(Debug, Clone, Deserialize)]
pub struct PlPgSQLStmtExecSql {
    #[serde(rename = "sqlstmt")]
    pub sqlstmt: PlPgSQLExpr,
    #[serde(default)]
    pub into: bool,
    #[serde(default)]
    pub target: Option<PlPgSQLVariable>,
}

/// PERFORM statement (execute SQL, discard result)
#[derive(Debug, Clone, Deserialize)]
pub struct PlPgSQLStmtPerform {
    #[serde(rename = "expr")]
    pub expr: PlPgSQLExpr,
}

/// EXECUTE dynamic SQL
#[derive(Debug, Clone, Deserialize)]
pub struct PlPgSQLStmtDynExecute {
    #[serde(rename = "query")]
    pub query: PlPgSQLExpr,
    #[allow(dead_code)]
    #[serde(default)]
    pub params: Option<Vec<PlPgSQLExpr>>,
    #[allow(dead_code)]
    #[serde(default)]
    pub into: bool,
    #[allow(dead_code)]
    #[serde(default)]
    pub target: Option<PlPgSQLVariable>,
}

/// GET DIAGNOSTICS statement
#[derive(Debug, Clone, Deserialize)]
pub struct PlPgSQLStmtGetDiag {
    #[allow(dead_code)]
    #[serde(rename = "is_stacked")]
    pub is_stacked: bool,
    #[serde(rename = "diag_items")]
    pub diag_items: Vec<PlPgSQLDiagItem>,
}

/// Diagnostic item
#[derive(Debug, Clone, Deserialize)]
pub struct PlPgSQLDiagItem {
    #[serde(rename = "kind")]
    pub kind: i64,
    #[serde(rename = "target_name")]
    pub target_name: String,
}

/// CASE statement
#[derive(Debug, Clone, Deserialize)]
pub struct PlPgSQLStmtCase {
    #[allow(dead_code)]
    #[serde(default)]
    pub expr: Option<PlPgSQLExpr>,
    #[serde(rename = "case_when_list")]
    pub case_when_list: Vec<PlPgSQLCaseWhen>,
    #[serde(default)]
    pub else_stmts: Option<Vec<PlPgSQLStmt>>,
}

/// CASE WHEN branch
#[derive(Debug, Clone, Deserialize)]
pub struct PlPgSQLCaseWhen {
    #[serde(rename = "expr")]
    pub expr: PlPgSQLExpr,
    #[serde(rename = "stmts")]
    pub stmts: Vec<PlPgSQLStmt>,
}

/// Variable reference
#[derive(Debug, Clone, Deserialize)]
pub struct PlPgSQLVariable {
    #[serde(rename = "name")]
    pub name: String,
}

/// Exception handler
#[derive(Debug, Clone, Deserialize)]
pub struct PlPgSQLException {
    #[serde(rename = "sqlstate")]
    pub sqlstate: String,
    #[serde(rename = "stmts")]
    pub stmts: Vec<PlPgSQLStmt>,
}

/// OPEN cursor statement
#[derive(Debug, Clone, Deserialize)]
pub struct PlPgSQLStmtOpen {
    #[serde(rename = "cursorname")]
    pub cursorname: String,
    #[serde(default)]
    pub query: Option<PlPgSQLExpr>,
}

/// FETCH cursor statement
#[derive(Debug, Clone, Deserialize)]
pub struct PlPgSQLStmtFetch {
    #[serde(rename = "cursorname")]
    pub cursorname: String,
    #[serde(default)]
    pub target: Option<PlPgSQLVariable>,
    #[serde(default)]
    pub direction: Option<String>, // FORWARD, BACKWARD, etc.
    #[serde(default)]
    pub count: Option<i64>,
}

/// CLOSE cursor statement
#[derive(Debug, Clone, Deserialize)]
pub struct PlPgSQLStmtClose {
    #[serde(rename = "cursorname")]
    pub cursorname: String,
}

/// MOVE cursor statement
#[derive(Debug, Clone, Deserialize)]
pub struct PlPgSQLStmtMove {
    #[serde(rename = "cursorname")]
    pub cursorname: String,
    #[serde(default)]
    pub direction: Option<String>,
    #[serde(default)]
    pub count: Option<i64>,
}
