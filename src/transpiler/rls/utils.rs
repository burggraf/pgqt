//! RLS utilities and helper functions
//!
//! This module contains utility functions for RLS transpilation including
//! role management, grant statements, and table name extraction.

use pg_query::protobuf::node::Node as NodeEnum;
use pg_query::protobuf::{
    CreateRoleStmt, DropRoleStmt, GrantStmt, GrantRoleStmt,
    SelectStmt, InsertStmt, UpdateStmt, DeleteStmt
};
use super::super::context::TranspileContext;
use crate::transpiler::reconstruct_node;

/// Reconstruct a CREATE ROLE statement as an INSERT into __pg_authid__
pub fn reconstruct_create_role_stmt(stmt: &CreateRoleStmt, _ctx: &mut TranspileContext) -> String {
    let role_name = stmt.role.clone();

    let mut superuser = false;
    let mut inherit = true;
    let mut createrole = false;
    let mut createdb = false;
    let mut canlogin = false;
    let mut password = "NULL".to_string();

    for opt in &stmt.options {
        if let Some(ref node) = opt.node {
            if let NodeEnum::DefElem(ref def) = node {
                match def.defname.as_str() {
                    "superuser" => {
                        superuser = def.arg.is_none() || match &def.arg {
                            Some(arg) => match &arg.node {
                                Some(NodeEnum::AConst(aconst)) => {
                                    matches!(&aconst.val, Some(pg_query::protobuf::a_const::Val::Ival(i)) if i.ival != 0)
                                }
                                _ => true,
                            }
                            None => true,
                        };
                    }
                    "inherit" => {
                        inherit = def.arg.is_none() || match &def.arg {
                            Some(arg) => match &arg.node {
                                Some(NodeEnum::AConst(aconst)) => {
                                    matches!(&aconst.val, Some(pg_query::protobuf::a_const::Val::Ival(i)) if i.ival != 0)
                                }
                                _ => true,
                            }
                            None => true,
                        };
                    }
                    "createrole" => {
                        createrole = def.arg.is_none() || match &def.arg {
                            Some(arg) => match &arg.node {
                                Some(NodeEnum::AConst(aconst)) => {
                                    matches!(&aconst.val, Some(pg_query::protobuf::a_const::Val::Ival(i)) if i.ival != 0)
                                }
                                _ => true,
                            }
                            None => true,
                        };
                    }
                    "createdb" => {
                        createdb = def.arg.is_none() || match &def.arg {
                            Some(arg) => match &arg.node {
                                Some(NodeEnum::AConst(aconst)) => {
                                    matches!(&aconst.val, Some(pg_query::protobuf::a_const::Val::Ival(i)) if i.ival != 0)
                                }
                                _ => true,
                            }
                            None => true,
                        };
                    }
                    "canlogin" => {
                        canlogin = def.arg.is_none() || match &def.arg {
                            Some(arg) => match &arg.node {
                                Some(NodeEnum::AConst(aconst)) => {
                                    matches!(&aconst.val, Some(pg_query::protobuf::a_const::Val::Ival(i)) if i.ival != 0)
                                }
                                _ => true,
                            }
                            None => true,
                        };
                    }
                    "password" => {
                        if let Some(ref arg) = def.arg {
                            if let Some(ref val) = arg.node {
                                if let NodeEnum::String(ref s) = val {
                                    password = format!("'{}'", s.sval.replace('\'', "''"));
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    format!(
        "INSERT INTO __pg_authid__ (rolname, rolsuper, rolinherit, rolcreaterole, rolcreatedb, rolcanlogin, rolpassword) \
         VALUES ('{}', {}, {}, {}, {}, {}, {})",
        role_name.to_lowercase(),
        if superuser { 1 } else { 0 },
        if inherit { 1 } else { 0 },
        if createrole { 1 } else { 0 },
        if createdb { 1 } else { 0 },
        if canlogin { 1 } else { 0 },
        password
    )
}

/// Reconstruct a DROP ROLE statement as a DELETE from __pg_authid__
pub fn reconstruct_drop_role_stmt(stmt: &DropRoleStmt) -> String {
    let roles: Vec<String> = stmt.roles.iter().filter_map(|r| {
        if let Some(ref node) = r.node {
            if let NodeEnum::RoleSpec(ref role) = node {
                return Some(format!("'{}'", role.rolename.to_lowercase()));
            }
        }
        None
    }).collect();

    format!("DELETE FROM __pg_authid__ WHERE rolname IN ({})", roles.join(", "))
}

/// Reconstruct a GRANT statement as an INSERT into __pg_acl__
pub fn reconstruct_grant_stmt(stmt: &GrantStmt) -> String {
    let is_grant = stmt.is_grant;
    let objtype = stmt.objtype;

    
    if objtype != pg_query::protobuf::ObjectType::ObjectTable as i32 &&
       objtype != pg_query::protobuf::ObjectType::ObjectView as i32 {
        return "SELECT 1".to_string(); 
    }

    let objects: Vec<String> = stmt.objects.iter().filter_map(|o| {
        if let Some(ref node) = o.node {
            if let NodeEnum::RangeVar(ref rv) = node {
                return Some(rv.relname.to_lowercase());
            }
        }
        None
    }).collect();

    let privileges: Vec<String> = stmt.privileges.iter().filter_map(|p| {
        if let Some(ref node) = p.node {
            if let NodeEnum::AccessPriv(ref ap) = node {
                return Some(ap.priv_name.to_uppercase());
            }
        }
        None
    }).collect();

    let grantees: Vec<String> = stmt.grantees.iter().filter_map(|g| {
        if let Some(ref node) = g.node {
            if let NodeEnum::RoleSpec(ref rs) = node {
                if rs.roletype == pg_query::protobuf::RoleSpecType::RolespecPublic as i32 {
                    return Some("PUBLIC".to_string());
                }
                return Some(rs.rolename.to_lowercase());
            }
        }
        None
    }).collect();

    if is_grant {
        if objects.is_empty() || privileges.is_empty() || grantees.is_empty() {
            return "SELECT 1".to_string();
        }

        let obj = &objects[0];
        let priv_ = &privileges[0];
        let grantee = &grantees[0];
        format!(
            "INSERT INTO __pg_acl__ (object_id, object_type, grantee_id, privilege, grantor_id) \
             SELECT c.oid, 'relation', COALESCE(r.oid, 0), '{}', 10 \
             FROM pg_class c LEFT JOIN pg_roles r ON r.rolname = '{}' \
             WHERE c.relname = '{}'",
            priv_, grantee, obj
        )
    } else {
        format!(
            "DELETE FROM __pg_acl__ WHERE object_id IN (SELECT oid FROM pg_class WHERE relname IN ({})) \
             AND grantee_id IN (SELECT oid FROM pg_roles WHERE rolname IN ({})) \
             AND privilege IN ({})",
            objects.iter().map(|o| format!("'{}'", o)).collect::<Vec<_>>().join(", "),
            grantees.iter().map(|g| format!("'{}'", g)).collect::<Vec<_>>().join(", "),
            privileges.iter().map(|p| format!("'{}'", p)).collect::<Vec<_>>().join(", ")
        )
    }
}

/// Reconstruct a GRANT role statement as an INSERT into __pg_auth_members__
pub fn reconstruct_grant_role_stmt(stmt: &GrantRoleStmt) -> String {
    let is_grant = stmt.is_grant;

    let granted_roles: Vec<String> = stmt.granted_roles.iter().filter_map(|r| {
        if let Some(ref node) = r.node {
            if let NodeEnum::RoleSpec(ref role) = node {
                return Some(role.rolename.to_lowercase());
            }
        }
        None
    }).collect();

    let grantee_roles: Vec<String> = stmt.grantee_roles.iter().filter_map(|r| {
        if let Some(ref node) = r.node {
            if let NodeEnum::RoleSpec(ref role) = node {
                return Some(role.rolename.to_lowercase());
            }
        }
        None
    }).collect();

    if is_grant {
        if granted_roles.is_empty() || grantee_roles.is_empty() {
            return "SELECT 1".to_string();
        }
        format!(
            "INSERT INTO __pg_auth_members__ (roleid, member, grantor) \
             SELECT r.oid, m.oid, 10 \
             FROM pg_roles r, pg_roles m \
             WHERE r.rolname = '{}' AND m.rolname = '{}'",
            granted_roles[0], grantee_roles[0]
        )
    } else {
        format!(
            "DELETE FROM __pg_auth_members__ WHERE roleid IN (SELECT oid FROM pg_roles WHERE rolname IN ({})) \
             AND member IN (SELECT oid FROM pg_roles WHERE rolname IN ({}))",
            granted_roles.iter().map(|r| format!("'{}'", r)).collect::<Vec<_>>().join(", "),
            grantee_roles.iter().map(|r| format!("'{}'", r)).collect::<Vec<_>>().join(", ")
        )
    }
}

/// Extract table name from SELECT statement
pub(crate) fn extract_table_name_from_select(stmt: &SelectStmt) -> String {
    if !stmt.from_clause.is_empty() {
        if let Some(ref node) = stmt.from_clause[0].node {
            if let NodeEnum::RangeVar(ref rv) = node {
                return rv.relname.to_lowercase();
            }
        }
    }
    String::new()
}

/// Extract table name from INSERT statement
pub(crate) fn extract_table_name_from_insert(stmt: &InsertStmt) -> String {
    stmt.relation
        .as_ref()
        .map(|r| r.relname.to_lowercase())
        .unwrap_or_default()
}

/// Extract table name from UPDATE statement
pub(crate) fn extract_table_name_from_update(stmt: &UpdateStmt) -> String {
    stmt.relation
        .as_ref()
        .map(|r| r.relname.to_lowercase())
        .unwrap_or_default()
}

/// Extract table name from DELETE statement
pub(crate) fn extract_table_name_from_delete(stmt: &DeleteStmt) -> String {
    stmt.relation
        .as_ref()
        .map(|r| r.relname.to_lowercase())
        .unwrap_or_default()
}

/// Frame option bitmasks from PostgreSQL (parsenodes.h)
#[allow(dead_code)]
pub mod frame_options {
    pub const NONDEFAULT: i32 = 0x00001;
    pub const RANGE: i32 = 0x00002;
    pub const ROWS: i32 = 0x00004;
    pub const GROUPS: i32 = 0x00008;
    pub const BETWEEN: i32 = 0x00010;
    pub const START_UNBOUNDED_PRECEDING: i32 = 0x00020;
    pub const END_UNBOUNDED_PRECEDING: i32 = 0x00040; 
    pub const START_UNBOUNDED_FOLLOWING: i32 = 0x00080; 
    pub const END_UNBOUNDED_FOLLOWING: i32 = 0x00100;
    pub const START_CURRENT_ROW: i32 = 0x00200;
    pub const END_CURRENT_ROW: i32 = 0x00400;
    pub const START_OFFSET_PRECEDING: i32 = 0x00800;
    pub const END_OFFSET_PRECEDING: i32 = 0x01000;
    pub const START_OFFSET_FOLLOWING: i32 = 0x02000;
    pub const END_OFFSET_FOLLOWING: i32 = 0x04000;
    pub const EXCLUDE_CURRENT_ROW: i32 = 0x08000;
    pub const EXCLUDE_GROUP: i32 = 0x10000;
    pub const EXCLUDE_TIES: i32 = 0x20000;
    pub const EXCLUSION: i32 = EXCLUDE_CURRENT_ROW | EXCLUDE_GROUP | EXCLUDE_TIES;
}
