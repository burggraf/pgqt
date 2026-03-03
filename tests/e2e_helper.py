"""
E2E Test Helper Module

Provides utilities for E2E tests including:
- Dynamic port allocation
- Proxy lifecycle management
- Database connection helpers
"""

import socket
import subprocess
import time
import os
import signal
import tempfile
import psycopg2
from typing import Optional, Tuple

def find_free_port(start_port: int = 5432, max_port: int = 5500) -> int:
    """Find a free port in the given range."""
    for port in range(start_port, max_port):
        with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
            try:
                s.bind(('127.0.0.1', port))
                return port
            except OSError:
                continue
    raise RuntimeError(f"No free ports found in range {start_port}-{max_port}")

class ProxyManager:
    """Manages the PGQT proxy lifecycle for E2E tests."""

    def __init__(self, db_path: Optional[str] = None, port: Optional[int] = None):
        # Check if we should use an existing proxy (from run_all_e2e.py)
        self.use_existing = False
        self.existing_port = None
        self.existing_host = "127.0.0.1"

        if "PROXY_HOST" in os.environ and "PROXY_PORT" in os.environ:
            # Running under run_all_e2e.py - use existing proxy
            self.use_existing = True
            self.existing_host = os.environ["PROXY_HOST"]
            self.existing_port = int(os.environ["PROXY_PORT"])
            self.port = self.existing_port
            self.db_path = None
            return

        # Normal mode - start our own proxy
        self.db_path = db_path or tempfile.mktemp(suffix='.db', prefix='pglite_e2e_')
        self.port = port or find_free_port()
        self.process: Optional[subprocess.Popen] = None

    def start(self, timeout: int = 30) -> bool:
        """Start the proxy server."""
        if self.use_existing:
            # Just verify the existing proxy is ready
            start_time = time.time()
            while time.time() - start_time < timeout:
                if self._is_ready():
                    return True
                time.sleep(0.5)
            
            # Diagnostic: print if it fails
            print(f"DEBUG: Proxy at {self.existing_host}:{self.port} not ready after {timeout}s")
            return False

        # Clean up old database if exists
        if os.path.exists(self.db_path):
            os.remove(self.db_path)

        env = os.environ.copy()
        env["PG_LITE_DB"] = self.db_path
        env["PG_LITE_PORT"] = str(self.port)

        # Start proxy using debug binary (release build may have linking issues)
        # First check if debug binary exists, fall back to cargo run if not
        debug_binary = "./target/debug/pgqt"
        if os.path.exists(debug_binary):
            self.process = subprocess.Popen(
                [debug_binary,
                 "--port", str(self.port),
                 "--database", self.db_path],
                env=env,
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
                text=True,
                preexec_fn=os.setsid,
            )
        else:
            # Fall back to cargo run in debug mode
            self.process = subprocess.Popen(
                ["cargo", "run", "--quiet", "--",
                 "--port", str(self.port),
                 "--database", self.db_path],
                env=env,
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
                text=True,
                preexec_fn=os.setsid,
            )

        # Wait for proxy to be ready
        start_time = time.time()
        while time.time() - start_time < timeout:
            if self._is_ready():
                return True
            time.sleep(0.5)

        self.stop()
        return False

    def _is_ready(self) -> bool:
        """Check if the proxy is ready to accept connections."""
        try:
            conn = psycopg2.connect(
                host=self.existing_host if self.use_existing else "127.0.0.1",
                port=self.port,
                database="postgres",
                user="postgres",
                password="postgres",
                connect_timeout=1
            )
            conn.close()
            return True
        except:
            return False

    def stop(self, timeout: int = 5):
        """Stop the proxy server."""
        if self.use_existing:
            # Don't stop the proxy when using existing one
            return

        if self.process:
            try:
                os.killpg(os.getpgid(self.process.pid), signal.SIGTERM)
                self.process.wait(timeout=timeout)
            except:
                try:
                    os.killpg(os.getpgid(self.process.pid), signal.SIGKILL)
                except:
                    pass
            self.process = None

        # Clean up database
        if self.db_path and os.path.exists(self.db_path):
            try:
                os.remove(self.db_path)
            except:
                pass

    def get_connection(self, database: str = "postgres"):
        """Get a database connection."""
        conn = psycopg2.connect(
            host=self.existing_host if self.use_existing else "127.0.0.1",
            port=self.port,
            database=database,
            user="postgres",
            password="postgres"
        )
        conn.autocommit = True
        return conn

    def __enter__(self):
        if not self.start():
            raise RuntimeError("Failed to start proxy")
        return self

    def __exit__(self, exc_type, exc_val, exc_tb):
        self.stop()
        return False

def run_e2e_test(test_name: str, test_func, timeout: int = 60):
    """
    Run an E2E test with proper setup and teardown.

    Usage:
        def test_my_feature(proxy):
            conn = proxy.get_connection()
            # ... test code ...
            conn.close()

        if __name__ == "__main__":
            run_e2e_test("my_feature", test_my_feature)
    """
    import sys

    print(f"Starting E2E test: {test_name}")

    with ProxyManager() as proxy:
        print(f"Proxy ready on port {proxy.port}")
        try:
            test_func(proxy)
            print(f"✓ {test_name} passed")
            sys.exit(0)
        except Exception as e:
            print(f"✗ {test_name} failed: {e}")
            import traceback
            traceback.print_exc()
            sys.exit(1)
