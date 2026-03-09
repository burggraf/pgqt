#!/usr/bin/env python3
"""
End-to-end tests for password authentication.

This test suite validates password authentication through the PostgreSQL wire protocol:
- Creating users with passwords
- Logging in with correct passwords
- Rejecting incorrect passwords
- Rejecting missing passwords
- Trust mode bypass
"""

import sys
import os
import psycopg2
import pytest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

from e2e_helper import run_e2e_test, ProxyManager

def test_create_user_with_password(proxy):
    """Test that CREATE USER stores hashed password."""
    conn = proxy.get_connection()
    cur = conn.cursor()
    
    # Create user with password
    cur.execute("CREATE USER testuser WITH PASSWORD 'testpass'")
    conn.commit()
    
    # Verify user exists with password hash (should be md5...)
    cur.execute("SELECT rolname, rolpassword FROM __pg_authid__ WHERE rolname = 'testuser'")
    row = cur.fetchone()
    
    assert row is not None, "User should exist"
    assert row[0] == 'testuser', f"Expected username 'testuser', got '{row[0]}'"
    assert row[1] is not None, "Password should be set"
    assert row[1].startswith('md5'), f"Password should be hashed with md5, got '{row[1]}'"
    
    print("✓ CREATE USER stores hashed password")
    
    cur.close()
    conn.close()

def test_login_with_correct_password(proxy):
    """Test that login succeeds with correct password."""
    conn = proxy.get_connection()
    cur = conn.cursor()
    
    # Create user with password
    cur.execute("CREATE USER testuser WITH PASSWORD 'testpass'")
    conn.commit()
    cur.close()
    conn.close()
    
    # Connect with correct password - should succeed
    conn2 = proxy.get_connection(user="testuser", password="testpass")
    cur2 = conn2.cursor()
    cur2.execute("SELECT current_user")
    row = cur2.fetchone()
    assert row[0] == 'testuser', f"Expected current_user 'testuser', got '{row[0]}'"
    
    print("✓ Login with correct password succeeds")
    
    cur2.close()
    conn2.close()

def test_login_with_wrong_password(proxy):
    """Test that login fails with incorrect password."""
    conn = proxy.get_connection()
    cur = conn.cursor()
    
    # Create user with password
    cur.execute("CREATE USER testuser WITH PASSWORD 'testpass'")
    conn.commit()
    cur.close()
    conn.close()
    
    # Connect with wrong password - should fail
    try:
        conn2 = proxy.get_connection(user="testuser", password="wrongpass")
        conn2.close()
        assert False, "Should have failed with wrong password"
    except psycopg2.OperationalError as e:
        # Expected failure - check for authentication error
        error_msg = str(e).lower()
        assert 'authentication' in error_msg or 'password' in error_msg, f"Expected auth error, got: {e}"
        print(f"✓ Login with wrong password rejected (error: {e})")

def test_login_without_password(proxy):
    """Test that login fails without password when user has one."""
    conn = proxy.get_connection()
    cur = conn.cursor()
    
    # Create user with password
    cur.execute("CREATE USER testuser WITH PASSWORD 'testpass'")
    conn.commit()
    cur.close()
    conn.close()
    
    # Connect without password - should fail
    try:
        conn2 = proxy.get_connection(user="testuser", password="")
        conn2.close()
        assert False, "Should have failed without password"
    except psycopg2.OperationalError as e:
        # Expected failure
        print(f"✓ Login without password rejected (error: {e})")

def test_user_without_password_can_login(proxy):
    """Test that users without passwords can still connect."""
    conn = proxy.get_connection()
    cur = conn.cursor()
    
    # Create user without password but with LOGIN permission
    cur.execute("CREATE USER nopassuser WITH LOGIN")
    conn.commit()
    cur.close()
    conn.close()
    
    # Connect without password - should succeed (user has no password set)
    conn2 = proxy.get_connection(user="nopassuser", password="")
    cur2 = conn2.cursor()
    cur2.execute("SELECT current_user")
    row = cur2.fetchone()
    assert row[0] == 'nopassuser', f"Expected current_user 'nopassuser', got '{row[0]}'"
    
    print("✓ User without password can login")
    
    cur2.close()
    conn2.close()

def test_alter_user_password(proxy):
    """Test that ALTER USER changes password."""
    conn = proxy.get_connection()
    cur = conn.cursor()
    
    # Create user with initial password
    cur.execute("CREATE USER testuser WITH PASSWORD 'oldpass'")
    conn.commit()
    
    # Change password
    cur.execute("ALTER USER testuser WITH PASSWORD 'newpass'")
    conn.commit()
    cur.close()
    conn.close()
    
    # Old password should fail
    try:
        conn2 = proxy.get_connection(user="testuser", password="oldpass")
        conn2.close()
        assert False, "Should have failed with old password"
    except psycopg2.OperationalError:
        print("✓ Old password no longer works")
    
    # New password should succeed
    conn3 = proxy.get_connection(user="testuser", password="newpass")
    cur3 = conn3.cursor()
    cur3.execute("SELECT current_user")
    row = cur3.fetchone()
    assert row[0] == 'testuser'
    
    print("✓ ALTER USER changes password correctly")
    
    cur3.close()
    conn3.close()

def test_nonexistent_user_auto_created(proxy):
    """Test that connecting as nonexistent user auto-creates them."""
    # Connect as a new user that doesn't exist yet
    conn = proxy.get_connection(user="newuser", password="anypass")
    cur = conn.cursor()
    cur.execute("SELECT current_user")
    row = cur.fetchone()
    assert row[0] == 'newuser', f"Expected current_user 'newuser', got '{row[0]}'"
    
    print("✓ Nonexistent user auto-created on first connection")
    
    cur.close()
    conn.close()

if __name__ == "__main__":
    import sys
    
    # Allow running individual tests
    test_name = sys.argv[1] if len(sys.argv) > 1 else None
    
    tests = [
        ("create_user", test_create_user_with_password),
        ("correct_pass", test_login_with_correct_password),
        ("wrong_pass", test_login_with_wrong_password),
        ("no_pass", test_login_without_password),
        ("user_no_pass", test_user_without_password_can_login),
        ("alter_pass", test_alter_user_password),
        ("auto_create", test_nonexistent_user_auto_created),
    ]
    
    if test_name:
        # Run specific test
        for name, test_func in tests:
            if name == test_name:
                run_e2e_test(name, test_func)
                sys.exit(0)
        print(f"Unknown test: {test_name}")
        print(f"Available: {', '.join(n for n, _ in tests)}")
        sys.exit(1)
    else:
        # Run all tests
        failed = []
        for name, test_func in tests:
            try:
                run_e2e_test(name, test_func)
            except Exception as e:
                print(f"✗ {name} failed: {e}")
                failed.append(name)
        
        if failed:
            print(f"\nFailed tests: {', '.join(failed)}")
            sys.exit(1)
        else:
            print("\nAll tests passed!")
            sys.exit(0)