import psycopg2
import sys

def test_session():
    try:
        # Connect to our proxy
        conn = psycopg2.connect(
            dbname='test',
            user='postgres',
            password='password',
            host='127.0.0.1',
            port=5432
        )
        print("✅ Connection established")

        cur = conn.cursor()
        
        # Test multiple sequential queries
        cur.execute("SELECT 1")
        print("✅ Query 1 sent")
        
        cur.execute("BEGIN")
        print("✅ Transaction BEGIN sent")
        
        cur.execute("COMMIT")
        print("✅ Transaction COMMIT sent")

        cur.close()
        conn.close()
        print("✅ Session closed gracefully")
        
    except Exception as e:
        print(f"❌ Session failed: {e}")
        sys.exit(1)

if __name__ == "__main__":
    test_session()
