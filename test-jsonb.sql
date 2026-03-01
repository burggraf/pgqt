-- 1. Drop previous test objects so script is idempotent
DROP TABLE IF EXISTS test_jsonb CASCADE;

-- 2. Create a test table with a JSONB column plus some scalar fields
CREATE TABLE test_jsonb(
    id serial PRIMARY KEY,
    name text,
    tags text[],
    -- JSONB field for flexible attributes
    props jsonb,
    created_at timestamp DEFAULT CURRENT_TIMESTAMP
);

-- 3. Insert sample rows with JSONB data
INSERT INTO test_jsonb(name, tags, props)
VALUES
    ('Alice', ARRAY['dev', 'remote'], '{"age": 30, "active": true, "country": "US", "team": {"id": 1, "name": "backend"}}'),
('Bob', ARRAY['qa', 'onsite'], '{"age": 25, "active": false, "country": "UK", "team": {"id": 2, "name": "qa"}}'),
('Carol', ARRAY['dev', 'remote'], '{"age": 35, "active": true, "country": "US", "team": {"id": 1, "name": "backend"}, "skills": ["sql","python"]}'),
('David', ARRAY['ops'], '{"age": 40, "active": true, "country": "DE"}'),
('Eve', ARRAY['dev'], '{"age": 28, "active": true, "country": "CA", "hobbies": ["gaming","hiking"]}');

-- 4. Basic JSONB extraction and casting
-- Extract age as numeric (->> + cast)
SELECT
    id,
    name,
    props ->> 'age' AS age_str,
(props ->> 'age')::int AS age_int
FROM
    test_jsonb
WHERE (props ->> 'age')::int >= 30;

-- Extract nested field (team.name)
SELECT
    id,
    name,
    props -> 'team' ->> 'name' AS team_name
FROM
    test_jsonb;

-- Extract boolean field and filter
SELECT
    id,
    name,
    props ->> 'active' AS active_str
FROM
    test_jsonb
WHERE (props ->> 'active')::boolean = TRUE;

-- 5. JSONB containment / nesting operators
-- @> (contains) -- check if props contains a given sub‑object
SELECT
    id,
    name,
    props
FROM
    test_jsonb
WHERE
    props @> '{"active": true}';

SELECT
    id,
    name,
    props
FROM
    test_jsonb
WHERE
    props @> '{"country": "US"}';

-- ? (key exists) -- check key presence
SELECT
    id,
    name,
    props
FROM
    test_jsonb
WHERE
    props ? 'team';

-- ?| (any key exists in array) -- at least one key in array exists
SELECT
    id,
    name,
    props
FROM
    test_jsonb
WHERE
    props ?| ARRAY['skills', 'hobbies'];

-- ?& (all keys exist) -- all keys in array exist
SELECT
    id,
    name,
    props
FROM
    test_jsonb
WHERE
    props ?& ARRAY['age', 'country'];

-- 6. JSONB array and object manipulation
-- Add a new key using the || operator
SELECT
    id,
    name,
    props || '{"role": "engineer"}' AS augmented_props
FROM
    test_jsonb
WHERE
    name = 'Alice';

-- Remove a key with the - operator
SELECT
    id,
    name,
    props,
    props - 'country' AS without_country
FROM
    test_jsonb
WHERE
    id = 1;

-- Remove multiple keys
SELECT
    id,
    name,
    props,
    props - ARRAY['age', 'active'] AS reduced_props
FROM
    test_jsonb
WHERE
    id = 1;

-- Concat two JSONB objects
SELECT
    id,
    name,
    props,
    props || '{"manager": "Lead"}' AS with_manager
FROM
    test_jsonb
WHERE
    id = 1;

-- 7. JSONB path operators (JSONPath, requires PG ≥ 12)
-- Check if path exists (jsonb_path_exists)
SELECT
    id,
    name,
    props
FROM
    test_jsonb
WHERE
    jsonb_path_exists(props, '$.team.id');

-- Query arrays with JSONPath (jsonb_path_query / jsonb_path_query_array)
SELECT
    id,
    name,
    jsonb_path_query(props, '$.skills[*]') AS skill
FROM
    test_jsonb
WHERE
    jsonb_path_exists(props, '$.skills[*]');

SELECT
    id,
    name,
    jsonb_path_query_array(props, '$.skills[*]') AS skills_array
FROM
    test_jsonb
WHERE
    props ? 'skills';

-- 8. Create JSONB‑related indexes
-- GIN index on props for containment queries (@>, ?)
CREATE INDEX idx_test_jsonb_props_gin ON test_jsonb USING GIN(props);

-- Expression index on a text field inside JSONB (country)
CREATE INDEX idx_test_jsonb_props_country ON test_jsonb((props ->> 'country'));

-- Expression index on an integer field inside JSONB (age)
CREATE INDEX idx_test_jsonb_props_age ON test_jsonb(((props ->> 'age')::int));

-- Expression index on a nested object (team.id)
CREATE INDEX idx_test_jsonb_props_team_id ON test_jsonb(((props -> 'team' ->> 'id')::int));

-- 9. Query that should use the indexes
-- This should use idx_test_jsonb_props_country (BTREE expression index)
EXPLAIN (
    ANALYZE
)
SELECT
    id,
    name,
    props
FROM
    test_jsonb
WHERE
    props ->> 'country' = 'US';

-- This should use the GIN index idx_test_jsonb_props_gin
EXPLAIN (
    ANALYZE
)
SELECT
    id,
    name,
    props
FROM
    test_jsonb
WHERE
    props @> '{"active": true}';

-- This should use idx_test_jsonb_props_age
EXPLAIN (
    ANALYZE
)
SELECT
    id,
    name,
    props
FROM
    test_jsonb
WHERE (props ->> 'age')::int >= 30;

-- 10. Basic JSONB validation / type fidelity
-- Inserts that should fail on real PG (invalid JSONB) can be useful,
-- but they abort the script; instead, test that valid JSONish strings round‑trip:
SELECT
    id,
    name,
    props,
    props::text AS props_as_text,
    props::jsonb AS props_roundtrip
FROM
    test_jsonb;

-- JSONB equality and comparison
SELECT
    id,
    name,
    props
FROM
    test_jsonb
ORDER BY
    props;

-- should work for JSONB (B‑tree order defined)
-- 11. Misc JSONB function calls
-- Check if props is an object vs array
SELECT
    id,
    name,
    jsonb_typeof(props) AS jb_type,
    jsonb_typeof(props -> 'team') AS team_type
FROM
    test_jsonb;

-- Get keys as JSONB array
SELECT
    id,
    name,
    jsonb_object_keys(props) AS top_level_keys
FROM
    test_jsonb
LIMIT 1;

-- Flatten object keys into rows
SELECT
    id,
    name,
    key,
    value
FROM
    test_jsonb,
    LATERAL jsonb_each(props) AS x(key,
        value);

