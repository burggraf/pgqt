//! Shadow Catalog (`__pg_meta__`) for storing PostgreSQL metadata in SQLite
//!
//! This module manages a hidden system table that stores the original PostgreSQL
//! type information, allowing for reversible migrations back to PostgreSQL.

use anyhow::{Context, Result};
use rusqlite::Connection;

/// Represents column metadata for a table
#[derive(Debug, Clone)]
pub struct ColumnMetadata {
    pub table_name: String,
    pub column_name: String,
    pub original_type: String,
    pub constraints: Option<String>,
}

/// Ensures the `__pg_meta__` and RBAC tables exist
pub fn init_catalog(conn: &Connection) -> Result<()> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS __pg_meta__ (
            table_name TEXT NOT NULL,
            column_name TEXT NOT NULL,
            original_type TEXT NOT NULL,
            constraints TEXT,
            PRIMARY KEY (table_name, column_name)
        )",
        [],
    )
    .context("Failed to create __pg_meta__ table")?;

    // Create an index for faster lookups by table name
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_pg_meta_table ON __pg_meta__(table_name)",
        [],
    )
    .context("Failed to create index on __pg_meta__")?;

    // --- RBAC Tables ---

    // __pg_authid__: stores roles
    conn.execute(
        "CREATE TABLE IF NOT EXISTS __pg_authid__ (
            oid INTEGER PRIMARY KEY AUTOINCREMENT,
            rolname TEXT UNIQUE NOT NULL,
            rolsuper BOOLEAN DEFAULT FALSE,
            rolinherit BOOLEAN DEFAULT TRUE,
            rolcreaterole BOOLEAN DEFAULT FALSE,
            rolcreatedb BOOLEAN DEFAULT FALSE,
            rolcanlogin BOOLEAN DEFAULT FALSE,
            rolpassword TEXT
        )",
        [],
    )
    .context("Failed to create __pg_authid__ table")?;

    // __pg_auth_members__: role membership
    conn.execute(
        "CREATE TABLE IF NOT EXISTS __pg_auth_members__ (
            roleid INTEGER NOT NULL,
            member INTEGER NOT NULL,
            grantor INTEGER NOT NULL,
            admin_option BOOLEAN DEFAULT FALSE,
            PRIMARY KEY (roleid, member)
        )",
        [],
    )
    .context("Failed to create __pg_auth_members__ table")?;

    // __pg_acl__: access control lists for relations
    conn.execute(
        "CREATE TABLE IF NOT EXISTS __pg_acl__ (
            object_id INTEGER NOT NULL,
            object_type TEXT NOT NULL, -- 'relation', 'database', 'schema'
            grantee_id INTEGER NOT NULL, -- role OID or 0 for PUBLIC
            privilege TEXT NOT NULL, -- 'SELECT', 'INSERT', etc.
            grantable BOOLEAN DEFAULT FALSE,
            grantor_id INTEGER NOT NULL,
            PRIMARY KEY (object_id, object_type, grantee_id, privilege)
        )",
        [],
    )
    .context("Failed to create __pg_acl__ table")?;

    // __pg_relation_meta__: table/view ownership
    conn.execute(
        "CREATE TABLE IF NOT EXISTS __pg_relation_meta__ (
            relname TEXT PRIMARY KEY,
            relowner INTEGER NOT NULL
        )",
        [],
    )
    .context("Failed to create __pg_relation_meta__ table")?;

    // __pg_rls_policies__: Row-Level Security policies
    conn.execute(
        "CREATE TABLE IF NOT EXISTS __pg_rls_policies__ (
            polname TEXT NOT NULL,
            polrelid TEXT NOT NULL,  -- table name (simplified, in PG this is oid)
            polcmd TEXT DEFAULT 'ALL', -- ALL, SELECT, INSERT, UPDATE, DELETE
            polpermissive BOOLEAN DEFAULT TRUE, -- TRUE = PERMISSIVE, FALSE = RESTRICTIVE
            polroles TEXT, -- comma-separated role names, empty = PUBLIC
            polqual TEXT, -- USING expression (for SELECT, UPDATE, DELETE)
            polwithcheck TEXT, -- WITH CHECK expression (for INSERT, UPDATE)
            polenabled BOOLEAN DEFAULT TRUE,
            PRIMARY KEY (polname, polrelid)
        )",
        [],
    )
    .context("Failed to create __pg_rls_policies__ table")?;

    // __pg_rls_enabled__: Track which tables have RLS enabled
    conn.execute(
        "CREATE TABLE IF NOT EXISTS __pg_rls_enabled__ (
            relname TEXT PRIMARY KEY,
            rls_enabled BOOLEAN DEFAULT FALSE,
            rls_forced BOOLEAN DEFAULT FALSE
        )",
        [],
    )
    .context("Failed to create __pg_rls_enabled__ table")?;

    // Bootstrap: Create default 'postgres' superuser (OID 10)
    conn.execute(
        "INSERT OR IGNORE INTO __pg_authid__ (oid, rolname, rolsuper, rolinherit, rolcreaterole, rolcreatedb, rolcanlogin)
         VALUES (10, 'postgres', 1, 1, 1, 1, 1)",
        [],
    )?;

    Ok(())
}

/// Store column metadata in the shadow catalog
pub fn store_column_metadata(conn: &Connection, metadata: &ColumnMetadata) -> Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO __pg_meta__ (table_name, column_name, original_type, constraints)
         VALUES (?1, ?2, ?3, ?4)",
        (
            &metadata.table_name,
            &metadata.column_name,
            &metadata.original_type,
            &metadata.constraints,
        ),
    )
    .context("Failed to store column metadata")?;

    Ok(())
}

/// Store multiple column metadata entries for a table
pub fn store_table_metadata(
    conn: &Connection,
    table_name: &str,
    columns: &[(String, String, Option<String>)],
) -> Result<()> {
    for (col_name, orig_type, constraints) in columns {
        let metadata = ColumnMetadata {
            table_name: table_name.to_string(),
            column_name: col_name.clone(),
            original_type: orig_type.clone(),
            constraints: constraints.clone(),
        };
        store_column_metadata(conn, &metadata)?;
    }
    Ok(())
}

/// Store relation ownership metadata
pub fn store_relation_metadata(
    conn: &Connection,
    table_name: &str,
    owner_oid: i64,
) -> Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO __pg_relation_meta__ (relname, relowner) VALUES (?1, ?2)",
        (table_name, owner_oid),
    )
    .context("Failed to store relation metadata")?;
    Ok(())
}

#[allow(dead_code)]
/// Retrieve all column metadata for a specific table
pub fn get_table_metadata(conn: &Connection, table_name: &str) -> Result<Vec<ColumnMetadata>> {
    let mut stmt = conn.prepare(
        "SELECT table_name, column_name, original_type, constraints
         FROM __pg_meta__
         WHERE table_name = ?1
         ORDER BY column_name"
    )?;

    let rows = stmt.query_map([table_name], |row| {
        Ok(ColumnMetadata {
            table_name: row.get(0)?,
            column_name: row.get(1)?,
            original_type: row.get(2)?,
            constraints: row.get(3)?,
        })
    })?;

    let mut result = Vec::new();
    for row in rows {
        result.push(row?);
    }

    Ok(result)
}

#[allow(dead_code)]
/// Retrieve metadata for a specific column
pub fn get_column_metadata(
    conn: &Connection,
    table_name: &str,
    column_name: &str,
) -> Result<Option<ColumnMetadata>> {
    let result = conn.query_row(
        "SELECT table_name, column_name, original_type, constraints
         FROM __pg_meta__
         WHERE table_name = ?1 AND column_name = ?2",
        [table_name, column_name],
        |row| {
            Ok(ColumnMetadata {
                table_name: row.get(0)?,
                column_name: row.get(1)?,
                original_type: row.get(2)?,
                constraints: row.get(3)?,
            })
        },
    );

    match result {
        Ok(metadata) => Ok(Some(metadata)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// RLS Policy metadata
#[derive(Debug, Clone)]
pub struct RlsPolicy {
    pub name: String,
    pub table_name: String,
    pub command: String, // ALL, SELECT, INSERT, UPDATE, DELETE
    pub permissive: bool,
    pub roles: Vec<String>,
    pub using_expr: Option<String>,
    pub with_check_expr: Option<String>,
    pub enabled: bool,
}

/// Enable RLS on a table
pub fn enable_rls(conn: &Connection, table_name: &str, force: bool) -> Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO __pg_rls_enabled__ (relname, rls_enabled, rls_forced) VALUES (?1, TRUE, ?2)",
        (table_name, force),
    )
    .context("Failed to enable RLS on table")?;
    Ok(())
}

/// Disable RLS on a table
pub fn disable_rls(conn: &Connection, table_name: &str) -> Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO __pg_rls_enabled__ (relname, rls_enabled, rls_forced) VALUES (?1, FALSE, FALSE)",
        [table_name],
    )
    .context("Failed to disable RLS on table")?;
    Ok(())
}

/// Check if RLS is enabled on a table
pub fn is_rls_enabled(conn: &Connection, table_name: &str) -> Result<bool> {
    let result: Result<bool, _> = conn.query_row(
        "SELECT rls_enabled FROM __pg_rls_enabled__ WHERE relname = ?1",
        [table_name],
        |row| row.get(0),
    );
    match result {
        Ok(enabled) => Ok(enabled),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(false),
        Err(e) => Err(e.into()),
    }
}

/// Check if RLS is forced on a table (bypass for table owner)
pub fn is_rls_forced(conn: &Connection, table_name: &str) -> Result<bool> {
    let result: Result<bool, _> = conn.query_row(
        "SELECT rls_forced FROM __pg_rls_enabled__ WHERE relname = ?1",
        [table_name],
        |row| row.get(0),
    );
    match result {
        Ok(forced) => Ok(forced),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(false),
        Err(e) => Err(e.into()),
    }
}

/// Store an RLS policy
pub fn store_rls_policy(conn: &Connection, policy: &RlsPolicy) -> Result<()> {
    let roles_str = if policy.roles.is_empty() {
        None
    } else {
        Some(policy.roles.join(","))
    };

    conn.execute(
        "INSERT OR REPLACE INTO __pg_rls_policies__ 
         (polname, polrelid, polcmd, polpermissive, polroles, polqual, polwithcheck, polenabled)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        (
            &policy.name,
            &policy.table_name,
            &policy.command,
            policy.permissive,
            roles_str,
            &policy.using_expr,
            &policy.with_check_expr,
            policy.enabled,
        ),
    )
    .context("Failed to store RLS policy")?;
    Ok(())
}

/// Get all policies for a table applicable to a specific command and roles
pub fn get_applicable_policies(
    conn: &Connection,
    table_name: &str,
    command: &str, // SELECT, INSERT, UPDATE, DELETE
    user_roles: &[String],
) -> Result<Vec<RlsPolicy>> {
    let mut stmt = conn.prepare(
        "SELECT polname, polrelid, polcmd, polpermissive, polroles, polqual, polwithcheck, polenabled
         FROM __pg_rls_policies__
         WHERE polrelid = ?1 
         AND polenabled = TRUE
         AND (polcmd = 'ALL' OR polcmd = ?2)"
    )?;

    let rows = stmt.query_map([table_name, command], |row| {
        let roles_str: Option<String> = row.get(4)?;
        let roles = roles_str
            .map(|s| s.split(',').map(|r| r.to_string()).collect())
            .unwrap_or_default();

        Ok(RlsPolicy {
            name: row.get(0)?,
            table_name: row.get(1)?,
            command: row.get(2)?,
            permissive: row.get(3)?,
            roles,
            using_expr: row.get(5)?,
            with_check_expr: row.get(6)?,
            enabled: row.get(7)?,
        })
    })?;

    let mut policies = Vec::new();
    for row in rows {
        let policy = row?;
        // Check if policy applies to current user roles
        // Empty roles means PUBLIC (applies to all)
        if policy.roles.is_empty() 
            || policy.roles.contains(&"PUBLIC".to_string())
            || user_roles.iter().any(|r| policy.roles.contains(r)) {
            policies.push(policy);
        }
    }

    Ok(policies)
}

/// Drop an RLS policy
pub fn drop_rls_policy(conn: &Connection, policy_name: &str, table_name: &str) -> Result<()> {
    conn.execute(
        "DELETE FROM __pg_rls_policies__ WHERE polname = ?1 AND polrelid = ?2",
        (policy_name, table_name),
    )
    .context("Failed to drop RLS policy")?;
    Ok(())
}

/// Get all policies for a table (for admin/inspection)
pub fn get_table_policies(conn: &Connection, table_name: &str) -> Result<Vec<RlsPolicy>> {
    let mut stmt = conn.prepare(
        "SELECT polname, polrelid, polcmd, polpermissive, polroles, polqual, polwithcheck, polenabled
         FROM __pg_rls_policies__
         WHERE polrelid = ?1"
    )?;

    let rows = stmt.query_map([table_name], |row| {
        let roles_str: Option<String> = row.get(4)?;
        let roles = roles_str
            .map(|s| s.split(',').map(|r| r.to_string()).collect())
            .unwrap_or_default();

        Ok(RlsPolicy {
            name: row.get(0)?,
            table_name: row.get(1)?,
            command: row.get(2)?,
            permissive: row.get(3)?,
            roles,
            using_expr: row.get(5)?,
            with_check_expr: row.get(6)?,
            enabled: row.get(7)?,
        })
    })?;

    let mut policies = Vec::new();
    for row in rows {
        policies.push(row?);
    }

    Ok(policies)
}

/// Initialize system catalog views to support psql commands like \dt, \d, etc.
pub fn init_system_views(conn: &Connection) -> Result<()> {
    // pg_namespace: list of schemas
    conn.execute(
        "CREATE VIEW IF NOT EXISTS pg_namespace AS
         SELECT 2200 as oid, 'public' as nspname
         UNION ALL
         SELECT 11 as oid, 'pg_catalog' as nspname
         UNION ALL
         SELECT 12 as oid, 'information_schema' as nspname",
        [],
    )?;

    // pg_class: list of tables, views, indexes, etc.
    conn.execute(
        "CREATE VIEW IF NOT EXISTS pg_class AS
         SELECT
            sm.rowid as oid,
            sm.name as relname,
            2200 as relnamespace,
            COALESCE(rm.relowner, 10) as relowner,
            0 as relam,
            0 as relfilenode,
            0 as reltablespace,
            0 as relpages,
            0 as reltuples,
            0 as relallvisible,
            0 as reltoastrelid,
            false as relhasindex,
            false as relisshared,
            'p' as relpersistence,
            CASE sm.type
                WHEN 'table' THEN 'r'
                WHEN 'view' THEN 'v'
                WHEN 'index' THEN 'i'
                ELSE 's'
            END as relkind,
            0 as relnatts,
            0 as relchecks,
            false as relhasrules,
            false as relhastriggers,
            false as relhassubclass,
            false as relrowsecurity,
            false as relforcerowsecurity,
            true as relispopulated,
            'n' as relreplident,
            false as relispartition,
            0 as relrewrite,
            0 as relfrozenxid,
            0 as relminmxid,
            NULL as relacl,
            NULL as reloptions,
            NULL as relpartbound
         FROM sqlite_master sm
         LEFT JOIN __pg_relation_meta__ rm ON rm.relname = sm.name
         WHERE sm.name NOT LIKE 'sqlite_%' AND sm.name NOT LIKE '__pg_meta__'
         AND sm.name NOT LIKE 'pg_%'",
        [],
    )?;

    // pg_type: basic types
    // Using common PostgreSQL OIDs
    conn.execute(
        "CREATE VIEW IF NOT EXISTS pg_type AS
         SELECT 16 as oid, 'bool' as typname, 11 as typnamespace, 10 as typowner, 1 as typlen, 'b' as typtype, true as typisdefined, ',' as typdelim, 0 as typrelid, 0 as typelem, 0 as typarray
         UNION ALL SELECT 18, 'char', 11, 10, 1, 'b', true, ',', 0, 0, 0
         UNION ALL SELECT 19, 'name', 11, 10, 64, 'b', true, ',', 0, 0, 0
         UNION ALL SELECT 20, 'int8', 11, 10, 8, 'b', true, ',', 0, 0, 0
         UNION ALL SELECT 21, 'int2', 11, 10, 2, 'b', true, ',', 0, 0, 0
         UNION ALL SELECT 23, 'int4', 11, 10, 4, 'b', true, ',', 0, 0, 0
         UNION ALL SELECT 25, 'text', 11, 10, -1, 'b', true, ',', 0, 0, 0
         UNION ALL SELECT 26, 'oid', 11, 10, 4, 'b', true, ',', 0, 0, 0
         UNION ALL SELECT 700, 'float4', 11, 10, 4, 'b', true, ',', 0, 0, 0
         UNION ALL SELECT 701, 'float8', 11, 10, 8, 'b', true, ',', 0, 0, 0
         UNION ALL SELECT 1043, 'varchar', 11, 10, -1, 'b', true, ',', 0, 0, 0
         UNION ALL SELECT 1114, 'timestamp', 11, 10, 8, 'b', true, ',', 0, 0, 0
         UNION ALL SELECT 1184, 'timestamptz', 11, 10, 8, 'b', true, ',', 0, 0, 0",
        [],
    )?;

    // pg_attribute: table columns
    conn.execute(
        "CREATE VIEW IF NOT EXISTS pg_attribute AS
         SELECT
            c.oid as attrelid,
            m.name as attname,
            t.oid as atttypid,
            m.cid + 1 as attnum,
            -1 as attlen,
            0 as atttypmod,
            m.cid + 1 as attndims,
            m.\"notnull\" as attnotnull,
            m.dflt_value IS NOT NULL as atthasdef,
            false as attisdropped,
            true as attislocal,
            0 as attinhcount,
            0 as attcollation,
            '' as attidentity,
            '' as attgenerated,
            NULL as attacl,
            NULL as attoptions,
            NULL as attfdwoptions,
            NULL as attmissingval
         FROM pg_class c
         JOIN pragma_table_info(c.relname) m
         LEFT JOIN pg_type t ON (
            CASE
                WHEN m.type LIKE 'int%' THEN 'int4'
                WHEN m.type LIKE 'text%' THEN 'text'
                WHEN m.type LIKE 'char%' THEN 'varchar'
                WHEN m.type LIKE 'float%' THEN 'float8'
                WHEN m.type LIKE 'real%' THEN 'float4'
                WHEN m.type LIKE 'bool%' THEN 'bool'
                ELSE 'text'
            END = t.typname
         )",
        [],
    )?;

    // pg_attrdef: column default values
    conn.execute(
        "CREATE VIEW IF NOT EXISTS pg_attrdef AS
         SELECT
            c.oid as adrelid,
            m.cid + 1 as adnum,
            m.dflt_value as adbin
         FROM pg_class c
         JOIN pragma_table_info(c.relname) m
         WHERE m.dflt_value IS NOT NULL",
        [],
    )?;

    // pg_constraint: table constraints
    conn.execute(
        "CREATE VIEW IF NOT EXISTS pg_constraint AS
         SELECT
            rowid as oid,
            'c' || rowid as conname,
            2200 as connamespace,
            'c' as contype,
            false as condeferrable,
            false as condeferred,
            true as convalidated,
            (SELECT rowid FROM sqlite_master WHERE name = m.tbl_name AND type = 'table') as conrelid,
            0 as contypid,
            0 as conindid,
            0 as conparentid,
            0 as confrelid,
            'n' as confupdtype,
            'n' as confdeltype,
            'p' as confmatchtype,
            false as conislocal,
            0 as coninhcount,
            false as connoinherit,
            NULL as conkey,
            NULL as confkey,
            NULL as conpfeqop,
            NULL as conppeqop,
            NULL as conffeqop,
            NULL as conexclop,
            NULL as conbin
         FROM sqlite_master m
         WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
        [],
    )?;

    // pg_index: table indexes
    conn.execute(
        "CREATE VIEW IF NOT EXISTS pg_index AS
         SELECT
            rowid as indexrelid,
            (SELECT rowid FROM sqlite_master WHERE name = m.tbl_name AND type = 'table') as indrelid,
            0 as indnatts,
            0 as indnkeyatts,
            false as indisunique,
            false as indisprimary,
            false as indisexclusion,
            false as indimmediate,
            false as indisclustered,
            false as indisvalid,
            false as indcheckxmin,
            false as indisready,
            false as indislive,
            false as indisreplident,
            NULL as indkey,
            NULL as indcollation,
            NULL as indclass,
            NULL as indoption,
            NULL as indexprs,
            NULL as indpred
         FROM sqlite_master m
         WHERE type = 'index' AND name NOT LIKE 'sqlite_%'",
        [],
    )?;

    // pg_am: access methods
    conn.execute(
        "CREATE VIEW IF NOT EXISTS pg_am AS
         SELECT 403 as oid, 'btree' as amname, 'i' as amhandler, 'i' as amtype
         UNION ALL SELECT 405, 'hash', 'i', 'i'
         UNION ALL SELECT 783, 'gist', 'i', 'i'
         UNION ALL SELECT 2742, 'gin', 'i', 'i'
         UNION ALL SELECT 4000, 'spgist', 'i', 'i'
         UNION ALL SELECT 5000, 'brin', 'i', 'i'",
        [],
    )?;

    // pg_description: object comments
    conn.execute(
        "CREATE VIEW IF NOT EXISTS pg_description AS
         SELECT 0 as objoid, 0 as classoid, 0 as objsubid, '' as description
         WHERE 0=1",
        [],
    )?;

    // pg_roles
    conn.execute(
        "CREATE VIEW IF NOT EXISTS pg_roles AS
         SELECT
            oid,
            rolname,
            rolsuper,
            rolinherit,
            rolcreaterole,
            rolcreatedb,
            rolcanlogin,
            -1 as rolconnlimit,
            NULL as rolvaliduntil,
            false as rolreplication,
            false as rolbypassrls,
            NULL as rolconfig
         FROM __pg_authid__",
        [],
    )?;

    // pg_authid
    conn.execute(
        "CREATE VIEW IF NOT EXISTS pg_authid AS
         SELECT * FROM __pg_authid__",
        [],
    )?;

    // pg_auth_members
    conn.execute(
        "CREATE VIEW IF NOT EXISTS pg_auth_members AS
         SELECT roleid, member, grantor, admin_option FROM __pg_auth_members__",
        [],
    )?;

    // pg_settings
    conn.execute(
        "CREATE VIEW IF NOT EXISTS pg_settings AS
         SELECT 'max_connections' as name, '100' as setting
         UNION ALL SELECT 'server_version', '15.0'
         UNION ALL SELECT 'server_encoding', 'UTF8'
         UNION ALL SELECT 'client_encoding', 'UTF8'
         UNION ALL SELECT 'standard_conforming_strings', 'on'
         UNION ALL SELECT 'TimeZone', 'UTC'",
        [],
    )?;

    // pg_proc: list of functions
    conn.execute(
        "CREATE VIEW IF NOT EXISTS pg_proc AS
         SELECT
            rowid as oid,
            name as proname,
            11 as pronamespace,
            10 as proowner,
            0 as prolang,
            0.0 as procost,
            0.0 as prorows,
            0 as provariadic,
            'v' as prokind,
            false as prosecdef,
            false as proleakproof,
            true as proisstrict,
            true as proretset,
            'v' as provolatile,
            0 as pronargs,
            0 as pronargdefaults,
            25 as prorettype,
            NULL as proargtypes,
            NULL as proallargtypes,
            NULL as proargmodes,
            NULL as proargnames,
            NULL as proargdefaults,
            NULL as protrftypes,
            '' as prosrc,
            NULL as probin,
            NULL as prosqlbody,
            NULL as proconfig,
            NULL as proacl
         FROM sqlite_master
         WHERE type = 'table' AND name = 'NON_EXISTENT_JUST_FOR_COLUMNS'
         -- Add common functions here if needed
         UNION ALL SELECT 10001, 'now', 11, 10, 0, 0.0, 0.0, 0, 'f', false, false, true, false, 'v', 0, 0, 1184, NULL, NULL, NULL, NULL, NULL, NULL, 'now', NULL, NULL, NULL, NULL
         UNION ALL SELECT 10002, 'current_timestamp', 11, 10, 0, 0.0, 0.0, 0, 'f', false, false, true, false, 'v', 0, 0, 1184, NULL, NULL, NULL, NULL, NULL, NULL, 'now', NULL, NULL, NULL, NULL",
        [],
    )?;

    // pg_database
    conn.execute(
        "CREATE VIEW IF NOT EXISTS pg_database AS
         SELECT
            1 as oid,
            'postgres' as datname,
            10 as datdba,
            6 as encoding,
            'en_US.UTF-8' as datcollate,
            'en_US.UTF-8' as datctype,
            'c' as datlocprovider,
            NULL as daticulocale,
            NULL as daticurules,
            true as datistemplate,
            true as datallowconn,
            -1 as datconnlimit,
            1 as datlastsysoid,
            1 as datfrozenxid,
            1 as datminmxid,
            1 as dattablespace,
            NULL as datacl",
        [],
    )?;

    Ok(())
}

#[allow(dead_code)]
/// Delete all metadata for a table (e.g., when table is dropped)
pub fn delete_table_metadata(conn: &Connection, table_name: &str) -> Result<()> {
    conn.execute(
        "DELETE FROM __pg_meta__ WHERE table_name = ?1",
        [table_name],
    )
    .context("Failed to delete table metadata")?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn setup_test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        init_catalog(&conn).unwrap();
        conn
    }

    #[test]
    fn test_init_catalog_creates_table() {
        let conn = setup_test_db();
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE name = '__pg_meta__'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn test_store_and_retrieve_column_metadata() {
        let conn = setup_test_db();

        let metadata = ColumnMetadata {
            table_name: "test_table".to_string(),
            column_name: "name".to_string(),
            original_type: "VARCHAR(10)".to_string(),
            constraints: Some("NOT NULL".to_string()),
        };

        store_column_metadata(&conn, &metadata).unwrap();

        let retrieved =
            get_column_metadata(&conn, "test_table", "name")
                .unwrap()
                .expect("Should find metadata");

        assert_eq!(retrieved.table_name, "test_table");
        assert_eq!(retrieved.column_name, "name");
        assert_eq!(retrieved.original_type, "VARCHAR(10)");
        assert_eq!(retrieved.constraints, Some("NOT NULL".to_string()));
    }

    #[test]
    fn test_store_table_metadata() {
        let conn = setup_test_db();

        let columns = vec![
            ("id".to_string(), "SERIAL".to_string(), None),
            (
                "name".to_string(),
                "VARCHAR(10)".to_string(),
                Some("NOT NULL".to_string()),
            ),
            (
                "created_at".to_string(),
                "TIMESTAMP WITH TIME ZONE".to_string(),
                None,
            ),
        ];

        store_table_metadata(&conn, "test_table", &columns).unwrap();

        let metadata = get_table_metadata(&conn, "test_table").unwrap();
        assert_eq!(metadata.len(), 3);

        let types: Vec<String> = metadata
            .iter()
            .map(|m| m.original_type.clone())
            .collect();
        assert!(types.contains(&"SERIAL".to_string()));
        assert!(types.contains(&"VARCHAR(10)".to_string()));
        assert!(types.contains(&"TIMESTAMP WITH TIME ZONE".to_string()));
    }

    #[test]
    fn test_get_nonexistent_column() {
        let conn = setup_test_db();
        let result = get_column_metadata(&conn, "nonexistent", "col").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_delete_table_metadata() {
        let conn = setup_test_db();

        let metadata = ColumnMetadata {
            table_name: "test_table".to_string(),
            column_name: "name".to_string(),
            original_type: "VARCHAR(10)".to_string(),
            constraints: None,
        };

        store_column_metadata(&conn, &metadata).unwrap();
        delete_table_metadata(&conn, "test_table").unwrap();

        let result = get_table_metadata(&conn, "test_table").unwrap();
        assert!(result.is_empty());
    }
}
