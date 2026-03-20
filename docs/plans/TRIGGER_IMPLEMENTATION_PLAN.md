# Trigger Implementation Plan for PGQT

## Executive Summary

This document outlines the implementation of PostgreSQL-compatible triggers in PGQT. Triggers will enable automatic execution of PL/pgSQL functions in response to INSERT, UPDATE, DELETE operations.

**Status:** Planning Phase  
**Estimated Effort:** 2-3 development sessions  
**Priority:** High - Completes core PostgreSQL functionality  

---

## Current State

### What Already Exists
1. **PL/pgSQL Runtime** - Fully functional via Lua transpilation
2. **Catalog Infrastructure** - `__pg_functions__` table for function storage
3. **Trigger Function Stub** - `execute_plpgsql_trigger()` in `src/plpgsql/mod.rs` (returns error)
4. **Basic CREATE TRIGGER Parsing** - `src/transpiler/mod.rs` line 440 (currently ignored)
5. **Session Context** - Per-connection session management

### What's Missing
1. CREATE/DROP TRIGGER statement handling
2. Trigger metadata storage in catalog
3. Hook into INSERT/UPDATE/DELETE execution
4. OLD/NEW row passing to trigger functions
5. BEFORE/AFTER timing support
6. FOR EACH ROW/FOR EACH STATEMENT support

---

## Architecture Design

### Components

```
┌─────────────────────────────────────────────────────────────┐
│                     Trigger System                           │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  1. SQL Parsing Layer                                        │
│     ├── CREATE TRIGGER → TriggerMetadata                     │
│     └── DROP TRIGGER → Removal                              │
│                                                              │
│  2. Catalog Storage                                          │
│     ├── __pg_triggers__ table (new)                         │
│     ├── TriggerMetadata struct                              │
│     └── Index by table_name + timing                        │
│                                                              │
│  3. Execution Hooks                                          │
│     ├── Pre-execution: Check for BEFORE triggers            │
│     ├── Post-execution: Check for AFTER triggers            │
│     └── Execute trigger function with OLD/NEW               │
│                                                              │
│  4. PL/pgSQL Integration                                     │
│     ├── Build OLD/NEW row as HashMap                        │
│     ├── Execute trigger function                            │
│     └── Handle return values (BEFORE only)                  │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

### Data Flow

```
INSERT INTO users (id, name) VALUES (1, 'Alice')
         │
         ▼
┌─────────────────┐
│ Transpile to    │
│ SQLite INSERT   │
└────────┬────────┘
         │
         ▼
┌─────────────────┐     ┌─────────────────┐
│ Check triggers  │────▶│ BEFORE INSERT   │
│ for users table │     │ triggers?       │
└────────┬────────┘     └────────┬────────┘
         │                       │
         │              ┌────────▼────────┐
         │              │ Execute trigger │
         │              │ with NEW = {id: │
         │              │ 1, name: 'Alice'}│
         │              └────────┬────────┘
         │                       │
         │              ┌────────▼────────┐
         │              │ Modify NEW row? │
         │              │ (BEFORE only)   │
         │              └────────┬────────┘
         │                       │
         ▼                       ▼
┌─────────────────────────────────────────┐
│ Execute SQLite INSERT with (possibly)   │
│ modified values                         │
└────────┬────────────────────────────────┘
         │
         ▼
┌─────────────────┐     ┌─────────────────┐
│ Check triggers  │────▶│ AFTER INSERT    │
│ for users table │     │ triggers?       │
└────────┬────────┘     └────────┬────────┘
         │                       │
         │              ┌────────▼────────┐
         │              │ Execute trigger │
         │              │ with NEW = {id: │
         │              │ 1, name: 'Alice'}│
         │              └─────────────────┘
         │
         ▼
┌─────────────────┐
│ Return result   │
│ to client       │
└─────────────────┘
```

---

## Implementation Phases

### Phase 1: Catalog & Metadata (Session 1)

**Goal:** Store trigger metadata

#### 1.1 Create `__pg_triggers__` Catalog Table
```sql
CREATE TABLE IF NOT EXISTS __pg_triggers__ (
    oid INTEGER PRIMARY KEY,
    tgname TEXT NOT NULL,           -- trigger name
    tgrelid INTEGER NOT NULL,       -- table OID
    tgtype INTEGER NOT NULL,        -- trigger type (timing + events)
    tgenabled BOOLEAN NOT NULL DEFAULT TRUE,
    tgisinternal BOOLEAN NOT NULL DEFAULT FALSE,
    tgconstraint INTEGER,           -- constraint OID (if constraint trigger)
    tgdeferrable BOOLEAN,
    tginitdeferred BOOLEAN,
    tgnargs INTEGER DEFAULT 0,      -- number of arguments
    tgargs TEXT,                    -- trigger arguments (space-separated)
    function_oid INTEGER NOT NULL,  -- function to execute
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(tgname, tgrelid)
);
```

**Files:**
- `src/catalog/mod.rs` - Add initialization
- `src/catalog/trigger.rs` (NEW) - CRUD operations

#### 1.2 Define TriggerMetadata Struct
```rust
#[derive(Debug, Clone)]
pub struct TriggerMetadata {
    pub oid: i64,
    pub name: String,
    pub table_oid: i64,
    pub timing: TriggerTiming,       // BEFORE, AFTER, INSTEAD OF
    pub events: Vec<TriggerEvent>,   // INSERT, UPDATE, DELETE, TRUNCATE
    pub row_or_statement: RowOrStatement, // ROW or STATEMENT
    pub enabled: bool,
    pub function_oid: i64,
    pub function_name: String,       // for convenience
    pub args: Vec<String>,           // trigger arguments
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriggerTiming {
    Before,
    After,
    InsteadOf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriggerEvent {
    Insert,
    Update,
    Delete,
    Truncate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowOrStatement {
    Row,
    Statement,
}
```

**Files:**
- `src/catalog/mod.rs` or `src/catalog/trigger.rs`

#### 1.3 Catalog Functions
```rust
// Store trigger metadata
pub fn store_trigger(conn: &Connection, metadata: &TriggerMetadata) -> Result<()>;

// Get triggers for a table (filtered by timing/event)
pub fn get_triggers_for_table(
    conn: &Connection, 
    table_oid: i64,
    timing: TriggerTiming,
    event: TriggerEvent
) -> Result<Vec<TriggerMetadata>>;

// Drop trigger
pub fn drop_trigger(conn: &Connection, trigger_name: &str, table_oid: i64) -> Result<()>;

// Enable/disable trigger
pub fn set_trigger_enabled(conn: &Connection, trigger_name: &str, table_oid: i64, enabled: bool) -> Result<()>;
```

**Files:**
- `src/catalog/trigger.rs`

---

### Phase 2: SQL Parsing (Session 1-2)

**Goal:** Parse CREATE TRIGGER / DROP TRIGGER

#### 2.1 CREATE TRIGGER Parsing

**PostgreSQL Syntax:**
```sql
CREATE [ OR REPLACE ] [ CONSTRAINT ] TRIGGER name
    { BEFORE | AFTER | INSTEAD OF } { event [ OR ... ] }
    ON table_name
    [ FROM referenced_table_name ]
    [ NOT DEFERRABLE | [ DEFERRABLE ] [ INITIALLY IMMEDIATE | INITIALLY DEFERRED ] ]
    [ REFERENCING { { OLD | NEW } TABLE [ AS ] transition_relation_name } [ ... ] ]
    [ FOR [ EACH ] { ROW | STATEMENT } ]
    [ WHEN ( condition ) ]
    EXECUTE { FUNCTION | PROCEDURE } function_name ( arguments )

event:
    INSERT | UPDATE [ OF column_name [, ... ] ] | DELETE | TRUNCATE
```

**Implementation:**
- Extend `src/transpiler/ddl.rs` to handle CREATE TRIGGER
- Parse pg_query AST for CreateTrigStmt
- Validate function exists in catalog
- Store in `__pg_triggers