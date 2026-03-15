#!/usr/bin/env python3
"""
Populate PostgreSQL regression test tables with synthetic data.
This script generates data that matches the patterns expected by the tests.
"""

import psycopg2
import os
import sys

PG_DSN = os.environ.get("PG_DSN", "host=localhost port=5432 user=postgres password=postgres dbname=postgres")

def connect_pg():
    return psycopg2.connect(PG_DSN)

def populate_tenk1(conn):
    """Populate tenk1 with 10,000 rows of deterministic data."""
    cur = conn.cursor()
    
    # Check if already populated
    cur.execute("SELECT COUNT(*) FROM tenk1")
    if cur.fetchone()[0] > 0:
        print("tenk1 already populated, skipping")
        return
    
    print("Populating tenk1...")
    
    # Generate 10,000 rows
    # The pattern follows PostgreSQL's original test data
    values = []
    for i in range(10000):
        unique1 = i
        unique2 = (i * 9301 + 49297) % 10000  # Pseudo-random permutation
        two = unique1 % 2
        four = unique1 % 4
        ten = unique1 % 10
        twenty = unique1 % 20
        hundred = unique1 % 100
        thousand = unique1 % 1000
        twothousand = unique1 % 2000
        fivethous = unique1 % 5000
        tenthous = unique1 % 10000
        odd = (unique1 % 2) * 2 + 1  # 1 or 3
        even = (unique1 % 2) * 2     # 0 or 2
        stringu1 = f"AAAAxx{unique1:05d}"
        stringu2 = f"AAAAxx{unique2:05d}"
        string4 = f"{four:04d}"
        
        values.append((unique1, unique2, two, four, ten, twenty, hundred, 
                      thousand, twothousand, fivethous, tenthous, odd, even,
                      stringu1, stringu2, string4))
    
    # Insert in batches
    batch_size = 1000
    for i in range(0, len(values), batch_size):
        batch = values[i:i+batch_size]
        args_str = ','.join(cur.mogrify("(%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s)", x).decode('utf-8') for x in batch)
        cur.execute(f"INSERT INTO tenk1 VALUES {args_str}")
        conn.commit()
        print(f"  Inserted {min(i+batch_size, 10000)}/10000 rows")
    
    cur.close()
    print("tenk1 populated")

def populate_onek(conn):
    """Populate onek with 1,000 rows (subset of tenk1)."""
    cur = conn.cursor()
    
    cur.execute("SELECT COUNT(*) FROM onek")
    if cur.fetchone()[0] > 0:
        print("onek already populated, skipping")
        return
    
    print("Populating onek...")
    
    # Copy first 1000 rows from tenk1 pattern
    values = []
    for i in range(1000):
        unique1 = i
        unique2 = (i * 9301 + 49297) % 1000
        two = unique1 % 2
        four = unique1 % 4
        ten = unique1 % 10
        twenty = unique1 % 20
        hundred = unique1 % 100
        thousand = unique1 % 1000
        twothousand = unique1 % 2000
        fivethous = unique1 % 5000
        tenthous = unique1 % 10000
        odd = (unique1 % 2) * 2 + 1
        even = (unique1 % 2) * 2
        stringu1 = f"AAAAxx{unique1:05d}"
        stringu2 = f"AAAAxx{unique2:05d}"
        string4 = f"{four:04d}"
        
        values.append((unique1, unique2, two, four, ten, twenty, hundred,
                      thousand, twothousand, fivethous, tenthous, odd, even,
                      stringu1, stringu2, string4))
    
    batch_size = 500
    for i in range(0, len(values), batch_size):
        batch = values[i:i+batch_size]
        args_str = ','.join(cur.mogrify("(%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s)", x).decode('utf-8') for x in batch)
        cur.execute(f"INSERT INTO onek VALUES {args_str}")
        conn.commit()
    
    cur.close()
    print("onek populated")

def populate_int_tables(conn):
    """Populate integer test tables."""
    cur = conn.cursor()
    
    # int8_tbl
    cur.execute("SELECT COUNT(*) FROM int8_tbl")
    if cur.fetchone()[0] == 0:
        print("Populating int8_tbl...")
        cur.execute("""
            INSERT INTO int8_tbl VALUES
            (123, 456),
            (123, 4567890123456789),
            (4567890123456789, 123),
            (4567890123456789, 4567890123456789),
            (-4567890123456789, 4567890123456789)
        """)
        conn.commit()
    
    # int4_tbl
    cur.execute("SELECT COUNT(*) FROM int4_tbl")
    if cur.fetchone()[0] == 0:
        print("Populating int4_tbl...")
        cur.execute("""
            INSERT INTO int4_tbl VALUES
            (0), (123456), (-123456), (2147483647), (-2147483647)
        """)
        conn.commit()
    
    # int2_tbl
    cur.execute("SELECT COUNT(*) FROM int2_tbl")
    if cur.fetchone()[0] == 0:
        print("Populating int2_tbl...")
        cur.execute("""
            INSERT INTO int2_tbl VALUES
            (0), (1234), (-1234), (32767), (-32767)
        """)
        conn.commit()
    
    cur.close()

def main():
    print("Connecting to PostgreSQL...")
    conn = connect_pg()
    
    print("\nPopulating test tables...")
    populate_tenk1(conn)
    populate_onek(conn)
    populate_int_tables(conn)
    
    conn.close()
    print("\nDone!")

if __name__ == "__main__":
    main()
