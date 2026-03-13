//! PL/pgSQL to Lua transpiler
//!
//! Converts PL/pgSQL AST into Lua source code for execution
//! in the mlua runtime environment.

// PL/pgSQL transpiler functions
#![allow(dead_code)]

use anyhow::Result;
use crate::plpgsql::ast::*;
use std::fmt::Write;

/// Transpile PL/pgSQL AST to Lua source code
pub fn transpile_to_lua(function: &PlpgsqlFunction) -> Result<String> {
    let mut ctx = TranspileContext::new(&function.datums);
    
    // Check if this is a SETOF function (uses RETURN NEXT)
    let is_setof = has_return_next(&function.action.block.body);
    
    // Generate function header
    ctx.emit_line("-- Generated from PL/pgSQL");
    let func_name = function.fn_name.as_deref().unwrap_or("anonymous");
    ctx.emit_line(&format!("local function {}(_ctx, ...)", func_name));
    ctx.indent();
    
    // Emit parameter declarations
    emit_parameters(&mut ctx, function)?;
    
    // Emit variable declarations (from DECLARE block)
    emit_variable_declarations(&mut ctx, function)?;
    
    // Initialize result set for SETOF functions
    if is_setof {
        ctx.emit_line("local _result_set = {}");
    }
    
    // Emit function body
    for stmt in &function.action.block.body {
        emit_statement(&mut ctx, stmt, is_setof)?;
    }
    
    // Return result set for SETOF functions
    if is_setof {
        ctx.emit_line("_result_set._is_result_set = true");
        ctx.emit_line("return _result_set");
    }
    
    ctx.dedent();
    ctx.emit_line("end");
    ctx.emit_line(&format!("return {}", func_name));
    
    Ok(ctx.output)
}

/// Emit variable declarations from DECLARE block
fn emit_variable_declarations(ctx: &mut TranspileContext, function: &PlpgsqlFunction) -> Result<()> {
    // Find where parameters end (usually before 'found' variable)
    let param_count = function.datums.iter()
        .position(|d| d.var_name.as_deref() == Some("found"))
        .unwrap_or(function.datums.len());
    
    // Emit declarations for variables after parameters (excluding 'found')
    let mut has_vars = false;
    for (i, datum) in function.datums.iter().enumerate() {
        // Skip parameters and 'found'
        if i < param_count {
            continue;
        }
        if datum.var_name.as_deref() == Some("found") {
            continue;
        }
        
        if let Some(var_name) = &datum.var_name {
            if !has_vars {
                ctx.emit_line("-- Local variables");
                has_vars = true;
            }
            
            // Initialize with default value if present
            if let Some(default) = &datum.default_val {
                let init_val = plpgsql_expr_to_lua(&default.query);
                ctx.emit_line(&format!("local {} = {}", var_name, init_val));
            } else {
                // Initialize to nil
                ctx.emit_line(&format!("local {} = nil", var_name));
            }
        }
    }
    
    Ok(())
}

/// Check if function body contains RETURN NEXT
fn has_return_next(stmts: &[PlPgSQLStmt]) -> bool {
    for stmt in stmts {
        match stmt {
            PlPgSQLStmt::ReturnNext(_) => return true,
            PlPgSQLStmt::Block(block) => {
                if has_return_next(&block.body) {
                    return true;
                }
            }
            PlPgSQLStmt::If(if_stmt) => {
                if has_return_next(&if_stmt.then_body) {
                    return true;
                }
                if let Some(ref elsif_list) = if_stmt.elsif_list {
                    for elsif in elsif_list {
                        if has_return_next(&elsif.stmts) {
                            return true;
                        }
                    }
                }
                if let Some(ref else_body) = if_stmt.else_body {
                    if has_return_next(else_body) {
                        return true;
                    }
                }
            }
            PlPgSQLStmt::Loop(loop_stmt) => {
                if has_return_next(&loop_stmt.body) {
                    return true;
                }
            }
            PlPgSQLStmt::While(while_stmt) => {
                if has_return_next(&while_stmt.body) {
                    return true;
                }
            }
            PlPgSQLStmt::ForI(for_i) => {
                if has_return_next(&for_i.body) {
                    return true;
                }
            }
            PlPgSQLStmt::ForS(for_s) => {
                if has_return_next(&for_s.body) {
                    return true;
                }
            }
            PlPgSQLStmt::Case(case) => {
                for when in &case.case_when_list {
                    if has_return_next(&when.stmts) {
                        return true;
                    }
                }
                if let Some(ref else_stmts) = case.else_stmts {
                    if has_return_next(else_stmts) {
                        return true;
                    }
                }
            }
            _ => {}
        }
    }
    false
}

/// Context for transpilation
struct TranspileContext<'a> {
    output: String,
    indent_level: usize,
    loop_depth: usize,
    /// Reference to function's datums for variable name lookup
    datums: &'a [PlPgSQLDatum],
}

impl<'a> TranspileContext<'a> {
    fn new(datums: &'a [PlPgSQLDatum]) -> Self {
        Self {
            output: String::new(),
            indent_level: 0,
            loop_depth: 0,
            datums,
        }
    }
    
    /// Look up variable name by datum index
    fn get_var_name(&self, index: i64) -> String {
        if index >= 0 && (index as usize) < self.datums.len() {
            self.datums[index as usize]
                .var_name
                .clone()
                .unwrap_or_else(|| format!("var_{}", index))
        } else {
            format!("var_{}", index)
        }
    }
    
    fn indent(&mut self) {
        self.indent_level += 1;
    }
    
    fn dedent(&mut self) {
        if self.indent_level > 0 {
            self.indent_level -= 1;
        }
    }
    
    fn emit_line(&mut self, line: &str) {
        let indent = "  ".repeat(self.indent_level);
        writeln!(self.output, "{}{}", indent, line).unwrap();
    }
    
    #[allow(dead_code)]
    fn emit(&mut self, text: &str) {
        self.output.push_str(text);
    }
}

/// Emit parameter handling
fn emit_parameters(ctx: &mut TranspileContext, function: &PlpgsqlFunction) -> Result<()> {
    // Parameters are stored in datums - emit them from the first N datums that are parameters
    // We can identify parameters by checking if they appear before 'found' (which is auto-added)
    
    // Find where parameters end (usually before 'found' variable)
    let param_count = function.datums.iter()
        .position(|d| d.var_name.as_deref() == Some("found"))
        .unwrap_or(function.datums.len());
    
    if param_count > 0 {
        ctx.emit_line("-- Parameters");
        for (i, datum) in function.datums.iter().take(param_count).enumerate() {
            if let Some(name) = &datum.var_name {
                ctx.emit_line(&format!("local {} = select({}, ...)", name, i + 1));
            }
        }
    }
    Ok(())
}

/// Emit a single statement
fn emit_statement(ctx: &mut TranspileContext, stmt: &PlPgSQLStmt, is_setof: bool) -> Result<()> {
    match stmt {
        PlPgSQLStmt::Block(block) => emit_block(ctx, block, is_setof)?,
        PlPgSQLStmt::Assign(assign) => emit_assign(ctx, assign)?,
        PlPgSQLStmt::If(if_stmt) => emit_if(ctx, if_stmt, is_setof)?,
        PlPgSQLStmt::Loop(loop_stmt) => emit_loop(ctx, loop_stmt, is_setof)?,
        PlPgSQLStmt::While(while_stmt) => emit_while(ctx, while_stmt, is_setof)?,
        PlPgSQLStmt::ForI(for_i) => emit_for_i(ctx, for_i, is_setof)?,
        PlPgSQLStmt::ForS(for_s) => emit_for_s(ctx, for_s, is_setof)?,
        PlPgSQLStmt::Exit(exit) => emit_exit(ctx, exit)?,
        PlPgSQLStmt::Return(ret) => emit_return(ctx, ret, is_setof)?,
        PlPgSQLStmt::ReturnNext(ret_next) => emit_return_next(ctx, ret_next)?,
        PlPgSQLStmt::Raise(raise) => emit_raise(ctx, raise)?,
        PlPgSQLStmt::ExecSql(exec) => emit_exec_sql(ctx, exec)?,
        PlPgSQLStmt::Perform(perform) => emit_perform(ctx, perform)?,
        PlPgSQLStmt::DynExecute(dyn_exec) => emit_dyn_execute(ctx, dyn_exec)?,
        PlPgSQLStmt::GetDiag(diag) => emit_get_diag(ctx, diag)?,
        PlPgSQLStmt::Case(case) => emit_case(ctx, case, is_setof)?,
        PlPgSQLStmt::Open(open) => emit_open(ctx, open)?,
        PlPgSQLStmt::Fetch(fetch) => emit_fetch(ctx, fetch)?,
        PlPgSQLStmt::Close(close) => emit_close(ctx, close)?,
        PlPgSQLStmt::Move(move_stmt) => emit_move(ctx, move_stmt)?,
    }
    Ok(())
}

/// Emit BEGIN/END block
fn emit_block(ctx: &mut TranspileContext, block: &PlPgSQLStmtBlock, is_setof: bool) -> Result<()> {
    if let Some(exceptions) = &block.exceptions {
        // Block with exception handler - use pcall
        ctx.emit_line("local _ok, _result_or_err = pcall(function()");
        ctx.indent();
        for stmt in &block.body {
            emit_statement(ctx, stmt, is_setof)?;
        }
        ctx.dedent();
        ctx.emit_line("end)");
        ctx.emit_line("if not _ok then");
        ctx.indent();
        ctx.emit_line("local _err = _result_or_err");
        ctx.emit_line("local _sqlstate = _err and _err.sqlstate or 'P0001'");
        ctx.emit_line("local _sqlerrm = _err and _err.message or tostring(_err)");
        ctx.emit_line("_ctx.SQLSTATE = _sqlstate");
        ctx.emit_line("_ctx.SQLERRM = _sqlerrm");
        
        // Emit WHEN clauses
        let exc_list = &exceptions.exc_list;
        for (i, exc) in exc_list.iter().enumerate() {
            let cond_name = exc.conditions.first()
                .map(|c| c.condname.as_str())
                .unwrap_or("OTHERS");
            let sqlstate = map_condition_to_sqlstate(cond_name);
            
            if i == 0 {
                ctx.emit_line(&format!("if _sqlstate == '{}' then", sqlstate));
            } else {
                ctx.emit_line(&format!("elseif _sqlstate == '{}' then", sqlstate));
            }
            ctx.indent();
            for stmt in &exc.action {
                emit_statement(ctx, stmt, is_setof)?;
            }
            ctx.dedent();
        }
        
        // Add OTHERS catch-all if not present
        if !exc_list.iter().any(|e| e.conditions.first().map(|c| c.condname == "OTHERS").unwrap_or(false)) {
            ctx.emit_line("else");
            ctx.indent();
            ctx.emit_line("error(_err)");
            ctx.dedent();
        }
        
        ctx.emit_line("end");  // Closes the if/elseif/else chain
        ctx.dedent();
        ctx.emit_line("else");
        ctx.indent();
        ctx.emit_line("return _result_or_err");  // Return the successful result
        ctx.dedent();
        ctx.emit_line("end");  // Closes the "if not _ok then"
    } else {
        // Regular block - just emit statements
        for stmt in &block.body {
            emit_statement(ctx, stmt, is_setof)?;
        }
    }
    Ok(())
}

/// Map PostgreSQL condition names to SQLSTATE codes
fn map_condition_to_sqlstate(condname: &str) -> String {
    match condname {
        "division_by_zero" => "22012",
        "unique_violation" => "23505",
        "foreign_key_violation" => "23503",
        "check_violation" => "23514",
        "not_null_violation" => "23502",
        "no_data_found" => "P0002",
        "too_many_rows" => "P0003",
        "raise_exception" => "P0001",
        "OTHERS" => "OTHERS",
        // Default to the condition name itself for custom errors
        other => other,
    }.to_string()
}

/// Emit variable assignment
fn emit_assign(ctx: &mut TranspileContext, assign: &PlPgSQLStmtAssign) -> Result<()> {
    // Get the variable name from datum index
    let varname = ctx.get_var_name(assign.varno);
    
    // The expression query may include the assignment itself (e.g., "i := i + 1")
    // We need to extract just the right-hand side
    let expr_query = &assign.expr.query;
    
    // Check if this is an assignment to a record field (e.g., "NEW.column = value")
    // In this case, the varname will be something like "var_3" (unnamed datum)
    // and the expr_query will contain the full assignment "NEW.column = value"
    if varname.starts_with("var_") && expr_query.contains('=') {
        // Parse the expression to extract target and value
        // Format: "NEW.column = value" or "NEW.column := value"
        if let Some(eq_pos) = expr_query.find('=') {
            let target = expr_query[..eq_pos].trim();
            let rhs = expr_query[eq_pos + 1..].trim();
            
            // Check if target looks like a record field access (NEW.column or OLD.column)
            if target.starts_with("NEW.") || target.starts_with("OLD.") {
                // Transpile the right-hand side
                let expr_lua = plpgsql_expr_to_lua(rhs);
                
                // Generate: NEW["column"] = value (or NEW.column = value)
                ctx.emit_line(&format!("{} = {}", target, expr_lua));
                return Ok(());
            }
        }
    }
    
    // Check if the query contains := (assignment operator)
    let rhs = if let Some(pos) = expr_query.find(":=") {
        // Extract the right-hand side after :=
        expr_query[pos + 2..].trim()
    } else {
        expr_query.trim()
    };
    
    // Transpile the right-hand side expression
    let expr_lua = plpgsql_expr_to_lua(rhs);
    ctx.emit_line(&format!("{} = {}", varname, expr_lua));
    Ok(())
}

/// Emit IF statement
fn emit_if(ctx: &mut TranspileContext, if_stmt: &PlPgSQLStmtIf, is_setof: bool) -> Result<()> {
    let cond = transpile_expr(&if_stmt.cond)?;
    ctx.emit_line(&format!("if {} then", cond));
    ctx.indent();
    for stmt in &if_stmt.then_body {
        emit_statement(ctx, stmt, is_setof)?;
    }
    ctx.dedent();
    
    // ELSIF branches
    if let Some(elsif_list) = &if_stmt.elsif_list {
        for elsif in elsif_list {
            let elsif_cond = transpile_expr(&elsif.cond)?;
            ctx.emit_line(&format!("elseif {} then", elsif_cond));
            ctx.indent();
            for stmt in &elsif.stmts {
                emit_statement(ctx, stmt, is_setof)?;
            }
            ctx.dedent();
        }
    }
    
    // ELSE branch
    if let Some(else_body) = &if_stmt.else_body {
        ctx.emit_line("else");
        ctx.indent();
        for stmt in else_body {
            emit_statement(ctx, stmt, is_setof)?;
        }
        ctx.dedent();
    }
    
    ctx.emit_line("end");
    Ok(())
}

/// Emit LOOP statement
fn emit_loop(ctx: &mut TranspileContext, loop_stmt: &PlPgSQLStmtLoop, is_setof: bool) -> Result<()> {
    ctx.loop_depth += 1;
    ctx.emit_line("while true do");
    ctx.indent();
    for stmt in &loop_stmt.body {
        emit_statement(ctx, stmt, is_setof)?;
    }
    ctx.dedent();
    ctx.emit_line("end");
    ctx.loop_depth -= 1;
    Ok(())
}

/// Emit WHILE loop
fn emit_while(ctx: &mut TranspileContext, while_stmt: &PlPgSQLStmtWhile, is_setof: bool) -> Result<()> {
    let cond = transpile_expr(&while_stmt.cond)?;
    ctx.emit_line(&format!("while {} do", cond));
    ctx.indent();
    for stmt in &while_stmt.body {
        emit_statement(ctx, stmt, is_setof)?;
    }
    ctx.dedent();
    ctx.emit_line("end");
    Ok(())
}

/// Emit FOR i IN start..end loop
fn emit_for_i(ctx: &mut TranspileContext, for_i: &PlPgSQLStmtForI, is_setof: bool) -> Result<()> {
    let lower = transpile_expr(&for_i.lower)?;
    let upper = transpile_expr(&for_i.upper)?;
    let var = &for_i.varname;
    
    if for_i.reverse {
        ctx.emit_line(&format!("for {} = {}, {}, -1 do", var, upper, lower));
    } else {
        ctx.emit_line(&format!("for {} = {}, {} do", var, lower, upper));
    }
    
    // Set loop variable for implicit RETURN NEXT
    ctx.emit_line(&format!("_loop_var = {}", var));
    
    ctx.indent();
    for stmt in &for_i.body {
        emit_statement(ctx, stmt, is_setof)?;
    }
    ctx.dedent();
    ctx.emit_line("end");
    Ok(())
}

/// Emit FOR row IN SELECT loop
fn emit_for_s(ctx: &mut TranspileContext, for_s: &PlPgSQLStmtForS, is_setof: bool) -> Result<()> {
    let query = &for_s.query.query;
    let var = &for_s.varname;
    ctx.emit_line(&format!("for {} in _ctx.query_iter([[{}]], {{}}) do", var, query));
    ctx.indent();
    for stmt in &for_s.body {
        emit_statement(ctx, stmt, is_setof)?;
    }
    ctx.dedent();
    ctx.emit_line("end");
    Ok(())
}

/// Emit EXIT statement
fn emit_exit(ctx: &mut TranspileContext, exit: &PlPgSQLStmtExit) -> Result<()> {
    if let Some(cond) = &exit.cond {
        let cond_lua = transpile_expr(cond)?;
        ctx.emit_line(&format!("if {} then break end", cond_lua));
    } else {
        ctx.emit_line("break");
    }
    Ok(())
}

/// Emit RETURN statement
fn emit_return(ctx: &mut TranspileContext, ret: &PlPgSQLStmtReturn, is_setof: bool) -> Result<()> {
    if let Some(expr) = &ret.expr {
        let expr_lua = transpile_expr(expr)?;
        if is_setof {
            // For SETOF functions, add to result set
            ctx.emit_line("table.insert(_result_set, {})");
        } else {
            ctx.emit_line(&format!("return {}", expr_lua));
        }
    } else if !is_setof {
        ctx.emit_line("return");
    }
    Ok(())
}

/// Emit RETURN NEXT statement
fn emit_return_next(ctx: &mut TranspileContext, ret_next: &PlPgSQLStmtReturnNext) -> Result<()> {
    // expr may be None when inside a FOR loop - loop variable is implicit
    if let Some(expr) = &ret_next.expr {
        let expr_lua = transpile_expr(expr)?;
        // Accumulate result in a table
        ctx.emit_line("if _result_set == nil then _result_set = {} end");
        ctx.emit_line(&format!("table.insert(_result_set, {})", expr_lua));
    } else {
        // No explicit expression - use the loop variable (i in FOR i IN ...)
        // This is handled by the FOR loop context
        ctx.emit_line("if _result_set == nil then _result_set = {} end");
        ctx.emit_line("table.insert(_result_set, _loop_var)");
    }
    Ok(())
}

/// Emit RAISE statement
fn emit_raise(ctx: &mut TranspileContext, raise: &PlPgSQLStmtRaise) -> Result<()> {
    let level = &raise.elog_level;
    let message = raise.message.as_deref().unwrap_or("");
    
    if level == "EXCEPTION" {
        // Extract ERRCODE if present
        let mut errcode = "P0001".to_string();
        if let Some(options) = &raise.options {
            for opt in options {
                if opt.opt_type == "ERRCODE" {
                    errcode = opt.expr.query.clone();
                }
            }
        }
        
        if let Some(params) = &raise.params {
            let param_list: Vec<String> = params.iter()
                .map(|p| plpgsql_expr_to_lua(&p.query))
                .collect();
            if param_list.is_empty() {
                ctx.emit_line(&format!("_ctx.raise_exception(\"{}\", {{errcode = \"{}\"}})", message, errcode));
            } else {
                ctx.emit_line(&format!("_ctx.raise_exception(\"{}\", {}, {{errcode = \"{}\"}})", message, param_list.join(", "), errcode));
            }
        } else {
            ctx.emit_line(&format!("_ctx.raise_exception(\"{}\", {{errcode = \"{}\"}})", message, errcode));
        }
    } else {
        // DEBUG, INFO, NOTICE, WARNING
        if let Some(params) = &raise.params {
            let param_list: Vec<String> = params.iter()
                .map(|p| plpgsql_expr_to_lua(&p.query))
                .collect();
            if param_list.is_empty() {
                ctx.emit_line(&format!("_ctx.raise(\"{}\", \"{}\")", level, message));
            } else {
                // Pass params as a Lua table
                ctx.emit_line(&format!("_ctx.raise(\"{}\", \"{}\", {{ {} }})", level, message, param_list.join(", ")));
            }
        } else {
            ctx.emit_line(&format!("_ctx.raise(\"{}\", \"{}\")", level, message));
        }
    }
    
    Ok(())
}

/// Emit EXEC SQL statement
fn emit_exec_sql(ctx: &mut TranspileContext, exec: &PlPgSQLStmtExecSql) -> Result<()> {
    let query = &exec.sqlstmt.query;
    if exec.into {
        // INTO clause - capture result into variable
        if let Some(target) = &exec.target {
            ctx.emit_line(&format!("{} = _ctx.scalar([[{}]], {{}})", target.name, query));
        } else {
            ctx.emit_line(&format!("_ctx.exec([[{}]], {{}})", query));
        }
    } else {
        ctx.emit_line(&format!("_ctx.exec([[{}]], {{}})", query));
    }
    Ok(())
}

/// Emit PERFORM statement
fn emit_perform(ctx: &mut TranspileContext, perform: &PlPgSQLStmtPerform) -> Result<()> {
    let query = &perform.expr.query;
    ctx.emit_line(&format!("_ctx.perform([[{}]], {{}})", query));
    Ok(())
}

/// Emit EXECUTE dynamic SQL
fn emit_dyn_execute(ctx: &mut TranspileContext, dyn_exec: &PlPgSQLStmtDynExecute) -> Result<()> {
    let query = &dyn_exec.query.query;
    ctx.emit_line(&format!("_ctx.execute([[{}]], {{}})", query));
    Ok(())
}

/// Emit GET DIAGNOSTICS
fn emit_get_diag(ctx: &mut TranspileContext, diag: &PlPgSQLStmtGetDiag) -> Result<()> {
    // Map diagnostic items to context properties
    // PostgreSQL diagnostic item kinds are strings like "ROW_COUNT", "RESULT_OID", etc.
    for item in &diag.diag_items {
        let value = match item.kind.as_str() {
            "ROW_COUNT" => "_ctx.ROW_COUNT",
            "RESULT_OID" => "_ctx.RESULT_OID or nil",
            "RETURNED_SQLSTATE" => "_ctx.SQLSTATE or '00000'",
            "MESSAGE_TEXT" => "_ctx.SQLERRM or ''",
            "PG_EXCEPTION_CONTEXT" => "_ctx.PG_CONTEXT or ''",
            "PG_CONTEXT" => "_ctx.PG_CONTEXT or ''",
            "CONSTRAINT_NAME" => "_ctx.constraint_name",
            "SCHEMA_NAME" => "_ctx.schema_name",
            "TABLE_NAME" => "_ctx.table_name",
            "COLUMN_NAME" => "_ctx.column_name",
            "DATATYPE_NAME" => "_ctx.datatype_name",
            _ => "nil",
        };
        // target is the datum index for the variable
        let var_name = ctx.get_var_name(item.target);
        ctx.emit_line(&format!("{} = {}", var_name, value));
    }
    Ok(())
}

/// Emit CASE statement
fn emit_case(ctx: &mut TranspileContext, case: &PlPgSQLStmtCase, is_setof: bool) -> Result<()> {
    let mut first = true;

    for when in &case.case_when_list {
        let cond = transpile_expr(&when.expr)?;
        if first {
            ctx.emit_line(&format!("if {} then", cond));
            first = false;
        } else {
            ctx.emit_line(&format!("elseif {} then", cond));
        }
        ctx.indent();
        for stmt in &when.stmts {
            emit_statement(ctx, stmt, is_setof)?;
        }
        ctx.dedent();
    }

    if let Some(else_stmts) = &case.else_stmts {
        ctx.emit_line("else");
        ctx.indent();
        for stmt in else_stmts {
            emit_statement(ctx, stmt, is_setof)?;
        }
        ctx.dedent();
    }

    ctx.emit_line("end");
    Ok(())
}

/// Emit OPEN cursor statement
fn emit_open(ctx: &mut TranspileContext, open: &PlPgSQLStmtOpen) -> Result<()> {
    let cursor_name = &open.cursorname;
    if let Some(query) = &open.query {
        ctx.emit_line(&format!("_ctx.cursor_open([[{}]], [[{}]])", cursor_name, query.query));
    } else {
        ctx.emit_line(&format!("_ctx.cursor_open(nil, [[{}]])", cursor_name));
    }
    Ok(())
}

/// Emit FETCH cursor statement
fn emit_fetch(ctx: &mut TranspileContext, fetch: &PlPgSQLStmtFetch) -> Result<()> {
    let cursor_name = &fetch.cursorname;
    if let Some(target) = &fetch.target {
        let direction = fetch.direction.as_deref().unwrap_or("FORWARD");
        let count = fetch.count.unwrap_or(1);
        ctx.emit_line(&format!(
            "{} = _ctx.cursor_fetch([[{}]], \"{}\", {})",
            target.name, cursor_name, direction, count
        ));
    } else {
        ctx.emit_line(&format!("_ctx.cursor_fetch([[{}]], \"FORWARD\", 1)", cursor_name));
    }
    Ok(())
}

/// Emit CLOSE cursor statement
fn emit_close(ctx: &mut TranspileContext, close: &PlPgSQLStmtClose) -> Result<()> {
    let cursor_name = &close.cursorname;
    ctx.emit_line(&format!("_ctx.cursor_close([[{}]])", cursor_name));
    Ok(())
}

/// Emit MOVE cursor statement
fn emit_move(ctx: &mut TranspileContext, move_stmt: &PlPgSQLStmtMove) -> Result<()> {
    let cursor_name = &move_stmt.cursorname;
    let direction = move_stmt.direction.as_deref().unwrap_or("FORWARD");
    let count = move_stmt.count.unwrap_or(1);
    ctx.emit_line(&format!(
        "_ctx.cursor_move([[{}]], \"{}\", {})",
        cursor_name, direction, count
    ));
    Ok(())
}

/// Transpile a PL/pgSQL expression to Lua
/// 
/// Distinguishes between:
/// 1. Simple expressions (arithmetic, comparisons, string ops) - direct Lua
/// 2. SQL queries (SELECT, INSERT, etc.) - execute via _ctx.scalar()
fn transpile_expr(expr: &PlPgSQLExpr) -> Result<String> {
    let query = expr.query.trim();
    
    // Check if this looks like a SQL query
    let is_sql_query = query.to_uppercase().starts_with("SELECT ")
        || query.to_uppercase().starts_with("INSERT ")
        || query.to_uppercase().starts_with("UPDATE ")
        || query.to_uppercase().starts_with("DELETE ")
        || query.to_uppercase().starts_with("WITH ")
        || query.to_uppercase().starts_with("CREATE ")
        || query.to_uppercase().starts_with("DROP ")
        || query.to_uppercase().starts_with("ALTER ");
    
    if is_sql_query {
        // SQL query - execute via runtime
        Ok(format!("_ctx.scalar([[{}]], {{}})", query))
    } else {
        // Simple expression - convert to Lua
        Ok(plpgsql_expr_to_lua(query))
    }
}

/// Convert a PL/pgSQL expression to Lua
fn plpgsql_expr_to_lua(expr: &str) -> String {
    let mut result = expr.to_string();
    
    // Map PostgreSQL built-in functions to Lua equivalents
    result = map_postgres_functions(&result);
    
    // Convert := to = (assignment operator)
    result = result.replace(":=", "=");
    
    // Convert PostgreSQL comparisons (=, <>, !=) to Lua (==, ~=)
    result = convert_comparisons_to_lua(&result);
    
    // Lowercase NEW.field and OLD.field access
    result = lowercase_record_access(&result);
    
    // Convert PostgreSQL string concatenation (||) to Lua (..)
    // Be careful not to convert inside string literals
    let mut in_string = false;
    let mut string_char = ' ';
    let chars: Vec<char> = result.chars().collect();
    let mut new_result = String::new();
    let mut i = 0;
    
    while i < chars.len() {
        let c = chars[i];
        
        // Track string literals
        if (c == '\'' || c == '"') && (i == 0 || chars[i-1] != '\\') {
            if !in_string {
                in_string = true;
                string_char = c;
            } else if c == string_char {
                in_string = false;
            }
            new_result.push(c);
        } else if !in_string && c == '|' && i + 1 < chars.len() && chars[i + 1] == '|' {
            // Convert || to safe concatenation
            new_result.push_str("..");
            i += 1; // Skip next |
        } else {
            new_result.push(c);
        }
        i += 1;
    }
    
    result = new_result;
    
    // Wrap division operations with zero check
    // This MUST be the last step to avoid its own = being converted to ==
    result = wrap_division_with_zero_check(&result);
    
    result
}

/// Lowercase NEW.field and OLD.field access in PL/pgSQL expressions
fn lowercase_record_access(expr: &str) -> String {
    let mut result = String::new();
    let chars: Vec<char> = expr.chars().collect();
    let mut i = 0;
    let mut in_string = false;
    let mut string_char = ' ';
    
    while i < chars.len() {
        let c = chars[i];
        
        // Track string literals
        if (c == '\'' || c == '"') && (i == 0 || chars[i-1] != '\\') {
            if !in_string {
                in_string = true;
                string_char = c;
            } else if c == string_char {
                in_string = false;
            }
            result.push(c);
        } else if !in_string {
            // Look for NEW. or OLD. (case insensitive)
            let remaining = &expr[i..];
            if remaining.to_uppercase().starts_with("NEW.") || remaining.to_uppercase().starts_with("OLD.") {
                // Push "NEW." or "OLD."
                result.push_str(&remaining[..4]);
                i += 4;
                
                // Lowercase the following identifier
                while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                    result.push(chars[i].to_ascii_lowercase());
                    i += 1;
                }
                // Decrement i because the loop will increment it
                i -= 1;
            } else {
                result.push(c);
            }
        } else {
            result.push(c);
        }
        i += 1;
    }
    result
}

/// Convert PostgreSQL comparisons to Lua equivalents
fn convert_comparisons_to_lua(expr: &str) -> String {
    let mut result = String::new();
    let chars: Vec<char> = expr.chars().collect();
    let mut i = 0;
    let mut in_string = false;
    let mut string_char = ' ';
    
    while i < chars.len() {
        let c = chars[i];
        
        // Track string literals
        if (c == '\'' || c == '"') && (i == 0 || chars[i-1] != '\\') {
            if !in_string {
                in_string = true;
                string_char = c;
            } else if c == string_char {
                in_string = false;
            }
            result.push(c);
        } else if !in_string {
            // Check for operators
            if c == '=' {
                // If it's not followed by another =, and not preceded by <, >, !, :
                let prev = if i > 0 { Some(chars[i-1]) } else { None };
                let next = if i + 1 < chars.len() { Some(chars[i+1]) } else { None };
                
                if next == Some('=') {
                    // Already ==
                    result.push_str("==");
                    i += 1;
                } else if matches!(prev, Some('<') | Some('>') | Some('!') | Some(':')) {
                    // Part of <=, >=, !=, :=
                    result.push('=');
                } else {
                    // Single = -> convert to ==
                    result.push_str("==");
                }
            } else if c == '<' && i + 1 < chars.len() && chars[i+1] == '>' {
                // <> -> ~=
                result.push_str("~=");
                i += 1;
            } else if c == '!' && i + 1 < chars.len() && chars[i+1] == '=' {
                // != -> ~=
                result.push_str("~=");
                i += 1;
            } else {
                result.push(c);
            }
        } else {
            result.push(c);
        }
        i += 1;
    }
    result
}

/// Map PostgreSQL built-in functions to Lua equivalents
/// 
/// This function replaces PostgreSQL function calls with their Lua runtime equivalents.
/// The Lua runtime provides these functions via the _ctx table.
fn map_postgres_functions(expr: &str) -> String {
    let mut result = expr.to_string();
    
    // Define function mappings: PostgreSQL function -> Lua equivalent
    // These functions are provided by the Lua runtime in _ctx
    let function_mappings: Vec<(&str, &str)> = vec![
        ("NOW()", "_ctx.now()"),
        ("CURRENT_TIMESTAMP", "_ctx.now()"),
        ("CURRENT_DATE", "_ctx.current_date()"),
        ("CURRENT_TIME", "_ctx.current_time()"),
        ("COALESCE(", "_ctx.coalesce("),
        ("NULLIF(", "_ctx.nullif("),
        ("LOWER(", "_ctx.lower("),
        ("UPPER(", "_ctx.upper("),
        ("LENGTH(", "_ctx.length("),
        ("ABS(", "_ctx.abs("),
        ("ROUND(", "_ctx.round("),
        ("CEIL(", "_ctx.ceil("),
        ("FLOOR(", "_ctx.floor("),
        ("REPLACE(", "_ctx.replace("),
    ];
    
    // Apply mappings (case-insensitive)
    for (pg_func, lua_func) in function_mappings {
        // Match both uppercase and lowercase versions
        let upper_func = pg_func.to_uppercase();
        let lower_func = pg_func.to_lowercase();
        
        // Try uppercase first
        let new_result = result.replace(&upper_func, lua_func);
        
        // Only do lowercase replacement if uppercase didn't match
        // This prevents double-conversion (e.g., NOW() -> _ctx.now() -> _ctx._ctx.now())
        if new_result == result && upper_func != lower_func {
            result = result.replace(&lower_func, lua_func);
        } else {
            result = new_result;
        }
    }
    
    result
}

/// Wrap division operations with zero check to match PostgreSQL semantics
fn wrap_division_with_zero_check(expr: &str) -> String {
    let mut result = String::new();
    let chars: Vec<char> = expr.chars().collect();
    let mut i = 0;
    let mut in_string = false;
    let mut string_char = ' ';
    
    while i < chars.len() {
        let c = chars[i];
        
        // Track string literals
        if (c == '\'' || c == '"') && (i == 0 || chars[i-1] != '\\') {
            if !in_string {
                in_string = true;
                string_char = c;
            } else if c == string_char {
                in_string = false;
            }
            result.push(c);
        } else if !in_string && c == '/' {
            // Found a division operator - we need to find the divisor
            // For simplicity, just wrap the whole expression in a helper call
            // This is a compromise - proper solution would parse the full expression
            result.push(c);
        } else {
            result.push(c);
        }
        i += 1;
    }
    
    // Check if there's a division in the expression
    if expr.contains('/') && !in_string {
        // Wrap the entire expression with a division-by-zero check
        // This is a simplified approach: use pcall to catch the "inf" result
        format!("(function() local _r = {}; if _r == math.huge or _r == -math.huge then error({{sqlstate='22012', message='division by zero'}}) end return _r end)()", result)
    } else {
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plpgsql::parser::parse_plpgsql_function;

    #[test]
    fn test_transpile_simple_function() {
        let sql = r#"
            CREATE FUNCTION add(a int, b int) RETURNS int AS $$
            BEGIN
                RETURN a + b;
            END;
            $$ LANGUAGE plpgsql;
        "#;
        
        let func = parse_plpgsql_function(sql).unwrap();
        let lua = transpile_to_lua(&func).unwrap();
        
        assert!(lua.contains("local function add(_ctx, ...)"));
        assert!(lua.contains("return add"));
    }

    #[test]
    fn test_transpile_with_if() {
        let sql = r#"
            CREATE FUNCTION max_val(a int, b int) RETURNS int AS $$
            BEGIN
                IF a > b THEN
                    RETURN a;
                ELSE
                    RETURN b;
                END IF;
            END;
            $$ LANGUAGE plpgsql;
        "#;
        
        let func = parse_plpgsql_function(sql).unwrap();
        let lua = transpile_to_lua(&func).unwrap();
        
        assert!(lua.contains("if"));
        assert!(lua.contains("then"));
        assert!(lua.contains("else"));
        assert!(lua.contains("end"));
    }

    #[test]
    fn test_transpile_with_loop() {
        let sql = r#"
            CREATE FUNCTION test_loop() RETURNS int AS $$
            DECLARE
                i int := 0;
            BEGIN
                WHILE i < 10 LOOP
                    i := i + 1;
                END LOOP;
                RETURN i;
            END;
            $$ LANGUAGE plpgsql;
        "#;
        
        let func = parse_plpgsql_function(sql).unwrap();
        let lua = transpile_to_lua(&func).unwrap();
        
        assert!(lua.contains("while"));
        assert!(lua.contains("do"));
        assert!(lua.contains("end"));
    }

    #[test]
    fn test_transpile_raise() {
        let sql = r#"
            CREATE FUNCTION log_test() RETURNS void AS $$
            BEGIN
                RAISE NOTICE 'Test message';
            END;
            $$ LANGUAGE plpgsql;
        "#;
        
        let func = parse_plpgsql_function(sql).unwrap();
        let lua = transpile_to_lua(&func).unwrap();
        
        assert!(lua.contains("_ctx.raise"));
        assert!(lua.contains("NOTICE"));
    }

    #[test]
    fn test_transpile_perform() {
        let sql = r#"
            CREATE FUNCTION do_something() RETURNS void AS $$
            BEGIN
                PERFORM some_func();
            END;
            $$ LANGUAGE plpgsql;
        "#;
        
        let func = parse_plpgsql_function(sql).unwrap();
        let lua = transpile_to_lua(&func).unwrap();
        
        assert!(lua.contains("_ctx.perform"));
    }

    #[test]
    fn test_transpile_new_column_assignment() {
        // Test that NEW.column assignments generate proper Lua code
        let sql = r#"
            CREATE FUNCTION set_timestamp() RETURNS TRIGGER AS $$
            BEGIN
                NEW.created_at = '2024-01-01';
                RETURN NEW;
            END;
            $$ LANGUAGE plpgsql;
        "#;
        
        let func = parse_plpgsql_function(sql).unwrap();
        let lua = transpile_to_lua(&func).unwrap();
        
        // The generated Lua should set the NEW table's column directly
        // Should be: NEW.created_at = '2024-01-01'
        // NOT: var_x = NEW.created_at = '2024-01-01' (which is invalid Lua)
        assert!(lua.contains("NEW.created_at = '2024-01-01'"));
        // Make sure we don't have the invalid chained assignment
        assert!(!lua.contains("var_3 = NEW.created_at"));
    }
    
    #[test]
    fn test_transpile_old_column_assignment() {
        // Test that OLD.column assignments also work
        let sql = r#"
            CREATE FUNCTION audit_delete() RETURNS TRIGGER AS $$
            BEGIN
                OLD.deleted_at = '2024-01-01';
                RETURN OLD;
            END;
            $$ LANGUAGE plpgsql;
        "#;
        
        let func = parse_plpgsql_function(sql).unwrap();
        let lua = transpile_to_lua(&func).unwrap();
        
        // Should generate: OLD.deleted_at = '2024-01-01'
        assert!(lua.contains("OLD.deleted_at = '2024-01-01'"));
    }
    
    #[test]
    fn test_transpile_regular_variable_assignment() {
        // Test that regular variable assignments still work
        let sql = r#"
            CREATE FUNCTION test_assign() RETURNS int AS $$
            DECLARE
                x int := 5;
            BEGIN
                x := x + 1;
                RETURN x;
            END;
            $$ LANGUAGE plpgsql;
        "#;
        
        let func = parse_plpgsql_function(sql).unwrap();
        let lua = transpile_to_lua(&func).unwrap();
        
        // Regular variables should still use the datum name
        assert!(lua.contains("x = x + 1"));
    }

    #[test]
    fn test_transpile_now_function() {
        // Test that NOW() is mapped to _ctx.now()
        let sql = r#"
            CREATE FUNCTION get_timestamp() RETURNS TEXT AS $$
            BEGIN
                RETURN NOW();
            END;
            $$ LANGUAGE plpgsql;
        "#;
        
        let func = parse_plpgsql_function(sql).unwrap();
        let lua = transpile_to_lua(&func).unwrap();
        
        // Should use _ctx.now() instead of NOW()
        assert!(lua.contains("_ctx.now()"));
        assert!(!lua.contains("NOW()"));
    }

    #[test]
    fn test_transpile_current_timestamp() {
        // Test that CURRENT_TIMESTAMP is mapped to _ctx.now()
        let sql = r#"
            CREATE FUNCTION get_timestamp() RETURNS TEXT AS $$
            BEGIN
                RETURN CURRENT_TIMESTAMP;
            END;
            $$ LANGUAGE plpgsql;
        "#;
        
        let func = parse_plpgsql_function(sql).unwrap();
        let lua = transpile_to_lua(&func).unwrap();
        
        // Should use _ctx.now() instead of CURRENT_TIMESTAMP
        assert!(lua.contains("_ctx.now()"));
        assert!(!lua.contains("CURRENT_TIMESTAMP"));
    }

    #[test]
    fn test_transpile_coalesce() {
        // Test that COALESCE is mapped to _ctx.coalesce()
        let sql = r#"
            CREATE FUNCTION safe_value(a int, b int) RETURNS int AS $$
            BEGIN
                RETURN COALESCE(a, b);
            END;
            $$ LANGUAGE plpgsql;
        "#;
        
        let func = parse_plpgsql_function(sql).unwrap();
        let lua = transpile_to_lua(&func).unwrap();
        
        // Should use _ctx.coalesce() instead of COALESCE
        assert!(lua.contains("_ctx.coalesce"));
        assert!(!lua.contains("COALESCE("));
    }
}
