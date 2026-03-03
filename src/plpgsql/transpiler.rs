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
    
    // Generate function header
    ctx.emit_line("-- Generated from PL/pgSQL");
    let func_name = function.fn_name.as_deref().unwrap_or("anonymous");
    ctx.emit_line(&format!("local function {}(_ctx, ...)", func_name));
    ctx.indent();
    
    // Emit parameter declarations
    emit_parameters(&mut ctx, function)?;
    
    // Emit variable declarations (from DECLARE block)
    ctx.emit_line("-- Variable declarations");
    
    // Emit function body
    for stmt in &function.action.block.body {
        emit_statement(&mut ctx, stmt)?;
    }
    
    ctx.dedent();
    ctx.emit_line("end");
    ctx.emit_line(&format!("return {}", func_name));
    
    Ok(ctx.output)
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
        PlPgSQLStmt::DynExecute(dyn_exec) => emit_dyn_execute(ctx, dyn_exec)?,
        PlPgSQLStmt::GetDiag(diag) => emit_get_diag(ctx, diag)?,
        PlPgSQLStmt::Case(case) => emit_case(ctx, case)?,
    }
    Ok(())
}

/// Emit BEGIN/END block
fn emit_block(ctx: &mut TranspileContext, block: &PlPgSQLStmtBlock) -> Result<()> {
    if let Some(exceptions) = &block.exceptions {
        // Block with exception handler - use pcall
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
                emit_statement(ctx, stmt)?;
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
            emit_statement(ctx, stmt)?;
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

/// Emit LOOP statement
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

/// Emit WHILE loop
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

/// Emit FOR i IN start..end loop
fn emit_for_i(ctx: &mut TranspileContext, for_i: &PlPgSQLStmtForI) -> Result<()> {
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
        emit_statement(ctx, stmt)?;
    }
    ctx.dedent();
    ctx.emit_line("end");
    Ok(())
}

/// Emit FOR row IN SELECT loop
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
fn emit_return(ctx: &mut TranspileContext, ret: &PlPgSQLStmtReturn) -> Result<()> {
    if let Some(expr) = &ret.expr {
        let expr_lua = transpile_expr(expr)?;
        ctx.emit_line(&format!("return {}", expr_lua));
    } else {
        ctx.emit_line("return");
    }
    Ok(())
}

/// Emit RETURN NEXT statement
fn emit_return_next(ctx: &mut TranspileContext, ret_next: &PlPgSQLStmtReturnNext) -> Result<()> {
    let expr_lua = transpile_expr(&ret_next.expr)?;
    ctx.emit_line(&format!("_ctx.return_next({})", expr_lua));
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
    for item in &diag.diag_items {
        let value = match item.kind {
            0 => "_ctx.row_count",      // ROW_COUNT
            1 => "_ctx.result_oid",     // RESULT_OID
            2 => "_ctx.pg_context",     // PG_CONTEXT
            _ => "nil",
        };
        ctx.emit_line(&format!("{} = {}", item.target_name, value));
    }
    Ok(())
}

/// Emit CASE statement
fn emit_case(ctx: &mut TranspileContext, case: &PlPgSQLStmtCase) -> Result<()> {
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
            emit_statement(ctx, stmt)?;
        }
        ctx.dedent();
    }
    
    if let Some(else_stmts) = &case.else_stmts {
        ctx.emit_line("else");
        ctx.indent();
        for stmt in else_stmts {
            emit_statement(ctx, stmt)?;
        }
        ctx.dedent();
    }
    
    ctx.emit_line("end");
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
