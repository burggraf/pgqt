-- Basic pgTAP-style assertion stub
-- For testing PG-style procedural logic on top of SQLite

CREATE OR REPLACE FUNCTION plan(tests INT) RETURNS TEXT AS $$
BEGIN
  RETURN '1..' || tests::text;
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION ok(test_bool BOOLEAN, description TEXT) RETURNS TEXT AS $$
BEGIN
  IF test_bool THEN
    RETURN 'ok - ' || description;
  ELSE
    RETURN 'not ok - ' || description;
  END IF;
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION is(actual ANYELEMENT, expected ANYELEMENT, description TEXT) RETURNS TEXT AS $$
BEGIN
  IF actual = expected THEN
    RETURN 'ok - ' || description;
  ELSE
    RETURN 'not ok - ' || description || ' (expected ' || expected::text || ', got ' || actual::text || ')';
  END IF;
END;
$$ LANGUAGE plpgsql;
