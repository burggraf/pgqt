# PL/pgSQL Phase 2 - Detailed Implementation Plan

## Executive Summary

This document provides a comprehensive, implementation-ready specification for adding PL/pgSQL support to PGQT (PostgreSQLite). Based on research into `pg_parse`, `mlua`, and PL/pgSQL semantics, this plan details the architecture, data structures, transpilation strategy, and execution model.

**Key Decision**: Use `pg_parse::parse_plpgsql()` for parsing, transpile to Lua, and execute via `mlua` with a custom sandboxed environment.

---

## Table of Contents

1. [Architecture Overview](#1-architecture-overview)
2. [PL/pgSQL Parser Integration](#2-plpgsql-parser-integration)
3. [AST to Lua Transpiler](#3-ast-to-lua-transpiler)
4. [Lua Runtime Environment](#4-lua-runtime-environment)
5. [Built-in Functions & Special Variables](#5-built-in-functions--special-variables)
6. [Exception Handling](#6-exception-handling)
7. [Trigger Support](#7-trigger-support)
8. [Type Mapping](#8-type-mapping)
9. [Implementation Phases](#9-implementation-phases)
10. [Testing Strategy](#10-testing-strategy)
11. [Performance Considerations](#11-performance-considerations)
12. [Security Model](#12-security-model)

---

## 1. Architecture Overview

### 1.1 System Architecture

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                           PL/pgSQL Execution Flow                            │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  CREATE FUNCTION ... LANGUAGE plpgsql                                        │
│           │                                                                  │
│           ▼                                                                  │
│  ┌─────────────────────────────────────┐                                    │
│  │  Phase 1: Parse (pg_parse)          │                                    │
│  │  ─────────────────────────────────  │                                    │
│  │  pg_parse::parse_plpgsql(sql)       │                                    │
│  │           │                         │                                    │
│  │           ▼                         │                                    │
│  │  JSON AST (PLpgSQL_function)        │                                    │
│  └───────────┬─────────────────────────┘                                    │
│              │                                                               │
│              ▼                                                               │
│  ┌─────────────────────────────────────┐                                    │
│  │  Phase 2: Transpile to Lua          │                                    │
│  │  ─────────────────────────────────  │                                    │
│  │  plpgsql_ast_to_lua(json_ast)       │                                    │
│  │           │                         │                                    │
│  │           ▼                         │                                    │
│  │  Lua source code                    │                                    │
│  └───────────┬─────────────────────────┘                                    │
│              │                                                               │
│              ▼                                                               │
│  ┌─────────────────────────────────────┐                                    │
│  │  Phase 3: Store in Catalog          │                                    │
│  │  ─────────────────────────────────  │                                    │
│  │  Store: metadata + Lua bytecode     │                                    │
│  │  Table: __pg_functions__            │                                    │
│  └───────────┬─────────────────────────┘                                    │
│              │                                                               │
│              ▼ (Function Call)                                               │
│  ┌─────────────────────────────────────┐                                    │
│  │  Phase 4: Execute (mlua)            │                                    │
│  │  ─────────────────────────────────  │                                    │
│  │  Load Lua code into sandbox         │                                    │
│  │  Bind PGQT API (db, special vars)   │                                    │
│  │  Execute with arguments             │                                    │
│  │  Return result                      │                                    │
│  └─────────────────────────────────────┘                                    │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 1.2 Component Responsibilities

| Component | File | Responsibility |
|-----------|------|----------------|
| PL/pgSQL Parser | `src/plpgsql/parser.rs` | Parse PL/pgSQL using `pg_parse`, deserialize JSON AST |
| AST Types | `src/plpgsql/ast.rs` | Rust structs representing PL/pgSQL AST nodes |
| Lua Transpiler | `src/plpgsql/transpiler.rs` | Convert AST to Lua source code |
| Lua Runtime | `src/plpgsql/runtime.rs` | `mlua` integration, sandbox setup, execution |
| PGQT API | `src/plpgsql/api.rs` | Lua bindings for database access, special variables |
| Exception Handler | `src/plpgsql/exception.rs` | SQLSTATE mapping, error handling |
| Trigger Support | `src/plpgsql/trigger.rs` | Trigger variable setup, OLD/NEW row handling |

---

## 2. PL/pgSQL Parser Integration

### 2.1 Dependency Addition

Add to `Cargo.toml`:

```toml
[dependencies]
# Existing dependencies...
pg_parse = "0.16"  # For parse_plpgsql function
mlua = { version = "0.10", features = ["luau", "serialize", "send"] }
```

**Rationale for Luau backend**:
- Better sandboxing support (`sandbox()` method)
- JIT compilation for better performance
- Maintained by Roblox, actively developed
- Compatible with Lua 5.1 syntax

### 2.2 JSON AST Structure

The `pg_parse::parse_plpgsql()` function returns JSON with this structure:

```json
{
  "PLpgSQL_function": {
    "fn_name": "function_name",
    "fn_argnames": ["arg1", "arg2"],
    "fn_argtypes": [23, 25],
    "fn_rettype": 23,
    "fn_body": [
      {
        "PLpgSQL_stmt_block": {
          "body": [
            {"PLpgSQL_stmt_assign": {...}},
            {"PLpgSQL_stmt_if": {...}},
            {"PLpgSQL_stmt_return": {...}}
          ]
        }
      }
    ]
  }
}
```

### 2.3 AST Node Types (Complete List)

Based on PostgreSQL source, these are the PL/pgSQL statement types we need to support:

| AST Node | Description | Priority |
|----------|-------------|----------|
| `PLpgSQL_stmt_block` | BEGIN/END block | P0 |
| `PLpgSQL_stmt_assign` | Variable assignment | P0 |
| `PLpgSQL_stmt_if` | IF/THEN/ELSE/ELSIF | P0 |
| `PLpgSQL_stmt_case` | CASE statement | P1 |
| `PLpgSQL_stmt_loop` | LOOP/END LOOP | P0 |
| `PLpgSQL_stmt_while` | WHILE loop | P0 |
| `PLpgSQL_stmt_fori` | FOR i IN start..end | P0 |
| `PLpgSQL_stmt_fors` | FOR row IN SELECT | P0 |
| `PLpgSQL_stmt_foreach_a` | FOREACH array loop | P1 |
| `PLpgSQL_stmt_exit` | EXIT/LEAVE loop | P0 |
| `PLpgSQL_stmt_return` | RETURN statement | P0 |
| `PLpgSQL_stmt_return_next` | RETURN NEXT (SETOF) | P1 |
| `PLpgSQL_stmt_return_query` | RETURN QUERY | P1 |
| `PLpgSQL_stmt_raise` | RAISE statement | P0 |
| `PLpgSQL_stmt_execsql` | SQL statement | P0 |
| `PLpgSQL_stmt_dynexecute` | EXECUTE dynamic SQL | P1 |
| `PLpgSQL_stmt_dynfors` | FOR IN EXECUTE | P1 |
| `PLpgSQL_stmt_getdiag` | GET DIAGNOSTICS | P1 |
| `PLpgSQL_stmt_open` | OPEN cursor | P2 |
| `PLpgSQL_stmt_fetch` | FETCH cursor | P2 |
| `PLpgSQL_stmt_close` | CLOSE cursor | P2 |
| `PLpgSQL_stmt_perform` | PERFORM query | P0 |
| `PLpgSQL_stmt_call` | CALL procedure | P2 |
| `PLpgSQL_stmt_commit` | COMMIT | P2 |
| `PLpgSQL_stmt_rollback` | ROLLBACK | P2 |

### 2.4 Rust AST Types

```rust
// src/plpgsql/ast.rs

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PlpgsqlFunction {
    pub fn_name: Option<String>,
    pub fn_argnames: Option<Vec<String>>,
    pub fn_argtypes: Option<Vec<i64>>,
    pub fn_rettype: Option<i64>,
    pub fn_body: Vec<PlPgSQLStmt>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case", tag = "stmt_type")]
pub enum PlPgSQLStmt {
    #[serde(rename = "PLpgSQL_stmt_block")]
    Block(PlPgSQLStmtBlock),
    #[serde(rename = "PLpgSQL_stmt_assign")]
    Assign(PlPgSQLStmtAssign),
    #[serde(rename = "PLpgSQL_stmt_if")]
    If(PlPgSQLStmtIf),
    #[serde(rename = "PLpgSQL_stmt_loop")]
    Loop(PlPgSQLStmtLoop),
    #[serde(rename = "PLpgSQL_stmt_while")]
    While(PlPgSQLStmtWhile),
    #[serde(rename = "PLpgSQL_stmt_fori")]
    ForI(PlPgSQLStmtForI),
    #[serde(rename = "PLpgSQL_stmt_fors")]
    ForS(PlPgSQLStmtForS),
    #[serde(rename = "PLpgSQL_stmt_exit")]
    Exit(PlPgSQLStmtExit),
    #[serde(rename = "PLpgSQL_stmt_return")]
    Return(PlPgSQLStmtReturn),
    #[serde(rename = "PLpgSQL_stmt_return_next")]
    ReturnNext(PlPgSQLStmtReturnNext),
    #[serde(rename = "PLpgSQL_stmt_raise")]
    Raise(PlPgSQLStmtRaise),
    #[serde(rename = "PLpgSQL_stmt_execsql")]
    ExecSql(PlPgSQLStmtExecSql),
    #[serde(rename = "PLpgSQL_stmt_dynexecute")]
    DynExecute(PlPgSQLStmtDynExecute),
    #[serde(rename = "PLpgSQL_stmt_getdiag")]
    GetDiag(PlPgSQLStmtGetDiag),
    #[serde(rename = "PLpgSQL_stmt_perform")]
    Perform(PlPgSQLStmtPerform),
    #[serde(rename = "PLpgSQL_stmt_case")]
    Case(PlPgSQLStmtCase),
}

#[derive(Debug, Clone, Deserialize)]
pub struct PlPgSQLStmtBlock {
    pub body: Vec<PlPgSQLStmt>,
    #[serde(default)]
    pub exceptions: Option<Vec<PlPgSQLException>>, // Simplified
}

#[derive(Debug, Clone, Deserialize)]
pub struct PlPgSQLStmtAssign {
    pub varname: String,
    pub expr: PlPgSQLExpr,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PlPgSQLStmtIf {
    pub cond: PlPgSQLExpr,
    pub then_body: Vec<PlPgSQLStmt>,
    #[serde(default)]
    pub elsif_list: Option<Vec<PlPgSQLStmtIfElsif>>,
    #[serde(default)]
    pub else_body: Option<Vec<PlPgSQLStmt>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PlPgSQLStmtIfElsif {
    pub cond: PlPgSQLExpr,
    pub stmts: Vec<PlPgSQLStmt>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PlPgSQLExpr {
    pub query: String,
    #[serde(default)]
    pub parse_mode: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PlPgSQLStmtReturn {
    #[serde(default)]
    pub expr: Option<PlPgSQLExpr>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PlPgSQLStmtRaise {
    pub elog_level: String, // DEBUG, INFO, NOTICE, WARNING, EXCEPTION
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub params: Option<Vec<PlPgSQLExpr>>,
    #[serde(default)]
    pub options: Option<Vec<PlPgSQLRaiseOption>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PlPgSQLRaiseOption {
    pub opt_type: String, // ERRCODE, MESSAGE, DETAIL, HINT
    pub expr: PlPgSQLExpr,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PlPgSQLStmtExecSql {
    pub sqlstmt: PlPgSQLExpr,
    #[serde(default)]
    pub into: bool,
    #[serde(default)]
    pub target: Option<PlPgSQLVariable>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PlPgSQLStmtPerform {
    pub expr: PlPgSQLExpr,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PlPgSQLStmtGetDiag {
    pub is_stacked: bool,
    pub diag_items: Vec<PlPgSQLDiagItem>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PlPgSQLDiagItem {
    pub kind: i64,
    pub target_name: String,
}

// ... additional types for loops, cursors, etc.
```

### 2.5 Parser Implementation

```rust
// src/plpgsql/parser.rs

use anyhow::{Result, Context};
use serde_json::Value;
use crate::plpgsql::ast::PlpgsqlFunction;

/// Parse PL/pgSQL function source and return AST
pub fn parse_plpgsql_function(sql: &str) -> Result<PlpgsqlFunction> {
    // Use pg_parse to get JSON AST
    let json_str = pg_parse::parse_plpgsql(sql)
        .map_err(|e| anyhow::anyhow!("Failed to parse PL/pgSQL: {}", e))?;
    
    // Parse JSON
    let json: Value = serde_json::from_str(&json_str)
        .context("Failed to parse PL/pgSQL AST JSON")?;
    
    // Extract PLpgSQL_function object
    let func_json = json.get("PLpgSQL_function")
        .ok_or_else(|| anyhow::anyhow!("Expected PLpgSQL_function in AST"))?;
    
    // Deserialize to our Rust types
    let function: PlpgsqlFunction = serde_json::from_value(func_json.clone())
        .context("Failed to deserialize PL/pgSQL AST")?;
    
    Ok(function)
}

/// Parse multiple functions (e.g., from CREATE OR REPLACE FUNCTION)
pub fn parse_plpgsql_batch(sql: &str) -> Result<Vec<PlpgsqlFunction>> {
    let json_str = pg_parse::parse_plpgsql(sql)
        .map_err(|e| anyhow::anyhow!("Failed to parse PL/pgSQL batch: {}", e))?;
    
    let json: Value = serde_json::from_str(&json_str)?;
    
    // pg_parse returns an array for multiple functions
    let functions: Vec<PlpgsqlFunction> = if let Some(arr) = json.as_array() {
        arr.iter()
            .map(|v| {
                let func_json = v.get("PLpgSQL_function")
                    .ok_or_else(|| anyhow::anyhow!("Expected PLpgSQL_function"))?;
                serde_json::from_value(func_json.clone())
                    .context("Failed to deserialize function")
            })
            .collect::<Result<Vec<_>>>()?
    } else {
        vec![parse_plpgsql_function(sql)?]
    };
    
    Ok(functions)
}
```

---

## 3. AST to Lua Transpiler

### 3.1 Transpilation Strategy

The transpiler converts PL/pgSQL AST nodes to Lua code following these principles:

1. **Variables → Lua locals**: PL/pgSQL variables become Lua local variables
2. **SQL expressions → pgqt.query()**: SQL fragments are executed via the PGQT API
3. **Control flow → Lua control flow**: IF/LOOP/WHILE map directly
4. **Exceptions → pcall/xpcall**: PL/pgSQL EXCEPTION blocks use Lua error handling
5. **Cursors → Lua iterators**: Cursor operations become iterator patterns

### 3.2 Core Transpiler Structure

```rust
// src/plpgsql/transpiler.rs

use anyhow::Result;
use crate::plpgsql::ast::*;
use std::fmt::Write;

/// Transpile PL/pgSQL AST to Lua source code
pub fn transpile_to_lua(function: &PlpgsqlFunction) -> Result<String> {
    let mut ctx = TranspileContext::new();
    
    // Generate function header
    ctx.emit_line("-- Generated from PL/pgSQL");
    ctx.emit_line(&format!("local function {}(_ctx, ...)", function.fn_name.as_deref().unwrap_or("anonymous")));
    ctx.indent();
    
    // Emit parameter declarations
    emit_parameters(&mut ctx, function)?;
    
    // Emit variable declarations (from DECLARE block)
    ctx.emit_line("-- Variable declarations");
    
    // Emit function body
    for stmt in &function.fn_body {
        emit_statement(&mut ctx, stmt)?;
    }
    
    ctx.dedent();
    ctx.emit_line("end");
    ctx.emit_line(&format!("return {}", function.fn_name.as_deref().unwrap_or("anonymous")));
    
    Ok(ctx.output)
}

/// Context for transpilation
struct TranspileContext {
    output: String,
    indent_level: usize,
    label_stack: Vec<String>,
    loop_depth: usize,
}

impl TranspileContext {
    fn new() -> Self {
        Self {
            output: String::new(),
            indent_level: 0,
            label_stack: Vec::new(),
            loop_depth: 0,
        }
    }
    
    fn indent(&mut self) {
        self.indent_level += 1;
    }
    
    fn dedent(&mut self) {
        self.indent_level -= 1;
    }
    
    fn emit_line(&mut self, line: &str) {
        let indent = "  ".repeat(self.indent_level);
        writeln!(self.output, "{}{}", indent, line).unwrap();
    }
    
    fn emit(&mut self, text: &str) {
        self.output.push_str(text);
    }
}

/// Emit parameter handling
fn emit_parameters(ctx: &mut TranspileContext, function: &PlpgsqlFunction) -> Result<()> {
    if let Some(argnames) = &function.fn_argnames {
        ctx.emit_line("-- Parameters");
        for (i, name) in argnames.iter().enumerate() {
            ctx.emit_line(&format!("local {} = select({})", name, i + 1));
        }
    }
    Ok(())
}

/// Emit a single statement
fn emit_statement(ctx: &mut TranspileContext, stmt: &PlPgSQLStmt) -> Result<()> {
    match stmt {
        PlPgSQLStmt::Block(block) => emit_block(ctx, block)?,
        PlPgSQLStmt::Assign(assign) => emit_assign(ctx, assign)?,
        PlPgSQLStmt::If(if_stmt) => emit_if(ctx, if_stmt)?,
        PlPgSQLStmt::Loop(loop_stmt) => emit_loop(ctx, loop_stmt)?,
        PlPgSQLStmt::While(while_stmt) => emit_while(ctx, while_stmt)?,
        PlPgSQLStmt::ForI(for_i) => emit_for_i(ctx, for_i)?,
        PlPgSQLStmt::ForS(for_s) => emit_for_s(ctx, for_s)?,
        PlPgSQLStmt::Exit(exit) => emit_exit(ctx, exit)?,
        PlPgSQLStmt::Return(ret) => emit_return(ctx, ret)?,
        PlPgSQLStmt::ReturnNext(ret_next) => emit_return_next(ctx, ret_next)?,
        PlPgSQLStmt::Raise(raise) => emit_raise(ctx, raise)?,
        PlPgSQLStmt::ExecSql(exec) => emit_exec_sql(ctx, exec)?,
        PlPgSQLStmt::Perform(perform) => emit_perform(ctx, perform)?,
        PlPgSQLStmt::DynExecute(dyn) => emit_dyn_execute(ctx, dyn)?,
        PlPgSQLStmt::GetDiag(diag) => emit_get_diag(ctx, diag)?,
        PlPgSQLStmt::Case(case) => emit_case(ctx, case)?,
        _ => ctx.emit_line(&format!("-- TODO: Unimplemented statement: {:?}", stmt)),
    }
    Ok(())
}
```

### 3.3 Statement Transpilation Examples

#### Variable Assignment

PL/pgSQL:
```sql
v_count := v_count + 1;
v_name := 'Hello, ' || user_name;
```

Lua:
```lua
v_count = _ctx.scalar("SELECT $1 + 1", {v_count})
v_name = _ctx.scalar("SELECT $1 || $2", {"Hello, ", user_name})
```

Implementation:
```rust
fn emit_assign(ctx: &mut TranspileContext, assign: &PlPgSQLStmtAssign) -> Result<()> {
    let expr_lua = transpile_expr(&assign.expr)?;
    ctx.emit_line(&format!("{} = {}", assign.varname, expr_lua));
    Ok(())
}

fn transpile_expr(expr: &PlPgSQLExpr) -> Result<String> {
    // SQL expressions are executed via _ctx.scalar
    Ok(format!("_ctx.scalar([[{}]], {{}})", expr.query))
}
```

#### IF Statement

PL/pgSQL:
```sql
IF v_count > 10 THEN
    RAISE NOTICE 'Count is high';
ELSIF v_count > 5 THEN
    RAISE NOTICE 'Count is medium';
ELSE
    RAISE NOTICE 'Count is low';
END IF;
```

Lua:
```lua
if _ctx.scalar("SELECT $1 > 10", {v_count}) then
  _ctx.raise("NOTICE", "Count is high")
elseif _ctx.scalar("SELECT $1 > 5", {v_count}) then
  _ctx.raise("NOTICE", "Count is medium")
else
  _ctx.raise("NOTICE", "Count is low")
end
```

Implementation:
```rust
fn emit_if(ctx: &mut TranspileContext, if_stmt: &PlPgSQLStmtIf) -> Result<()> {
    let cond = transpile_expr(&if_stmt.cond)?;
    ctx.emit_line(&format!("if {} then", cond));
    ctx.indent();
    for stmt in &if_stmt.then_body {
        emit_statement(ctx, stmt)?;
    }
    ctx.dedent();
    
    // ELSIF branches
    if let Some(elsif_list) = &if_stmt.elsif_list {
        for elsif in elsif_list {
            let elsif_cond = transpile_expr(&elsif.cond)?;
            ctx.emit_line(&format!("elseif {} then", elsif_cond));
            ctx.indent();
            for stmt in &elsif.stmts {
                emit_statement(ctx, stmt)?;
            }
            ctx.dedent();
        }
    }
    
    // ELSE branch
    if let Some(else_body) = &if_stmt.else_body {
        ctx.emit_line("else");
        ctx.indent();
        for stmt in else_body {
            emit_statement(ctx, stmt)?;
        }
        ctx.dedent();
    }
    
    ctx.emit_line("end");
    Ok(())
}
```

#### RETURN Statement

PL/pgSQL:
```sql
RETURN v_result;
RETURN;  -- For void functions
```

Lua:
```lua
return v_result
return  -- For void functions
```

Implementation:
```rust
fn emit_return(ctx: &mut TranspileContext, ret: &PlPgSQLStmtReturn) -> Result<()> {
    if let Some(expr) = &ret.expr {
        let expr_lua = transpile_expr(expr)?;
        ctx.emit_line(&format!("return {}", expr_lua));
    } else {
        ctx.emit_line("return");
    }
    Ok(())
}
```

#### RAISE Statement

PL/pgSQL:
```sql
RAISE NOTICE 'Value is %', v_value;
RAISE EXCEPTION 'Invalid input: %', v_input USING ERRCODE = '22023';
```

Lua:
```lua
_ctx.raise("NOTICE", "Value is %s", v_value)
_ctx.raise_exception("Invalid input: %s", v_input, {errcode = "22023"})
```

Implementation:
```rust
fn emit_raise(ctx: &mut TranspileContext, raise: &PlPgSQLStmtRaise) -> Result<()> {
    let level = &raise.elog_level;
    let message = raise.message.as_deref().unwrap_or("");
    
    if level == "EXCEPTION" {
        // Extract ERRCODE if present
        let mut errcode = "P0001".to_string(); // default raise_exception
        if let Some(options) = &raise.options {
            for opt in options {
                if opt.opt_type == "ERRCODE" {
                    errcode = opt.expr.query.clone();
                }
            }
        }
        
        if let Some(params) = &raise.params {
            let param_list: Vec<String> = params.iter()
                .map(|p| transpile_expr(p).unwrap_or_else(|_| "nil".to_string()))
                .collect();
            ctx.emit_line(&format!(
                "_ctx.raise_exception("{}"{}, {{errcode = "{}"}})",
                message,
                if param_list.is_empty() { "".to_string() } else { format!(", {}", param_list.join(", ")) },
                errcode
            ));
        } else {
            ctx.emit_line(&format!("_ctx.raise_exception("{}", {{errcode = "{}"}})", message, errcode));
        }
    } else {
        // DEBUG, INFO, NOTICE, WARNING
        if let Some(params) = &raise.params {
            let param_list: Vec<String> = params.iter()
                .map(|p| transpile_expr(p).unwrap_or_else(|_| "nil".to_string()))
                .collect();
            ctx.emit_line(&format!(
                "_ctx.raise("{}", "{}"{})",
                level,
                message,
                if param_list.is_empty() { "".to_string() } else { format!(", {}", param_list.join(", ")) }
            ));
        } else {
            ctx.emit_line(&format!("_ctx.raise("{}", "{}")", level, message));
        }
    }
    
    Ok(())
}
```

#### PERFORM Statement

PL/pgSQL:
```sql
PERFORM some_function(arg1, arg2);
PERFORM 1 FROM users WHERE active = true;
```

Lua:
```lua
_ctx.perform("SELECT some_function($1, $2)", {arg1, arg2})
_ctx.perform("SELECT 1 FROM users WHERE active = true", {})
```

Implementation:
```rust
fn emit_perform(ctx: &mut TranspileContext, perform: &PlPgSQLStmtPerform) -> Result<()> {
    let query = &perform.expr.query;
    ctx.emit_line(&format!("_ctx.perform([[{}]], {{}})", query));
    Ok(())
}
```

#### EXECUTE Dynamic SQL

PL/pgSQL:
```sql
EXECUTE 'SELECT * FROM ' || quote_ident(table_name) WHERE id = $1 USING v_id;
```

Lua:
```lua
_ctx.execute(string.format("SELECT * FROM %s WHERE id = $1", _ctx.quote_ident(table_name)), {v_id})
```

Implementation:
```rust
fn emit_dyn_execute(ctx: &mut TranspileContext, dyn: &PlPgSQLStmtDynExecute) -> Result<()> {
    let query = &dyn.query.query;
    ctx.emit_line(&format!("_ctx.execute([{}], {{}})", query));
    Ok(())
}
```

#### Loops

PL/pgSQL:
```sql
LOOP
    EXIT WHEN v_count > 10;
    v_count := v_count + 1;
END LOOP;

WHILE v_count < 10 LOOP
    v_count := v_count + 1;
END LOOP;

FOR i IN 1..10 LOOP
    RAISE NOTICE 'i = %', i;
END LOOP;

FOR rec IN SELECT * FROM users LOOP
    RAISE NOTICE 'User: %', rec.name;
END LOOP;
```

Lua:
```lua
while true do
  if v_count > 10 then break end
  v_count = _ctx.scalar("SELECT $1 + 1", {v_count})
end

while v_count < 10 do
  v_count = _ctx.scalar("SELECT $1 + 1", {v_count})
end

for i = 1, 10 do
  _ctx.raise("NOTICE", "i = %s", i)
end

for rec in _ctx.query_iter("SELECT * FROM users", {}) do
  _ctx.raise("NOTICE", "User: %s", rec.name)
end
```

Implementation:
```rust
fn emit_loop(ctx: &mut TranspileContext, loop_stmt: &PlPgSQLStmtLoop) -> Result<()> {
    ctx.loop_depth += 1;
    ctx.emit_line("while true do");
    ctx.indent();
    for stmt in &loop_stmt.body {
        emit_statement(ctx, stmt)?;
    }
    ctx.dedent();
    ctx.emit_line("end");
    ctx.loop_depth -= 1;
    Ok(())
}

fn emit_while(ctx: &mut TranspileContext, while_stmt: &PlPgSQLStmtWhile) -> Result<()> {
    let cond = transpile_expr(&while_stmt.cond)?;
    ctx.emit_line(&format!("while {} do", cond));
    ctx.indent();
    for stmt in &while_stmt.body {
        emit_statement(ctx, stmt)?;
    }
    ctx.dedent();
    ctx.emit_line("end");
    Ok(())
}

fn emit_for_i(ctx: &mut TranspileContext, for_i: &PlPgSQLStmtForI) -> Result<()> {
    let lower = transpile_expr(&for_i.lower)?;
    let upper = transpile_expr(&for_i.upper)?;
    let var = &for_i.varname;
    ctx.emit_line(&format!("for {} = {}, {} do", var, lower, upper));
    ctx.indent();
    for stmt in &for_i.body {
        emit_statement(ctx, stmt)?;
    }
    ctx.dedent();
    ctx.emit_line("end");
    Ok(())
}

fn emit_for_s(ctx: &mut TranspileContext, for_s: &PlPgSQLStmtForS) -> Result<()> {
    let query = &for_s.query.query;
    let var = &for_s.varname;
    ctx.emit_line(&format!("for {} in _ctx.query_iter([[{}]], {{}}) do", var, query));
    ctx.indent();
    for stmt in &for_s.body {
        emit_statement(ctx, stmt)?;
    }
    ctx.dedent();
    ctx.emit_line("end");
    Ok(())
}

fn emit_exit(ctx: &mut TranspileContext, exit: &PlPgSQLStmtExit) -> Result<()> {
    if let Some(cond) = &exit.cond {
        let cond_lua = transpile_expr(cond)?;
        ctx.emit_line(&format!("if {} then break end", cond_lua));
    } else {
        ctx.emit_line("break");
    }
    Ok(())
}
```

#### Exception Handling

PL/pgSQL:
```sql
BEGIN
    -- risky code
EXCEPTION
    WHEN division_by_zero THEN
        RAISE NOTICE 'Division by zero';
    WHEN OTHERS THEN
        RAISE NOTICE 'Error: %', SQLERRM;
END;
```

Lua:
```lua
local _ok, _err = pcall(function()
  -- risky code
end)
if not _ok then
  local _sqlstate = _err.sqlstate or "P0001"
  local _sqlerrm = _err.message or tostring(_err)
  if _sqlstate == "22012" then
    _ctx.raise("NOTICE", "Division by zero")
  else
    _ctx.raise("NOTICE", "Error: %s", _sqlerrm)
  end
end
```

Implementation:
```rust
fn emit_block(ctx: &mut TranspileContext, block: &PlPgSQLStmtBlock) -> Result<()> {
    if let Some(exceptions) = &block.exceptions {
        // Block with exception handler
        ctx.emit_line("local _ok, _err = pcall(function()");
        ctx.indent();
        for stmt in &block.body {
            emit_statement(ctx, stmt)?;
        }
        ctx.dedent();
        ctx.emit_line("end)");
        ctx.emit_line("if not _ok then");
        ctx.indent();
        ctx.emit_line("local _sqlstate = _err and _err.sqlstate or 'P0001'");
        ctx.emit_line("local _sqlerrm = _err and _err.message or tostring(_err)");
        
        // Emit WHEN clauses
        for (i, exc) in exceptions.iter().enumerate() {
            if i == 0 {
                ctx.emit_line(&format!("if _sqlstate == '{}' then", exc.sqlstate));
            } else {
                ctx.emit_line(&format!("elseif _sqlstate == '{}' then", exc.sqlstate));
            }
            ctx.indent();
            for stmt in &exc.stmts {
                emit_statement(ctx, stmt)?;
            }
            ctx.dedent();
        }
        
        ctx.emit_line("else");
        ctx.indent();
        ctx.emit_line("error(_err)"); -- Re-raise if not caught
        ctx.dedent();
        ctx.emit_line("end");
        ctx.dedent();
        ctx.emit_line("end");
    } else {
        // Regular block
        for stmt in &block.body {
            emit_statement(ctx, stmt)?;
        }
    }
    Ok(())
}
```

---

## 4. Lua Runtime Environment

### 4.1 Runtime Architecture

```rust
// src/plpgsql/runtime.rs

use mlua::{Lua, Table, Value as LuaValue, Function as LuaFunction, Variadic};
use rusqlite::{Connection, types::Value as SqliteValue};
use anyhow::Result;
use std::sync::{Arc, Mutex};

/// Runtime environment for executing PL/pgSQL (transpiled to Lua)
pub struct PlPgSqlRuntime {
    lua: Lua,
    // Cache for compiled functions
    function_cache: Arc<Mutex<HashMap<String, LuaFunction>>>,
}

/// Execution context passed to Lua functions
pub struct ExecutionContext {
    conn: Arc<Mutex<Connection>>,
    // Special variables
    sqlstate: Option<String>,
    sqlerrm: Option<String>,
    // Trigger variables (when applicable)
    trigger_data: Option<TriggerData>,
    // Row count from last operation
    row_count: i64,
    // Result set for RETURN NEXT
    result_set: Vec<Vec<SqliteValue>>,
}

#[derive(Clone)]
pub struct TriggerData {
    pub tg_name: String,
    pub tg_when: String,  // BEFORE, AFTER, INSTEAD OF
    pub tg_op: String,    // INSERT, UPDATE, DELETE, TRUNCATE
    pub tg_level: String, // ROW, STATEMENT
    pub tg_relid: i64,
    pub tg_table_name: String,
    pub tg_table_schema: String,
    pub tg_nargs: i64,
    pub tg_argv: Vec<String>,
    pub new_row: Option<HashMap<String, SqliteValue>>,
    pub old_row: Option<HashMap<String, SqliteValue>>,
}
```

### 4.2 Runtime Initialization

```rust
impl PlPgSqlRuntime {
    pub fn new() -> Result<Self> {
        // Create Luau VM with sandbox enabled
        let lua = Lua::new_with(
            mlua::StdLib::TABLE | mlua::StdLib::STRING | mlua::StdLib::MATH,
            mlua::LuaOptions::new(),
        )?;
        
        // Enable sandbox (Luau-specific)
        #[cfg(feature = "luau")]
        lua.sandbox(true)?;
        
        let runtime = Self {
            lua,
            function_cache: Arc::new(Mutex::new(HashMap::new())),
        };
        
        // Register built-in functions
        runtime.register_builtins()?;
        
        Ok(runtime)
    }
    
    fn register_builtins(&self) -> Result<()> {
        let globals = self.lua.globals();
        
        // select(n, ...) - Access function arguments
        let select_fn = self.lua.create_function(|lua, (n,): (i64,)| {
            let args: Variadic<LuaValue> = lua.globals().get("_args")?;
            if n >= 1 && n <= args.len() as i64 {
                Ok(args.get((n - 1) as usize).cloned().unwrap_or(LuaValue::Nil))
            } else {
                Ok(LuaValue::Nil)
            }
        })?;
        globals.set("select", select_fn)?;
        
        // string.format for RAISE message formatting
        // Already available in Luau stdlib
        
        Ok(())
    }
}
```

### 4.3 PGQT API for Lua

```rust
impl ExecutionContext {
    /// Create a Lua table with the PGQT API
    fn create_api_table(&self, lua: &Lua) -> Result<Table> {
        let api = lua.create_table()?;
        let conn = self.conn.clone();
        
        // _ctx.scalar(query, params) -> value
        let scalar_fn = lua.create_function(move |lua, (query, params): (String, Vec<LuaValue>)| {
            let sqlite_params: Vec<SqliteValue> = params.into_iter()
                .map(lua_to_sqlite)
                .collect::<Result<Vec<_>, _>>()?;
            
            let conn = conn.lock().unwrap();
            let result: Option<SqliteValue> = conn.query_row(&query, &sqlite_params[..], |row| {
                row.get(0)
            }).optional()?;
            
            Ok(sqlite_to_lua(lua, result.unwrap_or(SqliteValue::Null))?)
        })?;
        api.set("scalar", scalar_fn)?;
        
        // _ctx.query(query, params) -> table of rows
        let conn2 = self.conn.clone();
        let query_fn = lua.create_function(move |lua, (query, params): (String, Vec<LuaValue>)| {
            let sqlite_params: Vec<SqliteValue> = params.into_iter()
                .map(lua_to_sqlite)
                .collect::<Result<Vec<_>, _>>()?;
            
            let conn = conn2.lock().unwrap();
            let mut stmt = conn.prepare(&query)?;
            let column_count = stmt.column_count();
            
            let rows: Vec<Table> = stmt.query_map(&sqlite_params[..], |row| {
                let row_table = lua.create_table()?;
                for i in 0..column_count {
                    let col_name = stmt.column_name(i)?;
                    let value: SqliteValue = row.get(i)?;
                    row_table.set(col_name, sqlite_to_lua(lua, value)?)?;
                }
                Ok(row_table)
            })?.collect::<Result<Vec<_>, _>>()?;
            
            Ok(rows)
        })?;
        api.set("query", query_fn)?;
        
        // _ctx.query_iter(query, params) -> iterator function
        let conn3 = self.conn.clone();
        let query_iter_fn = lua.create_function(move |lua, (query, params): (String, Vec<LuaValue>)| {
            // Return an iterator function
            let conn = conn3.clone();
            let iter = lua.create_function_mut(move |lua, (): ()| {
                // Implementation would maintain cursor state
                // Simplified version returns next row or nil
                Ok(LuaValue::Nil)
            })?;
            Ok(iter)
        })?;
        api.set("query_iter", query_iter_fn)?;
        
        // _ctx.exec(query, params) -> row_count
        let conn4 = self.conn.clone();
        let exec_fn = lua.create_function(move |_lua, (query, params): (String, Vec<LuaValue>)| {
            let sqlite_params: Vec<SqliteValue> = params.into_iter()
                .map(lua_to_sqlite)
                .collect::<Result<Vec<_>, _>>()?;
            
            let conn = conn4.lock().unwrap();
            let rows_affected = conn.execute(&query, &sqlite_params[..])?;
            
            Ok(rows_affected as i64)
        })?;
        api.set("exec", exec_fn)?;
        
        // _ctx.perform(query, params) -> nil
        let perform_fn = lua.create_function(move |_lua, (query, params): (String, Vec<LuaValue>)| {
            let sqlite_params: Vec<SqliteValue> = params.into_iter()
                .map(lua_to_sqlite)
                .collect::<Result<Vec<_>, _>>()?;
            
            let conn = self.conn.lock().unwrap();
            conn.execute(&query, &sqlite_params[..])?;
            
            Ok(())
        })?;
        api.set("perform", perform_fn)?;
        
        // _ctx.execute(query, params) -> nil (dynamic SQL)
        let conn5 = self.conn.clone();
        let execute_fn = lua.create_function(move |_lua, (query, params): (String, Vec<LuaValue>)| {
            let sqlite_params: Vec<SqliteValue> = params.into_iter()
                .map(lua_to_sqlite)
                .collect::<Result<Vec<_>, _>>()?;
            
            let conn = conn5.lock().unwrap();
            conn.execute(&query, &sqlite_params[..])?;
            
            Ok(())
        })?;
        api.set("execute", execute_fn)?;
        
        // _ctx.raise(level, message, ...)
        let raise_fn = lua.create_function(|_lua, (level, message, args): (String, String, Variadic<LuaValue>)| {
            let formatted = if args.is_empty() {
                message
            } else {
                // Simple % substitution
                let mut result = message;
                for arg in args {
                    result = result.replacen("%s", &format!("{}", arg), 1);
                }
                result
            };
            
            match level.as_str() {
                "DEBUG" => println!("DEBUG: {}", formatted),
                "INFO" => println!("INFO: {}", formatted),
                "NOTICE" => println!("NOTICE: {}", formatted),
                "WARNING" => eprintln!("WARNING: {}", formatted),
                _ => println!("{}: {}", level, formatted),
            }
            
            Ok(())
        })?;
        api.set("raise", raise_fn)?;
        
        // _ctx.raise_exception(message, options)
        let raise_ex_fn = lua.create_function(|_lua, (message, options): (String, Option<Table>)| {
            let errcode = options
                .and_then(|t| t.get::<_, String>("errcode").ok())
                .unwrap_or_else(|| "P0001".to_string());
            
            Err(mlua::Error::RuntimeError(format!(
                "{{\"sqlstate\": \"{}\", \"message\": \"{}\"}}",
                errcode, message
            )))
        })?;
        api.set("raise_exception", raise_ex_fn)?;
        
        // _ctx.quote_ident(ident) -> quoted identifier
        let quote_ident_fn = lua.create_function(|_lua, ident: String| {
            // Simple implementation - production would handle reserved words
            if ident.chars().all(|c| c.is_alphanumeric() || c == '_') 
                && !ident.chars().next().unwrap_or('_').is_ascii_digit() {
                Ok(ident)
            } else {
                Ok(format!("\"{}\"", ident.replace('"', "\"\"")))
            }
        })?;
        api.set("quote_ident", quote_ident_fn)?;
        
        // Special variables
        if let Some(sqlstate) = &self.sqlstate {
            api.set("SQLSTATE", sqlstate.clone())?;
        }
        if let Some(sqlerrm) = &self.sqlerrm {
            api.set("SQLERRM", sqlerrm.clone())?;
        }
        
        // Trigger variables (if in trigger context)
        if let Some(trigger) = &self.trigger_data {
            api.set("TG_NAME", trigger.tg_name.clone())?;
            api.set("TG_WHEN", trigger.tg_when.clone())?;
            api.set("TG_OP", trigger.tg_op.clone())?;
            api.set("TG_LEVEL", trigger.tg_level.clone())?;
            api.set("TG_RELID", trigger.tg_relid)?;
            api.set("TG_RELNAME", trigger.tg_table_name.clone())?;
            api.set("TG_TABLE_NAME", trigger.tg_table_name.clone())?;
            api.set("TG_TABLE_SCHEMA", trigger.tg_table_schema.clone())?;
            api.set("TG_NARGS", trigger.tg_nargs)?;
            
            let argv = lua.create_table()?;
            for (i, arg) in trigger.tg_argv.iter().enumerate() {
                argv.set(i + 1, arg.clone())?;
            }
            api.set("TG_ARGV", argv)?;
            
            // NEW and OLD as tables
            if let Some(new) = &trigger.new_row {
                let new_table = lua.create_table()?;
                for (k, v) in new {
                    new_table.set(k.clone(), sqlite_to_lua(lua, v.clone())?)?;
                }
                api.set("NEW", new_table)?;
            }
            
            if let Some(old) = &trigger.old_row {
                let old_table = lua.create_table()?;
                for (k, v) in old {
                    old_table.set(k.clone(), sqlite_to_lua(lua, v.clone())?)?;
                }
                api.set("OLD", old_table)?;
            }
        }
        
        Ok(api)
    }
}
```

### 4.4 Type Conversion

```rust
/// Convert Lua value to SQLite value
fn lua_to_sqlite(value: LuaValue) -> Result<SqliteValue, mlua::Error> {
    match value {
        LuaValue::Nil => Ok(SqliteValue::Null),
        LuaValue::Boolean(b) => Ok(SqliteValue::Integer(if b { 1 } else { 0 })),
        LuaValue::Integer(i) => Ok(SqliteValue::Integer(i)),
        LuaValue::Number(n) => Ok(SqliteValue::Real(n)),
        LuaValue::String(s) => Ok(SqliteValue::Text(s.to_string_lossy().to_string())),
        _ => Err(mlua::Error::RuntimeError(
            format!("Cannot convert Lua value to SQLite: {:?}", value)
        )),
    }
}

/// Convert SQLite value to Lua value
fn sqlite_to_lua(lua: &Lua, value: SqliteValue) -> Result<LuaValue, mlua::Error> {
    match value {
        SqliteValue::Null => Ok(LuaValue::Nil),
        SqliteValue::Integer(i) => Ok(LuaValue::Integer(i)),
        SqliteValue::Real(f) => Ok(LuaValue::Number(f)),
        SqliteValue::Text(s) => Ok(LuaValue::String(lua.create_string(&s)?)),
        SqliteValue::Blob(b) => Ok(LuaValue::String(lua.create_string(&b)?)),
    }
}
```

### 4.5 Function Execution

```rust
impl PlPgSqlRuntime {
    /// Execute a PL/pgSQL function
    pub fn execute_function(
        &self,
        conn: &Connection,
        lua_code: &str,
        args: &[SqliteValue],
    ) -> Result<SqliteValue> {
        // Check cache first
        let cache_key = format!("{}", lua_code.len());
        {
            let cache = self.function_cache.lock().unwrap();
            if let Some(func) = cache.get(&cache_key) {
                return self.call_function(func, conn, args);
            }
        }
        
        // Compile and cache
        let func: LuaFunction = self.lua.load(lua_code).eval()?;
        {
            let mut cache = self.function_cache.lock().unwrap();
            cache.insert(cache_key.clone(), func.clone());
        }
        
        self.call_function(&func, conn, args)
    }
    
    fn call_function(
        &self,
        func: &LuaFunction,
        conn: &Connection,
        args: &[SqliteValue],
    ) -> Result<SqliteValue> {
        // Create execution context
        let ctx = ExecutionContext {
            conn: Arc::new(Mutex::new(conn.try_clone()?)),
            sqlstate: None,
            sqlerrm: None,
            trigger_data: None,
            row_count: 0,
            result_set: Vec::new(),
        };
        
        // Create API table
        let api = ctx.create_api_table(&self.lua)?;
        
        // Convert args to Lua values
        let lua_args: Vec<LuaValue> = args.iter()
            .map(|v| sqlite_to_lua(&self.lua, v.clone()))
            .collect::<Result<Vec<_>, _>>()?;
        
        // Store args for select() function
        self.lua.globals().set("_args", lua_args.clone())?;
        
        // Call function
        let result = func.call::<_, LuaValue>(api)?;
        
        // Convert result back to SQLite
        lua_to_sqlite(result).map_err(|e| anyhow::anyhow!("Lua error: {}", e))
    }
}
```

---

## 5. Built-in Functions & Special Variables

### 5.1 PL/pgSQL Built-in Functions

| Function | Lua Equivalent | Description |
|----------|----------------|-------------|
| `RAISE` | `_ctx.raise()` | Log messages at various levels |
| `RAISE EXCEPTION` | `_ctx.raise_exception()` | Raise an error |
| `PERFORM` | `_ctx.perform()` | Execute SQL, discard result |
| `EXECUTE` | `_ctx.execute()` | Dynamic SQL execution |
| `GET DIAGNOSTICS` | `_ctx` properties | Access execution info |
| `FOUND` | `_ctx.found` | True if query affected rows |
| `ROW_COUNT` | `_ctx.row_count` | Rows affected by last command |

### 5.2 Special Variables

| Variable | Lua Access | Description |
|----------|------------|-------------|
| `SQLSTATE` | `_ctx.SQLSTATE` | Current error code |
| `SQLERRM` | `_ctx.SQLERRM` | Current error message |

### 5.3 Trigger Special Variables

| Variable | Lua Access | Description |
|----------|------------|-------------|
| `TG_NAME` | `_ctx.TG_NAME` | Trigger name |
| `TG_WHEN` | `_ctx.TG_WHEN` | BEFORE/AFTER/INSTEAD OF |
| `TG_OP` | `_ctx.TG_OP` | INSERT/UPDATE/DELETE/TRUNCATE |
| `TG_LEVEL` | `_ctx.TG_LEVEL` | ROW/STATEMENT |
| `TG_RELID` | `_ctx.TG_RELID` | Table OID |
| `TG_RELNAME` | `_ctx.TG_RELNAME` | Table name |
| `TG_TABLE_NAME` | `_ctx.TG_TABLE_NAME` | Table name |
| `TG_TABLE_SCHEMA` | `_ctx.TG_TABLE_SCHEMA` | Schema name |
| `TG_NARGS` | `_ctx.TG_NARGS` | Number of trigger args |
| `TG_ARGV[]` | `_ctx.TG_ARGV[n]` | Trigger arguments |
| `NEW` | `_ctx.NEW` | New row (INSERT/UPDATE) |
| `OLD` | `_ctx.OLD` | Old row (UPDATE/DELETE) |

---

## 6. Exception Handling

### 6.1 SQLSTATE Error Code Mapping

Key SQLSTATE codes to support:

| Code | Condition Name | Description |
|------|----------------|-------------|
| 00000 | successful_completion | No error |
| 22012 | division_by_zero | Division by zero |
| 22003 | numeric_value_out_of_range | Numeric overflow |
| 23503 | foreign_key_violation | FK constraint violation |
| 23505 | unique_violation | Unique constraint violation |
| 23514 | check_violation | Check constraint violation |
| 25P02 | in_failed_sql_transaction | Transaction failed |
| 28P01 | invalid_password | Authentication failed |
| 3D000 | invalid_catalog_name | Database doesn't exist |
| 42703 | undefined_column | Column doesn't exist |
| 42883 | undefined_function | Function doesn't exist |
| 42P01 | undefined_table | Table doesn't exist |
| 42P07 | duplicate_table | Table already exists |
| P0001 | raise_exception | Generic PL/pgSQL exception |
| P0002 | no_data_found | Query returned no rows |
| P0003 | too_many_rows | Query returned multiple rows |

### 6.2 Exception Handling in Lua

```lua
-- Generated code for EXCEPTION block
local _ok, _err = pcall(function()
  -- Protected code
end)

if not _ok then
  local _sqlstate = _err.sqlstate or "P0001"
  local _sqlerrm = _err.message or tostring(_err)
  
  -- WHEN clauses
  if _sqlstate == "22012" then
    -- Handle division_by_zero
  elseif _sqlstate == "23505" then
    -- Handle unique_violation
  elseif _sqlstate == "P0002" then
    -- Handle no_data_found
  else
    -- WHEN OTHERS - re-raise
    error(_err)
  end
end
```

---

## 7. Trigger Support

### 7.1 Trigger Function Execution

```rust
impl PlPgSqlRuntime {
    /// Execute a trigger function
    pub fn execute_trigger(
        &self,
        conn: &Connection,
        lua_code: &str,
        trigger_data: TriggerData,
    ) -> Result<Option<HashMap<String, SqliteValue>>> {
        let ctx = ExecutionContext {
            conn: Arc::new(Mutex::new(conn.try_clone()?)),
            sqlstate: None,
            sqlerrm: None,
            trigger_data: Some(trigger_data),
            row_count: 0,
            result_set: Vec::new(),
        };
        
        let api = ctx.create_api_table(&self.lua)?;
        
        let func: LuaFunction = self.lua.load(lua_code).eval()?;
        let result = func.call::<_, LuaValue>(api)?;
        
        // Trigger functions return NEW, OLD, or NULL
        match result {
            LuaValue::Nil => Ok(None),
            LuaValue::Table(t) => {
                let mut row = HashMap::new();
                for pair in t.pairs::<String, LuaValue>() {
                    let (k, v) = pair?;
                    row.insert(k, lua_to_sqlite(v)?);
                }
                Ok(Some(row))
            }
            _ => Ok(None),
        }
    }
}
```

### 7.2 Trigger Variables Setup

When executing a trigger function, the runtime must populate:

1. **Context variables**: `TG_NAME`, `TG_WHEN`, `TG_OP`, `TG_LEVEL`, etc.
2. **Row data**: `NEW` and/or `OLD` as Lua tables
3. **Arguments**: `TG_ARGV` array

---

## 8. Type Mapping

### 8.1 PostgreSQL to SQLite Type Mapping

| PostgreSQL | SQLite | Lua | Notes |
|------------|--------|-----|-------|
| `INTEGER` | `INTEGER` | `number` | Direct mapping |
| `BIGINT` | `INTEGER` | `number` | Lua numbers are doubles |
| `SMALLINT` | `INTEGER` | `number` | |
| `SERIAL` | `INTEGER` | `number` | Auto-increment |
| `BIGSERIAL` | `INTEGER` | `number` | |
| `REAL` | `REAL` | `number` | |
| `DOUBLE PRECISION` | `REAL` | `number` | |
| `NUMERIC` | `REAL` | `number` | Precision loss possible |
| `DECIMAL` | `REAL` | `number` | |
| `TEXT` | `TEXT` | `string` | |
| `VARCHAR(n)` | `TEXT` | `string` | |
| `CHAR(n)` | `TEXT` | `string` | |
| `BOOLEAN` | `INTEGER` | `boolean` | 0/1 in SQLite |
| `DATE` | `TEXT` | `string` | ISO 8601 format |
| `TIMESTAMP` | `TEXT` | `string` | ISO 8601 format |
| `TIMESTAMPTZ` | `TEXT` | `string` | With timezone |
| `BYTEA` | `BLOB` | `string` | Binary data |
| `JSON` | `TEXT` | `string` | JSON as text |
| `JSONB` | `TEXT` | `string` | JSON as text |
| `ARRAY` | `TEXT` | `table` | JSON array |
| `NULL` | `NULL` | `nil` | |

### 8.2 Type Conversion Functions

```rust
/// Convert PostgreSQL OID to SQLite type hint
fn oid_to_sqlite_type(oid: i64) -> &'static str {
    match oid {
        16 => "INTEGER",    // bool
        20 => "INTEGER",    // int8
        21 => "INTEGER",    // int2
        23 => "INTEGER",    // int4
        25 => "TEXT",       // text
        700 => "REAL",      // float4
        701 => "REAL",      // float8
        1043 => "TEXT",     // varchar
        1114 => "TEXT",     // timestamp
        1184 => "TEXT",     // timestamptz
        _ => "TEXT",        // default
    }
}
```

---

## 9. Implementation Phases

### Phase 2A: Foundation (Week 1)

**Goal**: Basic parser and transpiler infrastructure

**Tasks**:
1. Add `pg_parse` and `mlua` dependencies to `Cargo.toml`
2. Create module structure:
   - `src/plpgsql/mod.rs`
   - `src/plpgsql/ast.rs`
   - `src/plpgsql/parser.rs`
   - `src/plpgsql/transpiler.rs`
   - `src/plpgsql/runtime.rs`
3. Implement AST types for core statements
4. Implement parser using `pg_parse::parse_plpgsql()`
5. Implement basic transpiler for:
   - Variable assignment
   - RETURN
   - RAISE
   - PERFORM
   - Simple SQL statements

**Deliverables**:
- Parser can parse simple PL/pgSQL functions
- Transpiler generates valid Lua for basic functions
- Unit tests for parser and transpiler

### Phase 2B: Control Flow (Week 2)

**Goal**: Complete control flow support

**Tasks**:
1. Implement remaining AST types:
   - IF/ELSIF/ELSE
   - LOOP/WHILE/FOR
   - EXIT/CONTINUE
   - CASE
2. Extend transpiler for all control flow
3. Implement Lua runtime with:
   - `_ctx.scalar()`
   - `_ctx.query()`
   - `_ctx.exec()`
   - `_ctx.raise()`
4. Add exception handling (pcall/xpcall)

**Deliverables**:
- Full control flow support
- Working runtime with PGQT API
- Integration tests

### Phase 2C: Advanced Features (Week 3)

**Goal**: Advanced PL/pgSQL features

**Tasks**:
1. Implement:
   - EXECUTE (dynamic SQL)
   - GET DIAGNOSTICS
   - RETURN NEXT / RETURN QUERY
   - Cursors (OPEN/FETCH/CLOSE)
2. Add special variables:
   - SQLSTATE
   - SQLERRM
   - FOUND
   - ROW_COUNT
3. Implement SQLSTATE error mapping
4. Add function overloading support

**Deliverables**:
- Advanced features working
- Error handling complete
- E2E tests

### Phase 2D: Trigger Support (Week 4)

**Goal**: Full trigger function support

**Tasks**:
1. Implement trigger variable setup
2. Add OLD/NEW row handling
3. Implement TG_* variables
4. Add CREATE TRIGGER parsing
5. Integrate with main.rs trigger handling

**Deliverables**:
- Trigger functions work
- CREATE TRIGGER support
- Trigger E2E tests

---

## 10. Testing Strategy

### 10.1 Unit Tests

```rust
// src/plpgsql/tests.rs

#[test]
fn test_parse_simple_function() {
    let sql = r#"
        CREATE FUNCTION add(a int, b int) RETURNS int AS $$
        BEGIN
            RETURN a + b;
        END;
        $$ LANGUAGE plpgsql;
    "#;
    
    let func = parse_plpgsql_function(sql).unwrap();
    assert_eq!(func.fn_name, Some("add".to_string()));
}

#[test]
fn test_transpile_assignment() {
    let func = PlpgsqlFunction {
        fn_name: Some("test".to_string()),
        fn_body: vec![PlPgSQLStmt::Assign(PlPgSQLStmtAssign {
            varname: "x".to_string(),
            expr: PlPgSQLExpr { query: "1 + 1".to_string(), parse_mode: Some(2) },
        })],
        ..Default::default()
    };
    
    let lua = transpile_to_lua(&func).unwrap();
    assert!(lua.contains("x = _ctx.scalar"));
}

#[test]
fn test_runtime_scalar() {
    let runtime = PlPgSqlRuntime::new().unwrap();
    let conn = Connection::open_in_memory().unwrap();
    
    let lua = r#"
        local function test(ctx, a, b)
            return ctx.scalar("SELECT $1 + $2", {a, b})
        end
        return test
    "#;
    
    let result = runtime.execute_function(&conn, lua, &[
        SqliteValue::Integer(5),
        SqliteValue::Integer(3),
    ]).unwrap();
    
    assert_eq!(result, SqliteValue::Integer(8));
}
```

### 10.2 Integration Tests

```rust
// tests/plpgsql_tests.rs

#[test]
fn test_plpgsql_if_statement() {
    let conn = setup_test_db();
    
    // Create function
    conn.execute(
        "CREATE FUNCTION max_val(a int, b int) RETURNS int LANGUAGE plpgsql AS $$
        BEGIN
            IF a > b THEN
                RETURN a;
            ELSE
                RETURN b;
            END IF;
        END;
        $$",
        [],
    ).unwrap();
    
    // Test execution
    let result: i64 = conn.query_row(
        "SELECT max_val(10, 5)",
        [],
        |row| row.get(0),
    ).unwrap();
    
    assert_eq!(result, 10);
}

#[test]
fn test_plpgsql_exception_handling() {
    let conn = setup_test_db();
    
    conn.execute(
        "CREATE FUNCTION safe_divide(a int, b int) RETURNS int LANGUAGE plpgsql AS $$
        BEGIN
            RETURN a / b;
        EXCEPTION
            WHEN division_by_zero THEN
                RETURN NULL;
        END;
        $$",
        [],
    ).unwrap();
    
    let result: Option<i64> = conn.query_row(
        "SELECT safe_divide(10, 0)",
        [],
        |row| row.get(0),
    ).unwrap();
    
    assert_eq!(result, None);
}
```

### 10.3 E2E Tests

```python
# tests/plpgsql_e2e_test.py

def test_plpgsql_basic_function():
    conn = psycopg2.connect(...)
    cur = conn.cursor()
    
    cur.execute("""
        CREATE FUNCTION greet(name text) RETURNS text LANGUAGE plpgsql AS $$
        BEGIN
            RETURN 'Hello, ' || name;
        END;
        $$
    """)
    
    cur.execute("SELECT greet('World')")
    result = cur.fetchone()[0]
    assert result == "Hello, World"
    print("test_plpgsql_basic_function: PASSED")

def test_plpgsql_loop():
    conn = psycopg2.connect(...)
    cur = conn.cursor()
    
    cur.execute("""
        CREATE FUNCTION factorial(n int) RETURNS int LANGUAGE plpgsql AS $$
        DECLARE
            result int := 1;
            i int;
        BEGIN
            FOR i IN 1..n LOOP
                result := result * i;
            END LOOP;
            RETURN result;
        END;
        $$
    """)
    
    cur.execute("SELECT factorial(5)")
    result = cur.fetchone()[0]
    assert result == 120
    print("test_plpgsql_loop: PASSED")

def test_plpgsql_trigger():
    conn = psycopg2.connect(...)
    cur = conn.cursor()
    
    cur.execute("""
        CREATE FUNCTION update_timestamp() RETURNS trigger LANGUAGE plpgsql AS $$
        BEGIN
            NEW.updated_at = NOW();
            RETURN NEW;
        END;
        $$
    """)
    
    cur.execute("""
        CREATE TRIGGER trg_update_timestamp
        BEFORE UPDATE ON users
        FOR EACH ROW
        EXECUTE FUNCTION update_timestamp()
    """)
    
    # Test trigger execution
    cur.execute("UPDATE users SET name = 'Updated' WHERE id = 1")
    
    cur.execute("SELECT updated_at FROM users WHERE id = 1")
    result = cur.fetchone()[0]
    assert result is not None
    print("test_plpgsql_trigger: PASSED")
```

---

## 11. Performance Considerations

### 11.1 Optimization Strategies

| Strategy | Implementation | Benefit |
|----------|----------------|---------|
| **Bytecode Caching** | Cache compiled Lua functions in `function_cache` | Avoid recompilation |
| **Prepared Statements** | Prepare SQL statements in `_ctx` methods | Faster execution |
| **Connection Pooling** | Reuse SQLite connections | Reduce connection overhead |
| **Lazy Transpilation** | Transpile on first call, cache result | Faster CREATE FUNCTION |
| **Inlined Scalar Functions** | Detect simple functions, inline SQL | Eliminate Lua overhead |

### 11.2 Benchmarking Targets

| Operation | Target | Notes |
|-----------|--------|-------|
| Simple scalar function | < 2x overhead vs SQL function | Lua call overhead |
| Loop with 1000 iterations | < 10ms | Control flow performance |
| Query in loop | Depends on query | Network/DB bound |
| Exception handling | < 1ms | pcall overhead |

---

## 12. Security Model

### 12.1 Sandboxing Strategy

```rust
impl PlPgSqlRuntime {
    fn create_sandbox(&self) -> Result<()> {
        let globals = self.lua.globals();
        
        // Remove dangerous functions
        globals.set("dofile", mlua::Nil)?;
        globals.set("loadfile", mlua::Nil)?;
        globals.set("require", mlua::Nil)?;
        
        // In Luau, use built-in sandbox
        #[cfg(feature = "luau")]
        self.lua.sandbox(true)?;
        
        // Set memory limit (if supported)
        // Set instruction count limit
        
        Ok(())
    }
}
```

### 12.2 Resource Limits

| Resource | Limit | Action on Exceed |
|----------|-------|------------------|
| Execution time | 30 seconds | Terminate Lua VM |
| Memory | 64 MB | Error |
| Instructions | 10 million | Error |
| Recursion depth | 100 | Error |
| Open cursors | 100 | Error |

### 12.3 SQL Injection Prevention

- All SQL parameters use parameterized queries
- `EXECUTE` with `USING` is safe
- String concatenation in SQL is blocked or warned

---

## Appendix A: Complete File Structure

```
src/
├── plpgsql/
│   ├── mod.rs           # Module exports
│   ├── ast.rs           # AST type definitions
│   ├── parser.rs        # pg_parse integration
│   ├── transpiler.rs    # AST to Lua conversion
│   ├── runtime