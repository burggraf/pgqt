#!/usr/bin/env python3
"""
Unified E2E Test Runner for PGQT

This script runs all Python e2e tests in sequence, managing the proxy
lifecycle efficiently by starting it once and running all tests against it.
"""

import subprocess
import sys
import os
import signal
import time
import tempfile
import glob
from typing import List, Tuple

# Test configuration
PROXY_HOST = "127.0.0.1"
PROXY_PORT = 5434
PROXY_TIMEOUT = 30  # seconds


class ProxyManager:
    """Manages the PGQT proxy lifecycle."""
    
    def __init__(self, db_path: str, port: int = PROXY_PORT):
        self.db_path = db_path
        self.port = port
        self.process = None
        
    def start(self) -> bool:
        """Start the proxy server."""
        
        if os.path.exists(self.db_path):
            os.remove(self.db_path)
        
        env = os.environ.copy()
        env["PGQT_DB"] = self.db_path
        env["PGQT_PORT"] = str(self.port)
        
        # Start proxy in release mode for faster execution
        # Explicitly pass --port and --database to ensure it listens where we expect
        # We don't use --host here to let it default to localhost:5434
        self.process = subprocess.Popen(
            ["cargo", "run", "--release", "--quiet", "--", "--port", str(self.port), "--database", self.db_path, "--trust-mode"],
            env=env,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            preexec_fn=os.setsid,
        )
        
        # Wait for proxy to be ready
        start_time = time.time()
        while time.time() - start_time < PROXY_TIMEOUT:
            if self._is_ready():
                return True
            time.sleep(0.5)
        
        self.stop()
        return False
    
    def _is_ready(self) -> bool:
        """Check if the proxy is ready to accept connections."""
        try:
            import psycopg2
            conn = psycopg2.connect(
                host=PROXY_HOST,
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
    
    def stop(self):
        """Stop the proxy server."""
        if self.process:
            try:
                os.killpg(os.getpgid(self.process.pid), signal.SIGTERM)
                self.process.wait(timeout=5)
            except:
                try:
                    os.killpg(os.getpgid(self.process.pid), signal.SIGKILL)
                except:
                    pass
            self.process = None


def discover_tests() -> List[str]:
    """Discover all e2e test files."""
    test_dir = os.path.dirname(os.path.abspath(__file__))
    pattern = os.path.join(test_dir, "*_e2e_test.py")
    return sorted(glob.glob(pattern))


def run_test_file(test_path: str, proxy_host: str, proxy_port: int) -> Tuple[bool, str]:
    """
    Run a single e2e test file.
    Returns (success, output)
    """
    test_name = os.path.basename(test_path)
    
    try:
        # Import the test module and run its tests
        import importlib.util
        spec = importlib.util.spec_from_file_location("test_module", test_path)
        module = importlib.util.module_from_spec(spec)
        
        # Set up the module's environment
        module.__dict__['PROXY_HOST'] = proxy_host
        module.__dict__['PROXY_PORT'] = proxy_port
        
        spec.loader.exec_module(module)
        
        # Look for test functions
        test_functions = [
            getattr(module, name) for name in dir(module)
            if name.startswith('test_') and callable(getattr(module, name))
        ]
        
        if not test_functions:
            return True, f"{test_name}: No tests found (may use different pattern)"
        
        # Run each test function
        passed = 0
        failed = 0
        errors = []
        
        for test_func in test_functions:
            try:
                test_func()
                passed += 1
            except Exception as e:
                failed += 1
                errors.append(f"  {test_func.__name__}: {str(e)}")
        
        if failed == 0:
            return True, f"{test_name}: {passed} passed"
        else:
            return False, f"{test_name}: {passed} passed, {failed} failed\n" + "\n".join(errors)
            
    except Exception as e:
        return False, f"{test_name}: Error loading test - {str(e)}"


def run_subprocess_test(test_path: str, proxy_host: str, proxy_port: int) -> Tuple[bool, str]:
    """
    Run a test file as a subprocess (for tests that manage their own setup).
    Returns (success, output)
    """
    test_name = os.path.basename(test_path)
    
    env = os.environ.copy()
    env["PROXY_HOST"] = proxy_host
    env["PROXY_PORT"] = str(proxy_port)
    
    try:
        result = subprocess.run(
            [sys.executable, test_path],
            env=env,
            capture_output=True,
            text=True,
            timeout=60
        )
        
        if result.returncode == 0:
            return True, f"{test_name}: PASSED"
        else:
            output = result.stdout + result.stderr
            return False, f"{test_name}: FAILED\n{output}"
    except subprocess.TimeoutExpired:
        return False, f"{test_name}: TIMEOUT"
    except Exception as e:
        return False, f"{test_name}: ERROR - {str(e)}"


def main():
    """Main entry point."""
    print("=" * 60)
    print("PGQT E2E Test Runner")
    print("=" * 60)
    print()
    
    # Check for psycopg2
    try:
        import psycopg2
    except ImportError:
        print("Error: psycopg2 not installed")
        print("Install with: pip install psycopg2-binary")
        sys.exit(1)
    
    # Create temporary database
    db_fd, db_path = tempfile.mkstemp(suffix='.db', prefix='pglite_e2e_')
    os.close(db_fd)
    
    proxy = None
    results = []
    
    try:
        # Start proxy
        print("Starting PGQT proxy...")
        proxy = ProxyManager(db_path, PROXY_PORT)
        if not proxy.start():
            print("Failed to start proxy!")
            sys.exit(1)
        print(f"Proxy ready on port {PROXY_PORT}")
        print()
        
        # Discover tests
        test_files = discover_tests()
        print(f"Discovered {len(test_files)} test files:")
        for test_file in test_files:
            print(f"  - {os.path.basename(test_file)}")
        print()
        
        # Run tests
        print("Running tests...")
        print("-" * 60)
        
        for test_file in test_files:
            success, output = run_subprocess_test(test_file, PROXY_HOST, PROXY_PORT)
            results.append((os.path.basename(test_file), success, output))
            
            if success:
                print(f"✓ {output}")
            else:
                print(f"✗ {output}")
        
        print("-" * 60)
        print()
        
    finally:
        # Stop proxy
        if proxy:
            print("Stopping proxy...")
            proxy.stop()
        
        # Clean up database
        if os.path.exists(db_path):
            os.remove(db_path)
    
    # Print summary
    print("=" * 60)
    print("Test Summary")
    print("=" * 60)
    
    passed = sum(1 for _, success, _ in results if success)
    failed = sum(1 for _, success, _ in results if not success)
    
    for test_name, success, output in results:
        status = "✓" if success else "✗"
        print(f"{status} {test_name}")
    
    print()
    print(f"Total: {passed} passed, {failed} failed")
    
    if failed > 0:
        print()
        print("Failed test details:")
        for test_name, success, output in results:
            if not success:
                print(f"\n{test_name}:")
                print(output)
        sys.exit(1)
    else:
        print()
        print("All tests passed! ✓")
        sys.exit(0)


if __name__ == "__main__":
    main()
