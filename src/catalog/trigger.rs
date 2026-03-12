//! Trigger metadata storage and retrieval
//!
//! This module handles persistence of trigger definitions in the `__pg_triggers__`
//! shadow table. Triggers created via `CREATE TRIGGER` are stored here so they
//! survive across connections and can be retrieved for execution.
//!
//! ## Key Functions
//! - [`store_trigger`] — Persist a trigger definition to the catalog
//! - [`get_trigger`] — Look up a trigger by name and table
//! - [`get_triggers_for_table`] — Get all triggers for a table (filtered by timing/event)
//! - [`drop_trigger`] — Remove a trigger from the catalog

use anyhow::Result;
use rusqlite::Connection;
use serde_json;

use super::{TriggerMetadata, TriggerTiming, TriggerEvent, RowOrStatement};

/// Store a trigger definition in the catalog
pub fn store_trigger(conn: &Connection, metadata: &TriggerMetadata) -> Result<i64> {
    // Encode events as a bitmask for efficient filtering
    let tgtype = encode_tgtype(&metadata.timing, &metadata.events, &metadata.row_or_statement);
    
    let events_json = serde_json::to_string(&metadata.events)?;
    let args_json = serde_json::to_string(&metadata.args)?;

    conn.execute(
        "INSERT INTO __pg_triggers__ 
         (tgname, tgrelid, tgtype, tgenabled, tgisinternal, 
          tgconstraint, tgdeferrable, tginitdeferred, 
          tgnargs, tgargs, function_oid, function_name)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        (
            &metadata.name,
            &metadata.table_oid,
            tgtype,
            metadata.enabled,
            metadata.is_internal,
            metadata.is_constraint.then(|| 1i64).unwrap_or(0),
            metadata.deferrable,
            metadata.initially_deferred,
            metadata.args.len() as i64,
            &args_json,
            &metadata.function_oid,
            &metadata.function_name,
        ),
    )?;

    let oid: i64 = conn.query_row(
        "SELECT last_insert_rowid()",
        [],
        |row| row.get(0),
    )?;

    Ok(oid)
}

/// Retrieve trigger metadata by name and table
pub fn get_trigger(
    conn: &Connection,
    trigger_name: &str,
    table_oid: i64,
) -> Result<Option<TriggerMetadata>> {
    let mut stmt = conn.prepare(
        "SELECT oid, tgname, tgrelid, tgtype, tgenabled, tgisinternal, 
                tgconstraint, tgdeferrable, tginitdeferred, 
                tgnargs, tgargs, function_oid, function_name
         FROM __pg_triggers__ WHERE tgname = ? AND tgrelid = ?"
    )?;
    
    let row_result = stmt.query_row([trigger_name, &table_oid.to_string()], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, i64>(2)?,
            row.get::<_, i64>(3)?,
            row.get::<_, bool>(4)?,
            row.get::<_, bool>(5)?,
            row.get::<_, i64>(6)?,
            row.get::<_, bool>(7)?,
            row.get::<_, bool>(8)?,
            row.get::<_, i64>(9)?,
            row.get::<_, String>(10)?,
            row.get::<_, i64>(11)?,
            row.get::<_, String>(12)?,
        ))
    });

    match row_result {
        Ok((oid, name, table_oid, tgtype, enabled, is_internal, 
            constraint, deferrable, initdeferred, nargs, args_json, 
            function_oid, function_name)) => {
            let (timing, events, row_or_statement) = decode_tgtype(tgtype);
            let args: Vec<String> = serde_json::from_str(&args_json)?;
            
            Ok(Some(TriggerMetadata {
                oid,
                name,
                table_oid,
                table_name: String::new(), // Will be filled in by caller if needed
                timing,
                events,
                row_or_statement,
                enabled,
                function_oid,
                function_name,
                args,
                is_internal,
                is_constraint: constraint != 0,
                deferrable,
                initially_deferred: initdeferred,
            }))
        }
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Get all triggers for a table, optionally filtered by timing and event
pub fn get_triggers_for_table(
    conn: &Connection,
    table_oid: i64,
    timing: Option<TriggerTiming>,
    event: Option<TriggerEvent>,
) -> Result<Vec<TriggerMetadata>> {
    let mut query = String::from(
        "SELECT oid, tgname, tgrelid, tgtype, tgenabled, tgisinternal, 
                tgconstraint, tgdeferrable, tginitdeferred, 
                tgnargs, tgargs, function_oid, function_name
         FROM __pg_triggers__ WHERE tgrelid = ? AND tgenabled = 1"
    );
    
    let mut params: Vec<rusqlite::types::Value> = vec![table_oid.into()];
    
    // Add timing filter if specified
    // Timing bits:
    // Before: 0x02 (bit 1)
    // After: 0x04 (bit 2)
    // InsteadOf: 0x40 (bit 6)
    if let Some(t) = timing {
        let timing_bits = match t {
            TriggerTiming::Before => 0x02,
            TriggerTiming::After => 0x04,
            TriggerTiming::InsteadOf => 0x40,
        };
        query.push_str(&format!(" AND (tgtype & {}) != 0", timing_bits));
    }
    
    // Add event filter if specified
    // Event bits (matching pg_query values):
    // Insert: 0x04 (bit 2)
    // Delete: 0x08 (bit 3)
    // Update: 0x10 (bit 4)
    // Truncate: 0x80 (bit 7)
    if let Some(e) = event {
        let event_bits = match e {
            TriggerEvent::Insert => 0x04,
            TriggerEvent::Delete => 0x08,
            TriggerEvent::Update => 0x10,
            TriggerEvent::Truncate => 0x80,
        };
        query.push_str(&format!(" AND (tgtype & {}) != 0", event_bits));
    }
    
    query.push_str(" ORDER BY oid");
    
    let mut stmt = conn.prepare(&query)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(params.iter()), |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, i64>(2)?,
            row.get::<_, i64>(3)?,
            row.get::<_, bool>(4)?,
            row.get::<_, bool>(5)?,
            row.get::<_, i64>(6)?,
            row.get::<_, bool>(7)?,
            row.get::<_, bool>(8)?,
            row.get::<_, i64>(9)?,
            row.get::<_, String>(10)?,
            row.get::<_, i64>(11)?,
            row.get::<_, String>(12)?,
        ))
    })?;

    let mut triggers = Vec::new();
    for row in rows {
        let (oid, name, table_oid, tgtype, enabled, is_internal, 
             constraint, deferrable, initdeferred, nargs, args_json, 
             function_oid, function_name) = row?;
        
        let (timing, events, row_or_statement) = decode_tgtype(tgtype);
        let args: Vec<String> = serde_json::from_str(&args_json)?;
        
        triggers.push(TriggerMetadata {
            oid,
            name,
            table_oid,
            table_name: String::new(),
            timing,
            events,
            row_or_statement,
            enabled,
            function_oid,
            function_name,
            args,
            is_internal,
            is_constraint: constraint != 0,
            deferrable,
            initially_deferred: initdeferred,
        });
    }

    Ok(triggers)
}

/// Drop a trigger from the catalog
pub fn drop_trigger(
    conn: &Connection,
    trigger_name: &str,
    table_oid: i64,
) -> Result<bool> {
    let mut stmt = conn.prepare(
        "DELETE FROM __pg_triggers__ WHERE tgname = ? AND tgrelid = ?"
    )?;
    
    let changes = stmt.execute([trigger_name, &table_oid.to_string()])?;
    Ok(changes > 0)
}

/// Calculate a table OID from table name (using hash)
/// This is a simple way to get a consistent OID for a table name
pub fn calc_table_oid(table_name: &str) -> i64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    
    let mut hasher = DefaultHasher::new();
    table_name.hash(&mut hasher);
    let hash = hasher.finish();
    
    // Use positive i64 range
    (hash as i64).abs()
}

/// Enable or disable a trigger
pub fn set_trigger_enabled(
    conn: &Connection,
    trigger_name: &str,
    table_oid: i64,
    enabled: bool,
) -> Result<bool> {
    let mut stmt = conn.prepare(
        "UPDATE __pg_triggers__ SET tgenabled = ? WHERE tgname = ? AND tgrelid = ?"
    )?;
    
    let changes = stmt.execute([
        rusqlite::types::Value::from(enabled),
        rusqlite::types::Value::from(trigger_name.to_string()),
        rusqlite::types::Value::from(table_oid),
    ])?;
    
    Ok(changes > 0)
}

/// Encode trigger metadata into PostgreSQL-compatible tgtype bitmask
/// 
/// PostgreSQL tgtype format (from pg_trigger.h):
/// - Bit 0 (0x01): Row-level trigger (1=row, 0=statement)
/// - Bit 1 (0x02): BEFORE trigger
/// - Bit 2 (0x04): AFTER trigger
/// - Bit 3 (0x08): INSERT event
/// - Bit 4 (0x10): DELETE event
/// - Bit 5 (0x20): UPDATE event
/// - Bit 6 (0x40): TRUNCATE event (also INSTEAD OF for views)
/// - Bit 7 (0x80): Reserved/used for other purposes
fn encode_tgtype(
    timing: &TriggerTiming,
    events: &[TriggerEvent],
    row_or_stmt: &RowOrStatement,
) -> i64 {
    let mut tgtype: i64 = 0;
    
    // Timing bits
    match timing {
        TriggerTiming::Before => tgtype |= 0x02,      // Bit 1
        TriggerTiming::After => tgtype |= 0x04,       // Bit 2
        TriggerTiming::InsteadOf => tgtype |= 0x40,   // Bit 6
    }
    
    // Row vs Statement
    match row_or_stmt {
        RowOrStatement::Row => tgtype |= 0x01,        // Bit 0 (row-level)
        RowOrStatement::Statement => (),              // 0 = statement-level
    }
    
    // Event bits (matching pg_query values)
    for event in events {
        match event {
            TriggerEvent::Insert => tgtype |= 0x04,   // Bit 2 (INSERT = 4)
            TriggerEvent::Delete => tgtype |= 0x08,   // Bit 3 (DELETE = 8)
            TriggerEvent::Update => tgtype |= 0x10,   // Bit 4 (UPDATE = 16)
            TriggerEvent::Truncate => tgtype |= 0x80, // Bit 7 (TRUNCATE = 128)
        }
    }
    
    tgtype
}

/// Decode PostgreSQL tgtype bitmask into trigger metadata
fn decode_tgtype(tgtype: i64) -> (TriggerTiming, Vec<TriggerEvent>, RowOrStatement) {
    // Timing
    let timing = if tgtype & 0x02 != 0 {
        TriggerTiming::Before
    } else if tgtype & 0x04 != 0 {
        TriggerTiming::After
    } else {
        TriggerTiming::InsteadOf
    };
    
    // Row vs Statement
    let row_or_statement = if tgtype & 0x01 != 0 {
        RowOrStatement::Row
    } else {
        RowOrStatement::Statement
    };
    
    // Events (matching pg_query values)
    let mut events = Vec::new();
    if tgtype & 0x04 != 0 {
        events.push(TriggerEvent::Insert);
    }
    if tgtype & 0x08 != 0 {
        events.push(TriggerEvent::Delete);
    }
    if tgtype & 0x10 != 0 {
        events.push(TriggerEvent::Update);
    }
    if tgtype & 0x80 != 0 {
        events.push(TriggerEvent::Truncate);
    }
    
    (timing, events, row_or_statement)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{init_catalog, TriggerTiming, TriggerEvent, RowOrStatement};
    use rusqlite::Connection;

    fn setup_test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        init_catalog(&conn).unwrap();
        conn
    }

    fn create_test_trigger_metadata(name: &str, table_oid: i64) -> TriggerMetadata {
        TriggerMetadata {
            oid: 0,
            name: name.to_string(),
            table_oid,
            table_name: "test_table".to_string(),
            timing: TriggerTiming::Before,
            events: vec![TriggerEvent::Insert],
            row_or_statement: RowOrStatement::Row,
            enabled: true,
            function_oid: 1,
            function_name: "test_func".to_string(),
            args: vec![],
            is_internal: false,
            is_constraint: false,
            deferrable: false,
            initially_deferred: false,
        }
    }

    #[test]
    fn test_store_and_retrieve_trigger() {
        let conn = setup_test_db();
        let metadata = create_test_trigger_metadata("test_trigger", 12345);

        // Store trigger
        let oid = store_trigger(&conn, &metadata).unwrap();
        assert!(oid > 0);

        // Retrieve trigger
        let retrieved = get_trigger(&conn, "test_trigger", 12345).unwrap();
        assert!(retrieved.is_some());
        
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.name, "test_trigger");
        assert_eq!(retrieved.table_oid, 12345);
        assert_eq!(retrieved.timing, TriggerTiming::Before);
        assert_eq!(retrieved.events.len(), 1);
        assert_eq!(retrieved.events[0], TriggerEvent::Insert);
        assert_eq!(retrieved.row_or_statement, RowOrStatement::Row);
        assert_eq!(retrieved.function_name, "test_func");
    }

    #[test]
    fn test_get_triggers_for_table() {
        let conn = setup_test_db();
        
        // Create multiple triggers
        let trigger1 = create_test_trigger_metadata("trigger1", 100);
        let trigger2 = TriggerMetadata {
            oid: 0,
            name: "trigger2".to_string(),
            table_oid: 100,
            table_name: "test_table".to_string(),
            timing: TriggerTiming::After,
            events: vec![TriggerEvent::Update],
            row_or_statement: RowOrStatement::Row,
            enabled: true,
            function_oid: 2,
            function_name: "func2".to_string(),
            args: vec!["arg1".to_string()],
            is_internal: false,
            is_constraint: false,
            deferrable: false,
            initially_deferred: false,
        };
        
        store_trigger(&conn, &trigger1).unwrap();
        store_trigger(&conn, &trigger2).unwrap();

        // Get all triggers for table 100
        let triggers = get_triggers_for_table(&conn, 100, None, None).unwrap();
        assert_eq!(triggers.len(), 2);

        // Filter by timing
        let before_triggers = get_triggers_for_table(&conn, 100, Some(TriggerTiming::Before), None).unwrap();
        assert_eq!(before_triggers.len(), 1);
        assert_eq!(before_triggers[0].name, "trigger1");

        // Filter by event
        let update_triggers = get_triggers_for_table(&conn, 100, None, Some(TriggerEvent::Update)).unwrap();
        assert_eq!(update_triggers.len(), 1);
        assert_eq!(update_triggers[0].name, "trigger2");
    }

    #[test]
    fn test_drop_trigger() {
        let conn = setup_test_db();
        let metadata = create_test_trigger_metadata("drop_test", 200);

        store_trigger(&conn, &metadata).unwrap();
        
        // Verify trigger exists
        assert!(get_trigger(&conn, "drop_test", 200).unwrap().is_some());

        // Drop trigger
        let dropped = drop_trigger(&conn, "drop_test", 200).unwrap();
        assert!(dropped);

        // Verify trigger is gone
        assert!(get_trigger(&conn, "drop_test", 200).unwrap().is_none());

        // Dropping non-existent trigger returns false
        let dropped = drop_trigger(&conn, "nonexistent", 200).unwrap();
        assert!(!dropped);
    }

    #[test]
    fn test_set_trigger_enabled() {
        let conn = setup_test_db();
        let metadata = create_test_trigger_metadata("enable_test", 300);

        store_trigger(&conn, &metadata).unwrap();
        
        // Verify trigger is enabled by default
        let trigger = get_trigger(&conn, "enable_test", 300).unwrap().unwrap();
        assert!(trigger.enabled);

        // Disable trigger
        let updated = set_trigger_enabled(&conn, "enable_test", 300, false).unwrap();
        assert!(updated);

        // Verify trigger is disabled
        let trigger = get_trigger(&conn, "enable_test", 300).unwrap().unwrap();
        assert!(!trigger.enabled);

        // Re-enable trigger
        set_trigger_enabled(&conn, "enable_test", 300, true).unwrap();
        let trigger = get_trigger(&conn, "enable_test", 300).unwrap().unwrap();
        assert!(trigger.enabled);
    }

    #[test]
    fn test_trigger_with_args() {
        let conn = setup_test_db();
        let mut metadata = create_test_trigger_metadata("args_test", 400);
        metadata.args = vec!["arg1".to_string(), "arg2".to_string()];

        let oid = store_trigger(&conn, &metadata).unwrap();
        
        let retrieved = get_trigger(&conn, "args_test", 400).unwrap().unwrap();
        assert_eq!(retrieved.args.len(), 2);
        assert_eq!(retrieved.args[0], "arg1");
        assert_eq!(retrieved.args[1], "arg2");
    }

    #[test]
    fn test_calc_table_oid() {
        let oid1 = calc_table_oid("users");
        let oid2 = calc_table_oid("users");
        let oid3 = calc_table_oid("orders");

        // Same table name should produce same OID
        assert_eq!(oid1, oid2);
        
        // Different table names should produce different OIDs (with high probability)
        assert_ne!(oid1, oid3);
    }

    #[test]
    fn test_trigger_events_multiple() {
        let conn = setup_test_db();
        let metadata = TriggerMetadata {
            oid: 0,
            name: "multi_event".to_string(),
            table_oid: 500,
            table_name: "test_table".to_string(),
            timing: TriggerTiming::Before,
            events: vec![TriggerEvent::Insert, TriggerEvent::Update, TriggerEvent::Delete],
            row_or_statement: RowOrStatement::Row,
            enabled: true,
            function_oid: 1,
            function_name: "multi_func".to_string(),
            args: vec![],
            is_internal: false,
            is_constraint: false,
            deferrable: false,
            initially_deferred: false,
        };

        store_trigger(&conn, &metadata).unwrap();
        
        let retrieved = get_trigger(&conn, "multi_event", 500).unwrap().unwrap();
        assert_eq!(retrieved.events.len(), 3);
        assert!(retrieved.events.contains(&TriggerEvent::Insert));
        assert!(retrieved.events.contains(&TriggerEvent::Update));
        assert!(retrieved.events.contains(&TriggerEvent::Delete));
    }
}