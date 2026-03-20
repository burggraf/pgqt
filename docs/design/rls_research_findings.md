# RLS Research Findings

## 1. PostgreSQL RLS Specification vs. Current Implementation

### 1.1 ALTER TABLE Syntax & Behavior
| Feature | PostgreSQL Behavior | Current Implementation (`src/catalog.rs`) |
|---------|---------------------|-------------------------------------------|
| `ENABLE RLS` | Activates RLS. Default-deny if no policies exist. | Supported via `enable_rls`. Correctly defaults to `FALSE` if no policies. |
| `DISABLE RLS` | Deactivates RLS. Policies remain in metadata. | Supported via `disable_rls`. |
| `FORCE RLS` | RLS applies even to table owners. | Supported via `enable_rls(..., true)`. |
| `NO FORCE RLS` | Owners are exempt from RLS (default). | Supported via `enable_rls(..., false)`. |
| **Locking** | Requires `AccessExclusiveLock`. | SQLite handles via file/table locks during `conn.execute`. |

### 1.2 Policy Definition (`CREATE POLICY`)
| Clause | Specification | Current Status / Gap |
|--------|---------------|----------------------|
| `AS [PERMISSIVE\|RESTRICTIVE]` | Permissive (OR), Restrictive (AND). | Metadata supports `polpermissive`. |
| `FOR [ALL\|SELECT\|...]` | Defines operation scope. | Metadata supports `polcmd`. |
| `TO role` | List of roles or `PUBLIC`. | Metadata supports `polroles`. |
| `USING (expr)` | Filter for existing rows. | Metadata supports `polqual`. |
| `WITH CHECK (expr)`| Filter for new/modified rows. | Metadata supports `polwithcheck`. |

### 1.3 Policy Evaluation Logic
The core logic for combining policies in PostgreSQL is:
**`(Permissive_1 OR Permissive_2 OR ...) AND (Restrictive_1 AND Restrictive_2 AND ...)`**

*   **Current implementation** in `src/rls.rs:build_rls_expression` **mostly matches** this:
    *   It joins permissive policies with `OR`.
    *   It joins restrictive policies with `AND`.
    *   It combines both groups with `AND`.
*   **Gap**: If *no* permissive policies apply, PostgreSQL defaults to **deny** (unless RLS is disabled or bypassed). Current logic returns `None` which might be interpreted as "no filter" by the caller.

---

## 2. Session Context & RBAC Integration

### 2.1 Current User / Role
*   In `src/rls.rs`, `RlsContext` stores `current_user` and `user_roles`.
*   `current_user` is correctly used in `can_bypass_rls` to check ownership.
*   **Recommendation**: We need to ensure the SQLite environment has a `current_user` function so that `USING (user_id = current_user)` works directly in injected SQL.

### 2.2 Bypassing RLS
*   Owners bypass RLS unless `FORCE` is enabled. Correctly implemented in `can_bypass_rls`.
*   Superusers always bypass. `src/catalog.rs` has a stub for `rolsuper`.

---

## 3. Proposed Changes: Moving to AST Injection

The current `transpile_with_rls` in `src/transpiler.rs` uses string concatenation:
```rust
result.sql = format!("{} WHERE ({})", result.sql, rls_where);
```
**This is fragile** (fails on queries with existing `WHERE`, `GROUP BY`, or subqueries).

### 3.1 Transpiler Integration Plan
1.  **Modify `reconstruct_select_stmt`**:
    *   Accept an optional `rls_predicate` string.
    *   If provided, combine it with the existing `where_clause` using `AND`.
2.  **Update `transpile_with_rls`**:
    *   Instead of string formatting the result, it should pass the RLS context down into the `reconstruct_` functions.
    *   Identify the table name from `RangeVar` nodes in the AST.
3.  **Handle `UPDATE` and `INSERT`**:
    *   `UPDATE`: Requires both `USING` (in `WHERE`) and `WITH CHECK`.
    *   `INSERT`: Requires `WITH CHECK`. Since SQLite lacks a native `WITH CHECK` on `INSERT`, we should transpile these to:
        `INSERT INTO ... SELECT ... WHERE (rls_check_expr)` or use a `BEFORE INSERT` trigger strategy.

### 3.2 SQL Function Support
Inject a custom SQLite function `current_user()` that returns `ctx.current_user` so that policies like `USING (owner = current_user())` are valid SQLite SQL.

---

## 4. Summary of Findings
- **Metadata**: Robust and matches PG catalog structure.
- **Logic**: Combination logic is correct but needs a "Default Deny" safeguard for empty permissive sets.
- **Injection**: Moving from string-based `format!` to AST-based injection is critical for 100% compatibility.
