//! Catalog initialization — creating shadow tables and populating pg_type data
//!
//! This module contains the startup functions that create all the hidden system
//! tables (`__pg_meta__`, `__pg_authid__`, `__pg_acl__`, `__pg_rls_policies__`, etc.)
//! and populate the static `pg_type` catalog with PostgreSQL's built-in type OIDs.
//!
//! Call [`init_catalog`] once when the SQLite connection is established, then call
//! [`init_pg_types`] to populate the type registry used by ORM introspection queries.

use anyhow::{Context, Result};
use rusqlite::Connection;

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

    
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_pg_meta_table ON __pg_meta__(table_name)",
        [],
    )
    .context("Failed to create index on __pg_meta__")?;

    

    
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

    // Create __pg_default_acl__ table for ALTER DEFAULT PRIVILEGES
    conn.execute(
        "CREATE TABLE IF NOT EXISTS __pg_default_acl__ (
            defaclrole INTEGER NOT NULL,
            defaclnamespace INTEGER,
            defaclobjtype TEXT NOT NULL,
            defaclacl TEXT NOT NULL,
            PRIMARY KEY (defaclrole, defaclnamespace, defaclobjtype)
        )",
        [],
    )
    .context("Failed to create __pg_default_acl__ table")?;

    // Create __pg_description__ table for COMMENT ON statements
    conn.execute(
        "CREATE TABLE IF NOT EXISTS __pg_description__ (
            objoid INTEGER NOT NULL,
            classoid INTEGER NOT NULL,
            objsubid INTEGER DEFAULT 0,
            description TEXT NOT NULL,
            PRIMARY KEY (objoid, classoid, objsubid)
        )",
        [],
    )
    .context("Failed to create __pg_description__ table")?;

    
    conn.execute(
        "CREATE TABLE IF NOT EXISTS __pg_relation_meta__ (
            relname TEXT PRIMARY KEY,
            relowner INTEGER NOT NULL
        )",
        [],
    )
    .context("Failed to create __pg_relation_meta__ table")?;

    
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

    
    conn.execute(
        "CREATE TABLE IF NOT EXISTS __pg_rls_enabled__ (
            relname TEXT PRIMARY KEY,
            rls_enabled BOOLEAN DEFAULT FALSE,
            rls_forced BOOLEAN DEFAULT FALSE
        )",
        [],
    )
    .context("Failed to create __pg_rls_enabled__ table")?;

    
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

    
    conn.execute(
        "INSERT OR IGNORE INTO __pg_authid__ (oid, rolname, rolsuper, rolinherit, rolcreaterole, rolcreatedb, rolcanlogin)
         VALUES (10, 'postgres', 1, 1, 1, 1, 1)",
        [],
    )?;

    
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

    
    init_pg_types(conn)?;

    
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

    
    conn.execute(
        "INSERT OR IGNORE INTO __pg_extension__ (oid, extname, extversion) VALUES
         (1, 'plpgsql', '1.0'),
         (2, 'uuid-ossp', '1.1'),
         (3, 'pg_trgm', '1.6'),
         (4, 'pgcrypto', '1.3')",
        [],
    )?;

    
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

    
    conn.execute(
        "CREATE TABLE IF NOT EXISTS __pg_functions__ (
            oid INTEGER PRIMARY KEY AUTOINCREMENT,
            funcname TEXT NOT NULL,
            schema_name TEXT DEFAULT 'public',
            arg_types TEXT,                    -- JSON array: [\"text\", \"integer\"]
            arg_names TEXT,                    -- JSON array: [\"arg1\", \"arg2\"]
            arg_modes TEXT,                    -- JSON array: [\"IN\", \"OUT\", \"INOUT\", \"VARIADIC\"]
            return_type TEXT NOT NULL,         -- e.g., \"integer\", \"SETOF users\"
            return_type_kind TEXT NOT NULL,    -- \"SCALAR\", \"SETOF\", \"TABLE\", \"VOID\"
            return_table_cols TEXT,            -- JSON: [{\"name\":\"id\",\"type\":\"int\"},...]
            function_body TEXT NOT NULL,       -- The SQL body
            language TEXT DEFAULT 'sql',
            volatility TEXT DEFAULT 'VOLATILE',-- 'IMMUTABLE', 'STABLE', 'VOLATILE'
            strict BOOLEAN DEFAULT FALSE,
            security_definer BOOLEAN DEFAULT FALSE,
            parallel TEXT DEFAULT 'UNSAFE',    -- 'UNSAFE', 'RESTRICTED', 'SAFE'
            owner_oid INTEGER NOT NULL,
            created_at TEXT DEFAULT CURRENT_TIMESTAMP
        )",
        [],
    )
    .context("Failed to create __pg_functions__ table")?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_pg_functions_name ON __pg_functions__(funcname)",
        [],
    )
    .context("Failed to create index on __pg_functions__")?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_pg_functions_schema ON __pg_functions__(schema_name)",
        [],
    )
    .context("Failed to create schema index on __pg_functions__")?;

    Ok(())
}

/// Initialize PostgreSQL type definitions
pub fn init_pg_types(conn: &Connection) -> Result<()> {
    let types = vec![
        
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
