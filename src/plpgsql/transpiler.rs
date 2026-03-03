//! PL/pgSQL to Lua transpiler
//!
//! Converts PL/pgSQL AST into Lua source code for execution
//! in the mlua runtime environment.

use anyhow::Result;
use crate::plpgsql::ast::*;
use std::fmt::Write;

/// Transpile PL/pgSQL AST to Lua source code
pub fn transpile_to_lua(function: &PlpgsqlFunction) -> Result<String> {
    let mut ctx = TranspileContext::new();
    
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
    ctx.emit_line("-- Variable declarations");
    
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
struct TranspileContext {
    output: String,
    indent_level: usize,
    loop_depth: usize,
}

impl TranspileContext {
    fn new() -> Self {
        Self {
            output: String::new(),
            indent_level: 0,
            loop_depth: 0,
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
    if let Some(argnames) = &function.fn_argnames {
        ctx.emit_line("-- Parameters");
        for (i, name) in argnames.iter().enumerate() {
            ctx.emit_line(&format!("local {} = select({}, ...)", name, i + 1));
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
        ctx.emit_line("local _ok, _err = pcall(function()");
        ctx.indent();
        for stmt in &block.body {
            emit_statement(ctx, stmt, is_setof)?;
        }
        ctx.dedent();
        ctx.emit_line("end)");
        ctx.emit_line("if not _ok then");
        ctx.indent();
        ctx.emit_line("local _sqlstate = _err and _err.sqlstate or 'P0001'");
        ctx.emit_line("local _sqlerrm = _err and _err.message or tostring(_err)");
        ctx.emit_line("_ctx.SQLSTATE = _sqlstate");
        ctx.emit_line("_ctx.SQLERRM = _sqlerrm");
        
        // Emit WHEN clauses
        for (i, exc) in exceptions.iter().enumerate() {
            let sqlstate = &exc.sqlstate;
            if i == 0 {
                ctx.emit_line(&format!("if _sqlstate == '{}' then", sqlstate));
            } else {
                ctx.emit_line(&format!("elseif _sqlstate == '{}' then", sqlstate));
            }
            ctx.indent();
            for stmt in &exc.stmts {
                emit_statement(ctx, stmt, is_setof)?;
            }
            ctx.dedent();
        }
        
        // Add OTHERS catch-all if not present
        if !exceptions.iter().any(|e| e.sqlstate == "OTHERS") {
            ctx.emit_line("else");
            ctx.indent();
            ctx.emit_line("error(_err)");
            ctx.dedent();
        }
        
        ctx.emit_line("end");
        ctx.dedent();
        ctx.emit_line("end");
    } else {
        // Regular block - just emit statements
        for stmt in &block.body {
            emit_statement(ctx, stmt, is_setof)?;
        }
    }
    Ok(())
}

/// Emit variable assignment
fn emit_assign(ctx: &mut TranspileContext, assign: &PlPgSQLStmtAssign) -> Result<()> {
    let expr_lua = transpile_expr(&assign.expr)?;
    ctx.emit_line(&format!("{} = {}", assign.varname, expr_lua));
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
    } else {
        if !is_setof {
            ctx.emit_line("return");
        }
    }
    Ok(())
}

/// Emit RETURN NEXT statement
fn emit_return_next(ctx: &mut TranspileContext, ret_next: &PlPgSQLStmtReturnNext) -> Result<()> {
    let expr_lua = transpile_expr(&ret_next.expr)?;
    // Accumulate result in a table
    ctx.emit_line("if _result_set == nil then _result_set = {} end");
    ctx.emit_line(&format!("table.insert(_result_set, {})", expr_lua));
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
                .map(|p| transpile_expr(p).unwrap_or_else(|_| "nil".to_string()))
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
                .map(|p| transpile_expr(p).unwrap_or_else(|_| "nil".to_string()))
                .collect();
            if param_list.is_empty() {
                ctx.emit_line(&format!("_ctx.raise(\"{}\", \"{}\")", level, message));
            } else {
                ctx.emit_line(&format!("_ctx.raise(\"{}\", \"{}\", {})", level, message, param_list.join(", ")));
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
    // PostgreSQL diagnostic item kinds:
    // 1 = ROW_COUNT, 2 = RESULT_OID, 3 = COMMAND_FUNCTION_CODE, 
    // 4 = RETURNED_SQLSTATE, 5 = MESSAGE_TEXT, 6 = PG_EXCEPTION_CONTEXT, etc.
    for item in &diag.diag_items {
        let value = match item.kind {
            1 => "_ctx.ROW_COUNT",           // ROW_COUNT
            2 => "_ctx.RESULT_OID or nil",   // RESULT_OID
            3 => "_ctx.command_function",    // COMMAND_FUNCTION_CODE
            4 => "_ctx.SQLSTATE or '00000'", // RETURNED_SQLSTATE
            5 => "_ctx.SQLERRM or ''",       // MESSAGE_TEXT
            6 => "_ctx.PG_CONTEXT or ''",    // PG_EXCEPTION_CONTEXT
            7 => "_ctx.constraint_name",     // CONSTRAINT_NAME
            8 => "_ctx.schema_name",         // SCHEMA_NAME
            9 => "_ctx.table_name",          // TABLE_NAME
            10 => "_ctx.column_name",        // COLUMN_NAME
            11 => "_ctx.datatype_name",      // DATATYPE_NAME
            _ => "nil",
        };
        ctx.emit_line(&format!("{} = {}", item.target_name, value));
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
fn transpile_expr(expr: &PlPgSQLExpr) -> Result<String> {
    // SQL expressions are executed via _ctx.scalar
    // For now, we pass the query directly
    // In a more sophisticated implementation, we'd parse the SQL
    // and convert variable references to parameters
    Ok(format!("_ctx.scalar([[SELECT {}]], {{}})", expr.query))
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
}
