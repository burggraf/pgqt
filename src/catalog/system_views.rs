//! PostgreSQL-compatible system catalog views
//!
//! This module creates and maintains SQLite virtual tables and views that emulate
//! PostgreSQL's system catalog (`pg_catalog` schema). These views are required for
//! compatibility with ORMs and tools like `psql` that issue introspection queries.
//!
//! ## Supported Views
//!
//! | View              | Description                                         |
//! |------------------|-----------------------------------------------------|
//! | `pg_class`        | Tables, indexes, views, sequences                   |
//! | `pg_attribute`    | Column definitions                                  |
//! | `pg_type`         | Data types (100+ PostgreSQL types)                  |
//! | `pg_namespace`    | Schemas                                             |
//! | `pg_index`        | Index metadata                                      |
//! | `pg_constraint`   | Primary keys, foreign keys, unique constraints      |
//! | `pg_roles`        | Users and roles                                     |
//! | `pg_database`     | Database information                                |
//! | `pg_proc`         | Functions                                           |
//! | `pg_settings`     | Server settings                                     |
//! | `pg_tables`       | User-friendly table listing                         |
//! | `pg_views`        | User-friendly view listing                          |
//! | `pg_indexes`      | User-friendly index listing                         |
//! | `pg_extension`    | Installed extensions                                |
//! | `pg_enum`         | Enum values                                         |

use anyhow::Result;
use rusqlite::Connection;

/// Initialize system catalog views to support psql commands like \dt, \d, etc.
pub fn init_system_views(conn: &Connection) -> Result<()> {
    
    conn.execute(
        "CREATE VIEW IF NOT EXISTS pg_namespace AS
         SELECT oid, nspname, nspowner, nspacl
         FROM __pg_namespace__",
        [],
    )?;

    
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
            0 as reltoastidxid,
            0 as reldeltarelid,
            0 as reldeltaidx,
            0 as relcudescrelid,
            0 as relcudescidx,
            CASE WHEN EXISTS (SELECT 1 FROM sqlite_master WHERE tbl_name = sm.name AND type = 'index') THEN true ELSE false END as relhasindex,
            false as relisshared,
            'p' as relpersistence,
            CASE sm.type
                WHEN 'table' THEN 'r'
                WHEN 'view' THEN 'v'
                WHEN 'index' THEN 'i'
                WHEN 'trigger' THEN 'r'
                ELSE 'r'
            END as relkind,
            (SELECT COUNT(*) FROM __pg_attribute__ WHERE attrelid = sm.rowid) as relnatts,
            0 as relchecks,
            false as relhasrules,
            false as relhastriggers,
            false as relhassubclass,
            0 as relcmprs,
            false as relhasclusterkey,
            false as relrowmovement,
            'n' as parttype,
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

    
    conn.execute(
        "CREATE VIEW IF NOT EXISTS pg_type AS
         SELECT
            oid, typname, typnamespace, typowner, typlen, typbyval,
            typtype, typcategory, typispreferred, typisdefined, typdelim,
            typrelid, 0 as typsubscript, typelem, typarray, typinput, typoutput, typreceive,
            typsend, typmodin, typmodout, typanalyze, typalign, typstorage,
            typnotnull, typbasetype, typtypmod, typndims, typcollation,
            typdefaultbin, typdefault, typacl
         FROM __pg_type__",
        [],
    )?;

    
    conn.execute(
        "CREATE VIEW IF NOT EXISTS pg_attribute AS
         SELECT
            attrelid, attname, atttypid, attstattarget, attlen,
            attnum, attndims, attcacheoff, atttypmod, attbyval,
            attstorage, attalign, attnotnull, atthasdef, atthasmissing,
            attidentity, attgenerated, attisdropped, attislocal,
            0 as attcmprmode, attinhcount, attcollation, '' as attcompression, attacl, attoptions, attfdwoptions,
            NULL as attinitdefval, attmissingval
         FROM __pg_attribute__",
        [],
    )?;

    
    conn.execute(
        "CREATE VIEW IF NOT EXISTS pg_attrdef AS
         SELECT oid, adrelid, adnum, adbin, adsrc
         FROM __pg_attrdef__",
        [],
    )?;

    
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

    
    // pg_description view
    conn.execute(
        "CREATE VIEW IF NOT EXISTS pg_description AS
         SELECT 
             objoid,
             classoid,
             objsubid,
             description
         FROM __pg_description__",
        [],
    )?;

    
    conn.execute(
        "CREATE VIEW IF NOT EXISTS pg_roles AS
         SELECT
            oid, rolname, rolsuper, rolinherit, rolcreaterole, rolcreatedb,
            rolcanlogin, -1 as rolconnlimit, NULL as rolvaliduntil,
            false as rolreplication, false as rolbypassrls, NULL as rolconfig
         FROM __pg_authid__",
        [],
    )?;

    
    conn.execute(
        "CREATE VIEW IF NOT EXISTS pg_authid AS
         SELECT * FROM __pg_authid__",
        [],
    )?;

    
    conn.execute(
        "CREATE VIEW IF NOT EXISTS pg_auth_members AS
         SELECT roleid, member, grantor, admin_option FROM __pg_auth_members__",
        [],
    )?;

    // pg_default_acl view
    conn.execute(
        "CREATE VIEW IF NOT EXISTS pg_default_acl AS
         SELECT 
             ROW_NUMBER() OVER (ORDER BY defaclrole, defaclnamespace, defaclobjtype) as oid,
             defaclrole,
             defaclnamespace,
             defaclobjtype,
             defaclacl
         FROM __pg_default_acl__",
        [],
    )?;

    
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

    
    conn.execute(
        "CREATE VIEW IF NOT EXISTS pg_proc AS
         SELECT
            10001 as oid, 'now' as proname, 11 as pronamespace, 10 as proowner,
            13 as prolang, 1.0 as procost, 0.0 as prorows, 0 as provariadic,
            'f' as prokind, false as prosecdef, false as proleakproof,
            true as proisstrict, false as proretset, false as proisagg, false as proiswindow,
            's' as provolatile,
            0 as pronargs, 0 as pronargdefaults, 1184 as prorettype,
            NULL as proargtypes, NULL as proallargtypes, NULL as proargmodes,
            NULL as proargnames, NULL as proargdefaults, NULL as protrftypes,
            'now' as prosrc, NULL as probin, NULL as prosqlbody, NULL as proconfig, NULL as proacl,
            'timestamp with time zone' as proresult
         UNION ALL
         SELECT 10002, 'current_timestamp', 11, 10, 13, 1.0, 0.0, 0, 'f', false,
                false, true, false, false, false,
                's', 0, 0, 1184, NULL, NULL, NULL, NULL, NULL,
                NULL, 'now', NULL, NULL, NULL, NULL, 'timestamp with time zone'
         UNION ALL
         SELECT 10003, 'current_date', 11, 10, 13, 1.0, 0.0, 0, 'f', false,
                false, true, false, false, false,
                's', 0, 0, 1082, NULL, NULL, NULL, NULL, NULL,
                NULL, 'current_date', NULL, NULL, NULL, NULL, 'date'
         UNION ALL
         SELECT 10004, 'current_time', 11, 10, 13, 1.0, 0.0, 0, 'f', false,
                false, true, false, false, false,
                's', 0, 0, 1266, NULL, NULL, NULL, NULL, NULL,
                NULL, 'current_time', NULL, NULL, NULL, NULL, 'time with time zone'
         UNION ALL
         SELECT
            f.oid, f.funcname as proname, n.oid as pronamespace, f.owner_oid as proowner,
            13 as prolang, 1.0 as procost, 0.0 as prorows, 0 as provariadic,
            'f' as prokind, f.security_definer as prosecdef, false as proleakproof,
            f.strict as proisstrict, (f.return_type_kind = 'SETOF') as proretset,
            false as proisagg, false as proiswindow,
            CASE f.volatility WHEN 'IMMUTABLE' THEN 'i' WHEN 'STABLE' THEN 's' ELSE 'v' END as provolatile,
            (SELECT COUNT(*) FROM json_each(f.arg_types)) as pronargs, 0 as pronargdefaults,
            COALESCE((SELECT oid FROM __pg_type__ WHERE typname = f.return_type), 25) as prorettype,
            f.arg_types as proargtypes, NULL as proallargtypes, f.arg_modes as proargmodes,
            f.arg_names as proargnames, NULL as proargdefaults, NULL as protrftypes,
            f.function_body as prosrc, NULL as probin, NULL as prosqlbody, NULL as proconfig, NULL as proacl,
            CASE 
                WHEN f.return_type_kind = 'SETOF' THEN 'SETOF ' || f.return_type
                WHEN f.return_type_kind = 'TABLE' THEN 'TABLE'
                WHEN f.return_type_kind = 'VOID' THEN 'void'
                ELSE f.return_type
            END as proresult
         FROM __pg_functions__ f
         JOIN pg_namespace n ON f.schema_name = n.nspname",
        [],
    )?;

    conn.execute(
        "CREATE VIEW IF NOT EXISTS pg_trigger AS
         SELECT
            sm.rowid as oid,
            (SELECT rowid FROM sqlite_master WHERE name = sm.tbl_name AND type IN ('table', 'view')) as tgrelid,
            0 as tgparentid,
            sm.name as tgname,
            0 as tgfoid,
            0 as tgtype,
            'O' as tgenabled,
            false as tgisinternal,
            0 as tgconstrrelid,
            0 as tgconstrindid,
            0 as tgconstraint,
            false as tgdeferrable,
            false as tginitdeferred,
            0 as tgnargs,
            '' as tgattr,
            '' as tgargs,
            NULL as tgqual,
            NULL as tgoldtable,
            NULL as tgnewtable
         FROM sqlite_master sm
         WHERE sm.type = 'trigger'",
        [],
    )?;

    
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

    
    conn.execute(
        "CREATE VIEW IF NOT EXISTS pg_extension AS
         SELECT oid, extname, extowner, extnamespace, extrelocatable,
                extversion, extconfig, extcondition
         FROM __pg_extension__",
        [],
    )?;

    
    conn.execute(
        "CREATE VIEW IF NOT EXISTS pg_enum AS
         SELECT oid, enumtypid, enumsortorder, enumlabel
         FROM __pg_enum__",
        [],
    )?;

    
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
