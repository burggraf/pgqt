-- Test Array operators
CREATE TABLE array_test (id INT, tags TEXT[]);
INSERT INTO array_test VALUES (1, ARRAY['a', 'b', 'c']), (2, ARRAY['b', 'c', 'd']), (3, ARRAY['x', 'y']);

-- Array contains
SELECT id FROM array_test WHERE tags @> ARRAY['a'];
-- Array overlap
SELECT id FROM array_test WHERE tags && ARRAY['b', 'z'];
-- Array contained
SELECT id FROM array_test WHERE tags <@ ARRAY['a', 'b', 'c', 'd', 'e'];

-- Test vector operators (pgvector)
CREATE TABLE vector_test (id INT, embedding VECTOR(3));
INSERT INTO vector_test VALUES (1, '[1,2,3]'), (2, '[4,5,6]'), (3, '[1.1, 2.1, 3.1]');

-- L2 distance (<->)
SELECT id, embedding <-> '[1,2,3]' as dist FROM vector_test ORDER BY dist;
-- Cosine distance (<=>)
SELECT id, embedding <=> '[1,2,3]' as dist FROM vector_test ORDER BY dist;
-- Inner product (<#>)
SELECT id, (embedding <#> '[1,2,3]') * -1 as prod FROM vector_test ORDER BY prod DESC;

-- Test Range operators
CREATE TABLE range_test (id INT, r INT4RANGE);
INSERT INTO range_test VALUES (1, '[1,10)'), (2, '[5,15)'), (3, '[20,30)');

-- Range overlap
SELECT id FROM range_test WHERE r && '[7,12)';
-- Range contains
SELECT id FROM range_test WHERE r @> 5;
-- Range contained
SELECT id FROM range_test WHERE r <@ '[0,100)';
