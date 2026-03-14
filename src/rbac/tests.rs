#[cfg(test)]
mod tests {
    use crate::handler::{SqliteHandler, HandlerUtils, SessionContext};
    use crate::transpiler::OperationType;

    fn setup_handler() -> SqliteHandler {
        let handler = SqliteHandler::new(":memory:").expect("Failed to create handler");
        
        // Ensure session 0 exists
        handler.sessions.insert(0, SessionContext::new("postgres".to_string()));
        
        handler
    }

    #[test]
    fn test_superuser_bypasses_all_checks() {
        let handler = setup_handler();
        
        // Superuser should bypass checks even without grants
        let result = handler.check_permissions(&["test_table".to_string()], OperationType::SELECT, "SELECT * FROM test_table");
        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[test]
    fn test_non_superuser_requires_privileges() {
        let handler = setup_handler();
        
        // Create a non-superuser
        {
            let conn = handler.conn.lock().unwrap();
            conn.execute("INSERT INTO __pg_authid__ (rolname, rolsuper) VALUES ('user1', 0)", []).unwrap();
            // Create a real table so it has an OID in pg_class
            conn.execute("CREATE TABLE test_table (id INT)", []).unwrap();
        }
        
        // Set current user to user1
        {
            let mut session = handler.sessions.get_mut(&0).unwrap();
            session.current_user = "user1".to_string();
        }
        
        // Should fail without grants
        let result = handler.check_permissions(&["test_table".to_string()], OperationType::SELECT, "SELECT * FROM test_table");
        assert!(result.is_ok());
        assert!(!result.unwrap());
    }

    #[test]
    fn test_select_privilege_check() {
        let handler = setup_handler();
        
        let user_oid: i64;
        let table_oid: i64;

        // Create role and table
        {
            let conn = handler.conn.lock().unwrap();
            conn.execute("INSERT INTO __pg_authid__ (rolname, rolsuper) VALUES ('user1', 0)", []).unwrap();
            user_oid = conn.last_insert_rowid();
            
            conn.execute("CREATE TABLE test_table (id INT)", []).unwrap();
            table_oid = conn.query_row("SELECT oid FROM pg_class WHERE relname = 'test_table'", [], |row| row.get(0)).unwrap();
            
            // Grant SELECT
            conn.execute("INSERT INTO __pg_acl__ (object_id, object_type, grantee_id, privilege, grantor_id) VALUES (?, 'relation', ?, 'SELECT', 10)", 
                [table_oid, user_oid]).unwrap();
        }
        
        // Set current user to user1
        {
            let mut session = handler.sessions.get_mut(&0).unwrap();
            session.current_user = "user1".to_string();
        }
        
        // Should pass with grant
        let result = handler.check_permissions(&["test_table".to_string()], OperationType::SELECT, "SELECT * FROM test_table");
        assert!(result.is_ok());
        assert!(result.unwrap());
        
        // INSERT should still fail
        let result_insert = handler.check_permissions(&["test_table".to_string()], OperationType::INSERT, "INSERT INTO test_table VALUES (1)");
        assert!(result_insert.is_ok());
        assert!(!result_insert.unwrap());
    }

    #[test]
    fn test_role_inheritance() {
        let handler = setup_handler();
        
        {
            let conn = handler.conn.lock().unwrap();
            // role1 (parent), role2 (member)
            conn.execute("INSERT INTO __pg_authid__ (rolname, rolsuper) VALUES ('role1', 0)", []).unwrap();
            let parent_oid = conn.last_insert_rowid();
            conn.execute("INSERT INTO __pg_authid__ (rolname, rolsuper) VALUES ('role2', 0)", []).unwrap();
            let member_oid = conn.last_insert_rowid();
            
            // Grant role1 to role2
            conn.execute("INSERT INTO __pg_auth_members__ (roleid, member, grantor) VALUES (?, ?, 10)", [parent_oid, member_oid]).unwrap();
            
            // Grant SELECT to role1
            conn.execute("CREATE TABLE inherited_table (id INT)", []).unwrap();
            let table_oid = conn.query_row("SELECT oid FROM pg_class WHERE relname = 'inherited_table'", [], |row| row.get(0)).unwrap();
            conn.execute("INSERT INTO __pg_acl__ (object_id, object_type, grantee_id, privilege, grantor_id) VALUES (?, 'relation', ?, 'SELECT', 10)", 
                [table_oid, parent_oid]).unwrap();
        }
        
        // Set current user to role2
        {
            let mut session = handler.sessions.get_mut(&0).unwrap();
            session.current_user = "role2".to_string();
        }
        
        // role2 should have access via role1
        let result = handler.check_permissions(&["inherited_table".to_string()], OperationType::SELECT, "SELECT * FROM inherited_table");
        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[test]
    fn test_revoke_removes_privilege() {
        let handler = setup_handler();
        
        let user_oid: i64;
        let table_oid: i64;

        {
            let conn = handler.conn.lock().unwrap();
            conn.execute("INSERT INTO __pg_authid__ (rolname, rolsuper) VALUES ('user1', 0)", []).unwrap();
            user_oid = conn.last_insert_rowid();
            
            conn.execute("CREATE TABLE test_table (id INT)", []).unwrap();
            table_oid = conn.query_row("SELECT oid FROM pg_class WHERE relname = 'test_table'", [], |row| row.get(0)).unwrap();
            
            conn.execute("INSERT INTO __pg_acl__ (object_id, object_type, grantee_id, privilege, grantor_id) VALUES (?, 'relation', ?, 'SELECT', 10)", 
                [table_oid, user_oid]).unwrap();
        }
        
        {
            let mut session = handler.sessions.get_mut(&0).unwrap();
            session.current_user = "user1".to_string();
        }
        
        // Pass first
        assert!(handler.check_permissions(&["test_table".to_string()], OperationType::SELECT, "SELECT * FROM test_table").unwrap());
        
        // Revoke
        {
            let conn = handler.conn.lock().unwrap();
            conn.execute("DELETE FROM __pg_acl__ WHERE object_id = ? AND grantee_id = ?", [table_oid, user_oid]).unwrap();
        }
        
        // Should fail now
        assert!(!handler.check_permissions(&["test_table".to_string()], OperationType::SELECT, "SELECT * FROM test_table").unwrap());
    }

    #[test]
    fn test_schema_create_privilege() {
        let handler = setup_handler();
        
        {
            let conn = handler.conn.lock().unwrap();
            conn.execute("INSERT INTO __pg_authid__ (rolname, rolsuper) VALUES ('user1', 0)", []).unwrap();
            conn.execute("INSERT INTO __pg_namespace__ (nspname, nspowner) VALUES ('myschema', 10)", []).unwrap();
        }
        
        {
            let mut session = handler.sessions.get_mut(&0).unwrap();
            session.current_user = "user1".to_string();
        }
        
        // Should fail because we only created schema but didn't grant CREATE
        let result = handler.check_permissions(&["myschema.newtable".to_string()], OperationType::DDL, "CREATE TABLE myschema.newtable (id int)");
        assert!(result.is_err() || !result.unwrap());
        
        // Grant CREATE
        {
            let conn = handler.conn.lock().unwrap();
            let user_oid: i64 = conn.query_row("SELECT oid FROM __pg_authid__ WHERE rolname = 'user1'", [], |row| row.get(0)).unwrap();
            let schema_oid: i64 = conn.query_row("SELECT oid FROM pg_namespace WHERE nspname = 'myschema'", [], |row| row.get(0)).unwrap();
            
            conn.execute("INSERT INTO __pg_acl__ (object_id, object_type, grantee_id, privilege, grantor_id) VALUES (?, 'schema', ?, 'CREATE', 10)", 
                [schema_oid, user_oid]).unwrap();
        }
        
        let result2 = handler.check_permissions(&["myschema.newtable".to_string()], OperationType::DDL, "CREATE TABLE myschema.newtable (id int)");
        assert!(result2.is_ok());
        assert!(result2.unwrap());
    }

    #[test]
    fn test_function_execute_privilege() {
        let handler = setup_handler();
        
        let user_oid: i64;
        let func_oid: i64;

        {
            let conn = handler.conn.lock().unwrap();
            conn.execute("INSERT INTO __pg_authid__ (rolname, rolsuper) VALUES ('user1', 0)", []).unwrap();
            user_oid = conn.last_insert_rowid();
            
            // Register function
            conn.execute("INSERT INTO __pg_functions__ (funcname, schema_name, owner_oid, arg_types, return_type, return_type_kind, function_body, language) 
                          VALUES ('myfunc', 'public', 10, '[]', 'int', 'SCALAR', 'select 1', 'sql')", []).unwrap();
            func_oid = conn.last_insert_rowid();
        }
        
        {
            let mut session = handler.sessions.get_mut(&0).unwrap();
            session.current_user = "user1".to_string();
        }
        
        // Should fail without EXECUTE grant
        let result = handler.check_function_privilege("myfunc");
        assert!(result.is_ok());
        assert!(!result.unwrap());
        
        // Grant EXECUTE
        {
            let conn = handler.conn.lock().unwrap();
            conn.execute("INSERT INTO __pg_acl__ (object_id, object_type, grantee_id, privilege, grantor_id) VALUES (?, 'function', ?, 'EXECUTE', 10)", 
                [func_oid, user_oid]).unwrap();
        }
        
        let result2 = handler.check_function_privilege("myfunc");
        assert!(result2.is_ok());
        assert!(result2.unwrap());
    }
}
