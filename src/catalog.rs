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

    // __pg_namespace__: Schema catalog for dynamic schema support
    conn.execute(
        "CREATE TABLE IF NOT EXISTS __pg_namespace__ (
            oid INTEGER PRIMARY KEY AUTOINCREMENT,
            nspname TEXT UNIQUE NOT NULL,
            nspowner INTEGER NOT NULL DEFAULT 10,
            nspacl TEXT
        )",
        [],
    )
    .context("Failed to create __pg_namespace__ table")?;

    // Insert default schemas if they don't exist
    conn.execute(
        "INSERT OR IGNORE INTO __pg_namespace__ (nspname, nspowner) VALUES ('public', 10)",
        [],
    )?;
    conn.execute(
        "INSERT OR IGNORE INTO __pg_namespace__ (nspname, nspowner) VALUES ('pg_catalog', 10)",
        [],
    )?;
    conn.execute(
        "INSERT OR IGNORE INTO __pg_namespace__ (nspname, nspowner) VALUES ('information_schema', 10)",
        [],
    )?;

    // Bootstrap: Create default 'postgres' superuser (OID 10)
    conn.execute(
        "INSERT OR IGNORE INTO __pg_authid__ (oid, rolname, rolsuper, rolinherit, rolcreaterole, rolcreatedb, rolcanlogin)
         VALUES (10, 'postgres', 1, 1, 1, 1, 1)",
        [],
    )?;

    // __pg_type__: Store PostgreSQL type definitions
    conn.execute(
        "CREATE TABLE IF NOT EXISTS __pg_type__ (
            oid INTEGER PRIMARY KEY,
            typname TEXT NOT NULL,
            typnamespace INTEGER DEFAULT 11,
            typowner INTEGER DEFAULT 10,
            typlen INTEGER NOT NULL,
            typbyval BOOLEAN NOT NULL,
            typtype CHAR NOT NULL,
            typcategory CHAR NOT NULL,
            typispreferred BOOLEAN DEFAULT false,
            typisdefined BOOLEAN DEFAULT true,
            typdelim CHAR DEFAULT ',',
            typrelid INTEGER DEFAULT 0,
            typelem INTEGER DEFAULT 0,
            typarray INTEGER DEFAULT 0,
            typinput TEXT,
            typoutput TEXT,
            typreceive TEXT,
            typsend TEXT,
            typmodin TEXT,
            typmodout TEXT,
            typanalyze TEXT,
            typalign CHAR DEFAULT 'i',
            typstorage CHAR DEFAULT 'p',
            typnotnull BOOLEAN DEFAULT false,
            typbasetype INTEGER DEFAULT 0,
            typtypmod INTEGER DEFAULT -1,
            typndims INTEGER DEFAULT 0,
            typcollation INTEGER DEFAULT 0,
            typdefaultbin TEXT,
            typdefault TEXT,
            typacl TEXT
        )",
        [],
    )
    .context("Failed to create __pg_type__ table")?;

    // Insert standard PostgreSQL types
    init_pg_types(conn)?;

    // __pg_attribute__: Store column metadata for pg_attribute view
    conn.execute(
        "CREATE TABLE IF NOT EXISTS __pg_attribute__ (
            attrelid INTEGER NOT NULL,
            attname TEXT NOT NULL,
            atttypid INTEGER NOT NULL DEFAULT 25,
            attstattarget INTEGER DEFAULT -1,
            attlen INTEGER DEFAULT -1,
            attnum INTEGER NOT NULL,
            attndims INTEGER DEFAULT 0,
            attcacheoff INTEGER DEFAULT -1,
            atttypmod INTEGER DEFAULT -1,
            attbyval BOOLEAN DEFAULT false,
            attstorage CHAR DEFAULT 'x',
            attalign CHAR DEFAULT 'i',
            attnotnull BOOLEAN DEFAULT false,
            atthasdef BOOLEAN DEFAULT false,
            atthasmissing BOOLEAN DEFAULT false,
            attidentity TEXT DEFAULT '',
            attgenerated TEXT DEFAULT '',
            attisdropped BOOLEAN DEFAULT false,
            attislocal BOOLEAN DEFAULT true,
            attinhcount INTEGER DEFAULT 0,
            attcollation INTEGER DEFAULT 0,
            attacl TEXT,
            attoptions TEXT,
            attfdwoptions TEXT,
            attmissingval TEXT,
            PRIMARY KEY (attrelid, attname)
        )",
        [],
    )
    .context("Failed to create __pg_attribute__ table")?;

    // __pg_index__: Store index metadata
    conn.execute(
        "CREATE TABLE IF NOT EXISTS __pg_index__ (
            indexrelid INTEGER PRIMARY KEY,
            indrelid INTEGER NOT NULL,
            indnatts INTEGER NOT NULL DEFAULT 0,
            indnkeyatts INTEGER NOT NULL DEFAULT 0,
            indisunique BOOLEAN DEFAULT false,
            indnullsnotdistinct BOOLEAN DEFAULT false,
            indisprimary BOOLEAN DEFAULT false,
            indisexclusion BOOLEAN DEFAULT false,
            indimmediate BOOLEAN DEFAULT true,
            indisclustered BOOLEAN DEFAULT false,
            indisvalid BOOLEAN DEFAULT true,
            indcheckxmin BOOLEAN DEFAULT false,
            indisready BOOLEAN DEFAULT true,
            indislive BOOLEAN DEFAULT true,
            indisreplident BOOLEAN DEFAULT false,
            indkey TEXT,
            indcollation TEXT,
            indclass TEXT,
            indoption TEXT,
            indexprs TEXT,
            indpred TEXT
        )",
        [],
    )
    .context("Failed to create __pg_index__ table")?;

    // __pg_constraint__: Store constraint metadata
    conn.execute(
        "CREATE TABLE IF NOT EXISTS __pg_constraint__ (
            oid INTEGER PRIMARY KEY,
            conname TEXT NOT NULL,
            connamespace INTEGER DEFAULT 2200,
            contype CHAR NOT NULL,
            condeferrable BOOLEAN DEFAULT false,
            condeferred BOOLEAN DEFAULT false,
            convalidated BOOLEAN DEFAULT true,
            conrelid INTEGER DEFAULT 0,
            contypid INTEGER DEFAULT 0,
            conindid INTEGER DEFAULT 0,
            conparentid INTEGER DEFAULT 0,
            confrelid INTEGER DEFAULT 0,
            confupdtype CHAR DEFAULT 'a',
            confdeltype CHAR DEFAULT 'a',
            confmatchtype CHAR DEFAULT 'u',
            conislocal BOOLEAN DEFAULT true,
            coninhcount INTEGER DEFAULT 0,
            connoinherit BOOLEAN DEFAULT false,
            conkey TEXT,
            confkey TEXT,
            conpfeqop TEXT,
            conppeqop TEXT,
            conffeqop TEXT,
            conexclop TEXT,
            conbin TEXT
        )",
        [],
    )
    .context("Failed to create __pg_constraint__ table")?;

    // __pg_attrdef__: Store column default values
    conn.execute(
        "CREATE TABLE IF NOT EXISTS __pg_attrdef__ (
            oid INTEGER PRIMARY KEY AUTOINCREMENT,
            adrelid INTEGER NOT NULL,
            adnum INTEGER NOT NULL,
            adbin TEXT,
            adsrc TEXT,
            UNIQUE(adrelid, adnum)
        )",
        [],
    )
    .context("Failed to create __pg_attrdef__ table")?;

    // __pg_extension__: Store extension info
    conn.execute(
        "CREATE TABLE IF NOT EXISTS __pg_extension__ (
            oid INTEGER PRIMARY KEY,
            extname TEXT NOT NULL,
            extowner INTEGER DEFAULT 10,
            extnamespace INTEGER DEFAULT 2200,
            extrelocatable BOOLEAN DEFAULT false,
            extversion TEXT DEFAULT '1.0',
            extconfig TEXT,
            extcondition TEXT
        )",
        [],
    )
    .context("Failed to create __pg_extension__ table")?;

    // Insert common extensions as "installed"
    conn.execute(
        "INSERT OR IGNORE INTO __pg_extension__ (oid, extname, extversion) VALUES
         (1, 'plpgsql', '1.0'),
         (2, 'uuid-ossp', '1.1'),
         (3, 'pg_trgm', '1.6'),
         (4, 'pgcrypto', '1.3')",
        [],
    )?;

    // __pg_enum__: Store enum values
    conn.execute(
        "CREATE TABLE IF NOT EXISTS __pg_enum__ (
            oid INTEGER PRIMARY KEY,
            enumtypid INTEGER NOT NULL,
            enumsortorder REAL NOT NULL,
            enumlabel TEXT NOT NULL
        )",
        [],
    )
    .context("Failed to create __pg_enum__ table")?;

    Ok(())
}

/// Initialize PostgreSQL type definitions
fn init_pg_types(conn: &Connection) -> Result<()> {
    let types = vec![
        // oid, typname, typlen, typbyval, typtype, typcategory
        (16, "bool", 1, true, 'b', 'B'),
        (17, "bytea", -1, false, 'b', 'U'),
        (18, "char", 1, true, 'b', 'S'),
        (19, "name", 64, false, 'b', 'S'),
        (20, "int8", 8, true, 'b', 'N'),
        (21, "int2", 2, true, 'b', 'N'),
        (22, "int2vector", -1, false, 'b', 'A'),
        (23, "int4", 4, true, 'b', 'N'),
        (24, "regproc", 4, true, 'b', 'N'),
        (25, "text", -1, false, 'b', 'S'),
        (26, "oid", 4, true, 'b', 'N'),
        (27, "tid", 6, false, 'b', 'U'),
        (28, "xid", 4, true, 'b', 'U'),
        (29, "cid", 4, true, 'b', 'U'),
        (30, "oidvector", -1, false, 'b', 'A'),
        (32, "pg_ddl_command", 8, true, 'p', 'P'),
        (71, "pg_type", -1, false, 'c', 'C'),
        (75, "pg_attribute", -1, false, 'c', 'C'),
        (81, "pg_proc", -1, false, 'c', 'C'),
        (83, "pg_class", -1, false, 'c', 'C'),
        (114, "json", -1, false, 'b', 'U'),
        (142, "xml", -1, false, 'b', 'U'),
        (194, "pg_node_tree", -1, false, 'b', 'S'),
        (3361, "pg_ndistinct", -1, false, 'b', 'U'),
        (3402, "pg_dependencies", -1, false, 'b', 'U'),
        (5017, "pg_mcv_list", -1, false, 'b', 'U'),
        (600, "point", 16, false, 'b', 'G'),
        (601, "lseg", 32, false, 'b', 'G'),
        (602, "path", -1, false, 'b', 'G'),
        (603, "box", 32, false, 'b', 'G'),
        (604, "polygon", -1, false, 'b', 'G'),
        (628, "line", 24, false, 'b', 'G'),
        (700, "float4", 4, true, 'b', 'N'),
        (701, "float8", 8, true, 'b', 'N'),
        (705, "unknown", -1, false, 'b', 'X'),
        (718, "circle", 24, false, 'b', 'G'),
        (790, "money", 8, true, 'b', 'N'),
        (829, "macaddr", 6, false, 'b', 'U'),
        (869, "inet", -1, false, 'b', 'I'),
        (650, "cidr", -1, false, 'b', 'I'),
        (774, "macaddr8", 8, false, 'b', 'U'),
        (1000, "boolarray", -1, false, 'b', 'A'),
        (1001, "byteaarray", -1, false, 'b', 'A'),
        (1002, "chararray", -1, false, 'b', 'A'),
        (1003, "namearray", -1, false, 'b', 'A'),
        (1005, "int2array", -1, false, 'b', 'A'),
        (1007, "int4array", -1, false, 'b', 'A'),
        (1009, "textarray", -1, false, 'b', 'A'),
        (1014, "bpchararray", -1, false, 'b', 'A'),
        (1015, "varchararray", -1, false, 'b', 'A'),
        (1016, "int8array", -1, false, 'b', 'A'),
        (1021, "float4array", -1, false, 'b', 'A'),
        (1022, "float8array", -1, false, 'b', 'A'),
        (1042, "bpchar", -1, false, 'b', 'S'),
        (1043, "varchar", -1, false, 'b', 'S'),
        (1082, "date", 4, true, 'b', 'D'),
        (1083, "time", 8, true, 'b', 'D'),
        (1114, "timestamp", 8, true, 'b', 'D'),
        (1184, "timestamptz", 8, true, 'b', 'D'),
        (1186, "interval", 16, false, 'b', 'D'),
        (1266, "timetz", 12, false, 'b', 'D'),
        (1560, "bit", -1, false, 'b', 'V'),
        (1562, "varbit", -1, false, 'b', 'V'),
        (1700, "numeric", -1, false, 'b', 'N'),
        (1790, "refcursor", -1, false, 'b', 'U'),
        (2202, "regprocedure", 4, true, 'b', 'N'),
        (2203, "regoper", 4, true, 'b', 'N'),
        (2204, "regoperator", 4, true, 'b', 'N'),
        (2205, "regclass", 4, true, 'b', 'N'),
        (2206, "regtype", 4, true, 'b', 'N'),
        (2249, "record", -1, false, 'b', 'P'),
        (2275, "cstring", -1, false, 'b', 'P'),
        (2276, "any", 4, true, 'b', 'P'),
        (2277, "anyarray", -1, false, 'b', 'P'),
        (2278, "void", 4, true, 'b', 'P'),
        (2279, "trigger", 4, true, 'b', 'P'),
        (2280, "language_handler", 4, true, 'b', 'P'),
        (2281, "internal", 8, true, 'b', 'P'),
        (2776, "anyelement", 4, true, 'b', 'P'),
        (2950, "uuid", 16, false, 'b', 'U'),
        (3220, "pg_lsn", 8, true, 'b', 'U'),
        (3310, "pg_snapshot", -1, false, 'b', 'U'),
        (3500, "anyenum", 4, true, 'b', 'P'),
        (3614, "tsvector", -1, false, 'b', 'U'),
        (3615, "tsquery", -1, false, 'b', 'U'),
        (3642, "gtsvector", -1, false, 'b', 'U'),
        (3734, "regconfig", 4, true, 'b', 'N'),
        (3769, "regdictionary", 4, true, 'b', 'N'),
        (3802, "jsonb", -1, false, 'b', 'U'),
        (3904, "type_jsonb_path", -1, false, 'b', 'U'),
        (3905, "jsonbarray", -1, false, 'b', 'A'),
        (3906, "jsonarray", -1, false, 'b', 'A'),
        (3907, "jsonpath", -1, false, 'b', 'U'),
        (3912, "jsonpatharray", -1, false, 'b', 'A'),
        (4089, "regrole", 4, true, 'b', 'N'),
        (4090, "regnamespace", 4, true, 'b', 'N'),
        (4096, "regcollation", 4, true, 'b', 'N'),
    ];

    for (oid, name, len, byval, typtype, category) in &types {
        conn.execute(
            "INSERT OR IGNORE INTO __pg_type__ 
             (oid, typname, typlen, typbyval, typtype, typcategory)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            (*oid, *name, *len, *byval, typtype.to_string(), category.to_string()),
        )?;
    }

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
#[allow(dead_code)]
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
#[allow(dead_code)]
pub fn enable_rls(conn: &Connection, table_name: &str, force: bool) -> Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO __pg_rls_enabled__ (relname, rls_enabled, rls_forced) VALUES (?1, TRUE, ?2)",
        (table_name, force),
    )
    .context("Failed to enable RLS on table")?;
    Ok(())
}

/// Disable RLS on a table
#[allow(dead_code)]
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
#[allow(dead_code)]
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
#[allow(dead_code)]
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
#[allow(dead_code)]
pub fn drop_rls_policy(conn: &Connection, policy_name: &str, table_name: &str) -> Result<()> {
    conn.execute(
        "DELETE FROM __pg_rls_policies__ WHERE polname = ?1 AND polrelid = ?2",
        (policy_name, table_name),
    )
    .context("Failed to drop RLS policy")?;
    Ok(())
}

/// Get all policies for a table (for admin/inspection)
#[allow(dead_code)]
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
/// Initialize system catalog views to support psql commands like \dt, \d, etc.
pub fn init_system_views(conn: &Connection) -> Result<()> {
    // pg_namespace: list of schemas
    conn.execute(
        "CREATE VIEW IF NOT EXISTS pg_namespace AS
         SELECT oid, nspname, nspowner, nspacl
         FROM __pg_namespace__",
        [],
    )?;

    // pg_class: list of tables, views, indexes, etc.
    conn.execute(
        "CREATE VIEW IF NOT EXISTS pg_class AS
         SELECT
            sm.rowid as oid,
            sm.name as relname,
            (SELECT oid FROM pg_namespace WHERE nspname = 'public') as relnamespace,
            0 as reltype,
            0 as reloftype,
            COALESCE(rm.relowner, 10) as relowner,
            0 as relam,
            0 as relfilenode,
            0 as reltablespace,
            0 as relpages,
            0.0 as reltuples,
            0 as relallvisible,
            0 as reltoastrelid,
            false as relhasindex,
            false as relisshared,
            'p' as relpersistence,
            CASE sm.type
                WHEN 'table' THEN 'r'
                WHEN 'view' THEN 'v'
                WHEN 'index' THEN 'i'
                WHEN 'trigger' THEN 'r'
                ELSE 'r'
            END as relkind,
            0 as relnatts,  -- Simplified to avoid pragma in view
            0 as relchecks,
            false as relhasrules,
            false as relhastriggers,
            false as relhassubclass,
            COALESCE(re.rls_enabled, false) as relrowsecurity,
            COALESCE(re.rls_forced, false) as relforcerowsecurity,
            true as relispopulated,
            'd' as relreplident,
            false as relispartition,
            0 as relrewrite,
            0 as relfrozenxid,
            0 as relminmxid,
            NULL as relacl,
            NULL as reloptions,
            NULL as relpartbound
         FROM sqlite_master sm
         LEFT JOIN __pg_relation_meta__ rm ON rm.relname = sm.name
         LEFT JOIN __pg_rls_enabled__ re ON re.relname = sm.name
         WHERE sm.name NOT LIKE 'sqlite_%' 
           AND sm.name NOT LIKE '__pg_%'
           AND sm.type IN ('table', 'view', 'index')",
        [],
    )?;

    // pg_type: all data types from __pg_type__
    conn.execute(
        "CREATE VIEW IF NOT EXISTS pg_type AS
         SELECT
            oid, typname, typnamespace, typowner, typlen, typbyval,
            typtype, typcategory, typispreferred, typisdefined, typdelim,
            typrelid, typelem, typarray, typinput, typoutput, typreceive,
            typsend, typmodin, typmodout, typanalyze, typalign, typstorage,
            typnotnull, typbasetype, typtypmod, typndims, typcollation,
            typdefaultbin, typdefault, typacl
         FROM __pg_type__",
        [],
    )?;

    // pg_attribute: table columns from __pg_attribute__
    conn.execute(
        "CREATE VIEW IF NOT EXISTS pg_attribute AS
         SELECT
            attrelid, attname, atttypid, attstattarget, attlen,
            attnum, attndims, attcacheoff, atttypmod, attbyval,
            attstorage, attalign, attnotnull, atthasdef, atthasmissing,
            attidentity, attgenerated, attisdropped, attislocal,
            attinhcount, attcollation, attacl, attoptions, attfdwoptions,
            attmissingval
         FROM __pg_attribute__",
        [],
    )?;

    // pg_attrdef: column default values from __pg_attrdef__
    conn.execute(
        "CREATE VIEW IF NOT EXISTS pg_attrdef AS
         SELECT oid, adrelid, adnum, adbin, adsrc
         FROM __pg_attrdef__",
        [],
    )?;

    // pg_constraint: constraints from __pg_constraint__
    conn.execute(
        "CREATE VIEW IF NOT EXISTS pg_constraint AS
         SELECT
            oid, conname, connamespace, contype, condeferrable, condeferred,
            convalidated, conrelid, contypid, conindid, conparentid, confrelid,
            confupdtype, confdeltype, confmatchtype, conislocal, coninhcount,
            connoinherit, conkey, confkey, conpfeqop, conppeqop, conffeqop,
            conexclop, conbin
         FROM __pg_constraint__",
        [],
    )?;

    // pg_index: index metadata from __pg_index__
    conn.execute(
        "CREATE VIEW IF NOT EXISTS pg_index AS
         SELECT
            indexrelid, indrelid, indnatts, indnkeyatts, indisunique,
            indnullsnotdistinct, indisprimary, indisexclusion, indimmediate,
            indisclustered, indisvalid, indcheckxmin, indisready, indislive,
            indisreplident, indkey, indcollation, indclass, indoption,
            indexprs, indpred
         FROM __pg_index__",
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

    // pg_description: object comments (empty)
    conn.execute(
        "CREATE VIEW IF NOT EXISTS pg_description AS
         SELECT 0 as objoid, 0 as classoid, 0 as objsubid, '' as description
         WHERE 0=1",
        [],
    )?;

    // pg_roles: user-friendly role view
    conn.execute(
        "CREATE VIEW IF NOT EXISTS pg_roles AS
         SELECT
            oid, rolname, rolsuper, rolinherit, rolcreaterole, rolcreatedb,
            rolcanlogin, -1 as rolconnlimit, NULL as rolvaliduntil,
            false as rolreplication, false as rolbypassrls, NULL as rolconfig
         FROM __pg_authid__",
        [],
    )?;

    // pg_authid: raw authid view
    conn.execute(
        "CREATE VIEW IF NOT EXISTS pg_authid AS
         SELECT * FROM __pg_authid__",
        [],
    )?;

    // pg_auth_members: role memberships
    conn.execute(
        "CREATE VIEW IF NOT EXISTS pg_auth_members AS
         SELECT roleid, member, grantor, admin_option FROM __pg_auth_members__",
        [],
    )?;

    // pg_settings: server settings
    conn.execute(
        "CREATE VIEW IF NOT EXISTS pg_settings AS
         SELECT 'max_connections' as name, '100' as setting, NULL as unit,
                'Connections and Authentication' as category, 
                'Sets the maximum number of concurrent connections.' as short_desc,
                NULL as extra_desc, 'postmaster' as context, 'integer' as vartype,
                'default' as source, '1' as min_val, '262143' as max_val,
                NULL as enumvals, '100' as boot_val, '100' as reset_val,
                NULL as sourcefile, NULL as sourceline, false as pending_restart
         UNION ALL SELECT 'server_version', '15.0', NULL, 'Version and Platform Compatibility',
                'Shows the server version.', NULL, 'internal', 'string', 'default',
                NULL, NULL, NULL, '15.0', '15.0', NULL, NULL, false
         UNION ALL SELECT 'server_encoding', 'UTF8', NULL, 'Client Connection Defaults',
                'Sets the server (database) character set encoding.', NULL,
                'internal', 'string', 'default', NULL, NULL, NULL, 'UTF8', 'UTF8',
                NULL, NULL, false
         UNION ALL SELECT 'client_encoding', 'UTF8', NULL, 'Client Connection Defaults',
                'Sets the client-side encoding (character set).', NULL, 'user',
                'string', 'default', NULL, NULL, NULL, 'UTF8', 'UTF8', NULL, NULL, false
         UNION ALL SELECT 'standard_conforming_strings', 'on', NULL, 'Version and Platform Compatibility',
                'Causes ... strings to treat backslashes literally.', NULL,
                'user', 'bool', 'default', NULL, NULL, NULL, 'on', 'on', NULL, NULL, false
         UNION ALL SELECT 'TimeZone', 'UTC', NULL, 'Client Connection Defaults',
                'Sets the time zone for displaying and interpreting time stamps.',
                NULL, 'user', 'string', 'default', NULL, NULL, NULL, 'UTC', 'UTC',
                NULL, NULL, false",
        [],
    )?;

    // pg_proc: list of functions
    conn.execute(
        "CREATE VIEW IF NOT EXISTS pg_proc AS
         SELECT
            10001 as oid, 'now' as proname, 11 as pronamespace, 10 as proowner,
            13 as prolang, 1.0 as procost, 0.0 as prorows, 0 as provariadic,
            'f' as prokind, false as prosecdef, false as proleakproof,
            true as proisstrict, false as proretset, 's' as provolatile,
            0 as pronargs, 0 as pronargdefaults, 1184 as prorettype,
            NULL as proargtypes, NULL as proallargtypes, NULL as proargmodes,
            NULL as proargnames, NULL as proargdefaults, NULL as protrftypes,
            'now' as prosrc, NULL as probin, NULL as prosqlbody, NULL as proconfig, NULL as proacl
         UNION ALL
         SELECT 10002, 'current_timestamp', 11, 10, 13, 1.0, 0.0, 0, 'f', false,
                false, true, false, 's', 0, 0, 1184, NULL, NULL, NULL, NULL, NULL,
                NULL, 'now', NULL, NULL, NULL, NULL
         UNION ALL
         SELECT 10003, 'current_date', 11, 10, 13, 1.0, 0.0, 0, 'f', false,
                false, true, false, 's', 0, 0, 1082, NULL, NULL, NULL, NULL, NULL,
                NULL, 'current_date', NULL, NULL, NULL, NULL
         UNION ALL
         SELECT 10004, 'current_time', 11, 10, 13, 1.0, 0.0, 0, 'f', false,
                false, true, false, 's', 0, 0, 1266, NULL, NULL, NULL, NULL, NULL,
                NULL, 'current_time', NULL, NULL, NULL, NULL",
        [],
    )?;

    // pg_database: database information
    conn.execute(
        "CREATE VIEW IF NOT EXISTS pg_database AS
         SELECT
            1 as oid, 'postgres' as datname, 10 as datdba, 6 as encoding,
            'en_US.UTF-8' as datcollate, 'en_US.UTF-8' as datctype,
            'c' as datlocprovider, NULL as daticulocale, NULL as daticurules,
            true as datistemplate, true as datallowconn, -1 as datconnlimit,
            1 as datlastsysoid, 1 as datfrozenxid, 1 as datminmxid,
            1 as dattablespace, NULL as datacl",
        [],
    )?;

    // pg_extension: installed extensions
    conn.execute(
        "CREATE VIEW IF NOT EXISTS pg_extension AS
         SELECT oid, extname, extowner, extnamespace, extrelocatable,
                extversion, extconfig, extcondition
         FROM __pg_extension__",
        [],
    )?;

    // pg_enum: enum values
    conn.execute(
        "CREATE VIEW IF NOT EXISTS pg_enum AS
         SELECT oid, enumtypid, enumsortorder, enumlabel
         FROM __pg_enum__",
        [],
    )?;

    // pg_tables: user-friendly table listing
    conn.execute(
        "CREATE VIEW IF NOT EXISTS pg_tables AS
         SELECT
            n.nspname as schemaname,
            c.relname as tablename,
            COALESCE((SELECT rolname FROM pg_roles WHERE oid = c.relowner), 'postgres') as tableowner,
            NULL as tablespace,
            c.relhasindex as hasindexes,
            c.relhasrules as hasrules,
            c.relhastriggers as hastriggers,
            c.relrowsecurity as rowsecurity
         FROM pg_class c
         JOIN pg_namespace n ON c.relnamespace = n.oid
         WHERE c.relkind = 'r'",
        [],
    )?;

    // pg_views: user-friendly view listing
    conn.execute(
        "CREATE VIEW IF NOT EXISTS pg_views AS
         SELECT
            n.nspname as schemaname,
            c.relname as viewname,
            COALESCE((SELECT rolname FROM pg_roles WHERE oid = c.relowner), 'postgres') as viewowner,
            NULL as definition
         FROM pg_class c
         JOIN pg_namespace n ON c.relnamespace = n.oid
         WHERE c.relkind = 'v'",
        [],
    )?;

    // pg_indexes: user-friendly index listing
    conn.execute(
        "CREATE VIEW IF NOT EXISTS pg_indexes AS
         SELECT
            n.nspname as schemaname,
            t.relname as tablename,
            i.relname as indexname,
            NULL as tablespace,
            'CREATE INDEX ' || i.relname || ' ON ' || t.relname || ' (...)'
                as indexdef
         FROM pg_index ix
         JOIN pg_class i ON i.oid = ix.indexrelid
         JOIN pg_class t ON t.oid = ix.indrelid
         JOIN pg_namespace n ON n.oid = t.relnamespace",
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

/// Populate __pg_attribute__ for a given table from sqlite metadata
pub fn populate_pg_attribute(conn: &Connection, table_name: &str) -> Result<()> {
    // Get the table's OID from pg_class
    let oid_result: Result<i64, _> = conn.query_row(
        "SELECT oid FROM pg_class WHERE relname = ?1",
        [table_name],
        |row| row.get(0)
    );
    
    let oid = match oid_result {
        Ok(o) => o,
        Err(_) => return Ok(()), // Table not in pg_class yet, skip
    };
    
    // Clear existing entries for this table
    conn.execute(
        "DELETE FROM __pg_attribute__ WHERE attrelid = ?1",
        [oid],
    )?;
    
    // Get column info from pragma_table_info
    let mut stmt = conn.prepare(
        "SELECT name, type, cid, \"notnull\", dflt_value 
         FROM pragma_table_info(?1)"
    )?;
    
    let rows = stmt.query_map([table_name], |row| {
        Ok((
            row.get::<_, String>(0)?,  // name
            row.get::<_, String>(1)?,  // type
            row.get::<_, i64>(2)?,     // cid (column id)
            row.get::<_, bool>(3)?,    // notnull
            row.get::<_, Option<String>>(4)?,  // dflt_value
        ))
    })?;
    
    for row in rows {
        let (col_name, col_type, cid, notnull, dflt) = row?;
        
        // Map SQLite type to PostgreSQL type OID
        let typid = match col_type.to_lowercase().as_str() {
            t if t.contains("int") => 23,      // int4
            t if t.contains("real") => 700,    // float4
            t if t.contains("float") => 701,   // float8
            t if t.contains("bool") => 16,     // bool
            t if t.contains("blob") => 17,     // bytea
            _ => 25,                           // text (default)
        };
        
        conn.execute(
            "INSERT INTO __pg_attribute__ 
             (attrelid, attname, atttypid, attnum, attnotnull, atthasdef)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            (oid, col_name, typid, cid + 1, notnull, dflt.is_some()),
        )?;
    }
    
    Ok(())
}

/// Populate __pg_index__ from sqlite_master
pub fn populate_pg_index(conn: &Connection) -> Result<()> {
    // Clear and repopulate
    conn.execute("DELETE FROM __pg_index__", [])?;
    
    let mut stmt = conn.prepare(
        "SELECT sm.rowid, sm.name, sm.sql, sm.tbl_name 
         FROM sqlite_master sm 
         WHERE sm.type = 'index' 
         AND sm.name NOT LIKE 'sqlite_%' 
         AND sm.name NOT LIKE '__pg_%'"
    )?;
    
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,  // rowid (indexrelid)
            row.get::<_, String>(1)?,  // name
            row.get::<_, Option<String>>(2)?,  // sql
            row.get::<_, String>(3)?,  // tbl_name
        ))
    })?;
    
    for row in rows {
        let (indexrelid, _name, _sql, tbl_name) = row?;
        
        // Get table OID
        let table_oid: Option<i64> = conn.query_row(
            "SELECT oid FROM pg_class WHERE relname = ?1",
            [&tbl_name],
            |row| row.get(0)
        ).ok();
        
        if let Some(indrelid) = table_oid {
            // Determine if it's a unique/primary index from the SQL
            let is_unique = _sql.as_ref().map(|s| s.to_uppercase().contains("UNIQUE")).unwrap_or(false);
            let is_primary = _name.starts_with("sqlite_autoindex") || 
                            _sql.as_ref().map(|s| s.to_uppercase().contains("PRIMARY")).unwrap_or(false);
            
            conn.execute(
                "INSERT INTO __pg_index__ 
                 (indexrelid, indrelid, indisunique, indisprimary)
                 VALUES (?1, ?2, ?3, ?4)",
                (indexrelid, indrelid, is_unique, is_primary),
            )?;
        }
    }
    
    Ok(())
}

/// Populate __pg_constraint__ from SQLite constraints
pub fn populate_pg_constraint(conn: &Connection) -> Result<()> {
    conn.execute("DELETE FROM __pg_constraint__", [])?;
    
    // Get all tables
    let mut stmt = conn.prepare(
        "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' AND name NOT LIKE '__pg_%'"
    )?;
    
    let tables: Vec<String> = stmt.query_map([], |row| row.get(0))?
        .filter_map(|r| r.ok())
        .collect();
    
    let mut oid_counter: i64 = 10000;
    
    for table in &tables {
        // Get table OID
        let table_oid: i64 = conn.query_row(
            "SELECT oid FROM pg_class WHERE relname = ?1",
            [table],
            |row| row.get(0)
        ).unwrap_or(0);
        
        if table_oid == 0 {
            continue;
        }
        
        // Get primary key info from pragma_table_info
        let mut pk_stmt = conn.prepare(
            "SELECT name, cid FROM pragma_table_info(?1) WHERE pk > 0 ORDER BY pk"
        )?;
        
        let pk_cols: Vec<(String, i64)> = pk_stmt.query_map([table], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?.filter_map(|r| r.ok()).collect();
        
        if !pk_cols.is_empty() {
            let pk_name = format!("{}_pkey", table);
            let pk_key = pk_cols.iter().map(|(_, cid)| (cid + 1).to_string()).collect::<Vec<_>>().join(" ");
            
            conn.execute(
                "INSERT INTO __pg_constraint__ 
                 (oid, conname, contype, conrelid, conkey)
                 VALUES (?1, ?2, 'p', ?3, ?4)",
                (oid_counter, &pk_name, table_oid, pk_key),
            )?;
            oid_counter += 1;
        }
        
        // Get foreign keys from pragma_foreign_key_list
        let mut fk_stmt = conn.prepare("SELECT id, seq, \"table\", \"from\", \"to\", on_update, on_delete, match FROM pragma_foreign_key_list(?1)")?;
        let fk_rows = fk_stmt.query_map([table], |row| {
            Ok((
                row.get::<_, i64>(0)?,  // id
                row.get::<_, String>(1)?,  // seq
                row.get::<_, String>(2)?,  // table
                row.get::<_, String>(3)?,  // from
                row.get::<_, String>(4)?,  // to
                row.get::<_, String>(5)?,  // on_update
                row.get::<_, String>(6)?,  // on_delete
                row.get::<_, String>(7)?,  // match
            ))
        })?;
        
        for fk in fk_rows.filter_map(|r| r.ok()) {
            let (_, _, ref fk_table, ref fk_from, _, _, _, _) = fk;
            let fk_name = format!("{}_{}_fkey", table, fk_from);
            
            // Get the column number
            let from_cid: i64 = conn.query_row(
                "SELECT cid FROM pragma_table_info(?1) WHERE name = ?2",
                [table.clone(), fk_from.clone()],
                |row| row.get(0)
            ).unwrap_or(0);
            
            conn.execute(
                "INSERT INTO __pg_constraint__ 
                 (oid, conname, contype, conrelid, confrelid, conkey, confkey)
                 VALUES (?1, ?2, 'f', ?3, 
                    (SELECT oid FROM pg_class WHERE relname = ?4), ?5, ?6)",
                (oid_counter, &fk_name, table_oid, fk_table.clone(), from_cid + 1, "1"),
            )?;
            oid_counter += 1;
        }
    }
    
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
