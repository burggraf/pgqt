-- PostgreSQL Regression Test Tables
-- These tables are expected by the PostgreSQL regression test suite

-- tenk1: 10,000 row test table
CREATE TABLE IF NOT EXISTS tenk1 (
    unique1 INTEGER,
    unique2 INTEGER,
    two INTEGER,
    four INTEGER,
    ten INTEGER,
    twenty INTEGER,
    hundred INTEGER,
    thousand INTEGER,
    twothousand INTEGER,
    fivethous INTEGER,
    tenthous INTEGER,
    odd INTEGER,
    even INTEGER,
    stringu1 TEXT,
    stringu2 TEXT,
    string4 TEXT
);

-- onek: 1,000 row test table
CREATE TABLE IF NOT EXISTS onek (
    unique1 INTEGER,
    unique2 INTEGER,
    two INTEGER,
    four INTEGER,
    ten INTEGER,
    twenty INTEGER,
    hundred INTEGER,
    thousand INTEGER,
    twothousand INTEGER,
    fivethous INTEGER,
    tenthous INTEGER,
    odd INTEGER,
    even INTEGER,
    stringu1 TEXT,
    stringu2 TEXT,
    string4 TEXT
);

-- onek2: Another 1,000 row test table
CREATE TABLE IF NOT EXISTS onek2 (
    unique1 INTEGER,
    unique2 INTEGER,
    two INTEGER,
    four INTEGER,
    ten INTEGER,
    twenty INTEGER,
    hundred INTEGER,
    thousand INTEGER,
    twothousand INTEGER,
    fivethous INTEGER,
    tenthous INTEGER,
    odd INTEGER,
    even INTEGER,
    stringu1 TEXT,
    stringu2 TEXT,
    string4 TEXT
);

-- int8_tbl: 64-bit integer test table
CREATE TABLE IF NOT EXISTS int8_tbl (
    q1 BIGINT,
    q2 BIGINT
);

-- int4_tbl: 32-bit integer test table
CREATE TABLE IF NOT EXISTS int4_tbl (
    f1 INTEGER
);

-- int2_tbl: 16-bit integer test table
CREATE TABLE IF NOT EXISTS int2_tbl (
    f1 SMALLINT
);

-- arrtest: Array test table
CREATE TABLE IF NOT EXISTS arrtest (
    a TEXT,  -- Stored as JSON array
    b TEXT   -- Stored as JSON array
);

-- aggtest: Aggregate test table
CREATE TABLE IF NOT EXISTS aggtest (
    a INTEGER,
    b INTEGER
);

-- testjsonb: JSONB test table
CREATE TABLE IF NOT EXISTS testjsonb (
    id INTEGER,
    data TEXT  -- Stored as JSON
);

-- jsonb_populate_record test table
CREATE TABLE IF NOT EXISTS jsonb_populate_record (
    a INTEGER,
    b TEXT,
    c INTEGER
);

-- json_populate_record test table
CREATE TABLE IF NOT EXISTS json_populate_record (
    a INTEGER,
    b TEXT,
    c INTEGER
);

-- varchar_tbl: VARCHAR test table
CREATE TABLE IF NOT EXISTS varchar_tbl (
    f1 TEXT
);

-- point_tbl: Geometric point test table
CREATE TABLE IF NOT EXISTS point_tbl (
    f1 TEXT  -- Stored as point representation
);

-- Create indexes for common queries
CREATE INDEX IF NOT EXISTS idx_tenk1_unique1 ON tenk1(unique1);
CREATE INDEX IF NOT EXISTS idx_tenk1_unique2 ON tenk1(unique2);
CREATE INDEX IF NOT EXISTS idx_onek_unique1 ON onek(unique1);
CREATE INDEX IF NOT EXISTS idx_onek_unique2 ON onek(unique2);
