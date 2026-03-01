# Geometric Types in PGlite Proxy

PostgreSQL geometric types are supported in PGlite Proxy by storing them as `TEXT` in SQLite using their canonical string representation. Spatial operators and functions are transpiled to custom SQLite functions implemented in Rust.

## Supported Types

| Type | Canonical Format | Description |
| :--- | :--- | :--- |
| `point` | `(x,y)` | Point on a 2D plane |
| `line` | `{A,B,C}` | Infinite line (Ax + By + C = 0) |
| `lseg` | `((x1,y1),(x2,y2))` | Finite line segment |
| `box` | `((x1,y1),(x2,y2))` | Rectangular box |
| `path` | `((x1,y1),...)` or `[(x1,y1),...]` | Closed or open path |
| `polygon` | `((x1,y1),...)` | Closed polygon |
| `circle` | `<(x,y),r>` | Circle with center and radius |

## Supported Operators

The following PostgreSQL geometric operators are supported:

| Operator | Name | Supported Types |
| :--- | :--- | :--- |
| `&&` | Overlaps | box, polygon, circle |
| `@>` | Contains | box, polygon, circle |
| `<@` | Contained in | box, polygon, circle |
| `<<` | Strictly left | point, box, polygon, circle |
| `>>` | Strictly right | point, box, polygon, circle |
| `<<|` | Strictly below | point, box, polygon, circle |
| `|>>` | Strictly above | point, box, polygon, circle |
| `<->` | Distance | point, box, circle |
| `?|` | Is vertical | line, lseg |
| `?-` | Is horizontal | line, lseg |
| `?||` | Is parallel | line, lseg |
| `?-|` | Is perpendicular| line, lseg |

## Usage Examples

### Point Distance
```sql
SELECT point(1, 2) <-> point(4, 6); -- Returns 5.0
```

### Box Overlap Check
```sql
SELECT box '(0,0),(2,2)' && box '(1,1),(3,3)'; -- Returns true
```

### Spatial Query with Boxes
```sql
CREATE TABLE areas (id SERIAL PRIMARY KEY, boundary BOX);
INSERT INTO areas (boundary) VALUES ('(0,0),(2,2)'), ('(4,4),(6,6)');

-- Find areas overlapping with a target box
SELECT id FROM areas WHERE boundary && '(1,1),(5,5)'; -- Returns IDs [1, 2]
```

## Internal Storage
All geometric types are stored in SQLite as `TEXT`. The proxy automatically handles the conversion between PostgreSQL's wire format (binary or text) and the underlying SQLite text storage. When queries are transpiled, operators like `&&` are rewritten to call internal SQLite functions like `geo_overlaps(col, literal)`.
