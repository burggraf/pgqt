# Implementation Plan: Row-Level Security (RLS) via AST Injection

## 1. Overview
The goal is to implement PostgreSQL-compatible Row-Level Security (RLS) in `pgqt` by injecting `WHERE` clauses into the AST during transpilation. This avoids the need for table renaming and views, providing a more robust and transparent implementation.

## 2. Research Phase
- **PostgreSQL Compatibility**:
  - `ALTER TABLE ... [ENABLE|DISABLE] ROW LEVEL SECURITY`.
  - `ALTER TABLE ... [FORCE|NO FORCE] ROW LEVEL SECURITY`.
  - `CREATE POLICY name ON table_name [AS {PERMISSIVE|RESTRICTIVE}] [FOR {ALL|SELECT|INSERT|UPDATE|DELETE}] [TO {role_name|PUBLIC|CURRENT_USER|SESSION_USER}] [USING (using_expression)] [WITH CHECK (check_expression)]`.
  - `ALTER POLICY ...`.
  - `DROP POLICY ...`.
- **Policy Evaluation**:
  - Multiple `PERMISSIVE` policies are combined with `OR`.
  - `RESTRICTIVE` policies are combined with `AND`.
  - `USING` clause: For existing rows (`SELECT`, `UPDATE`, `DELETE`).
  - `WITH CHECK` clause: For new/modified rows (`INSERT`, `UPDATE`).
- **Session Context**:
  - Integration with the existing RBAC system.
  - Support for `current_user`, `session_user`.

## 3. Architecture & Design
- **Metadata Storage**:
  - Use a system table (e.g., `__pg_rls_policies__`) to store policy definitions.
  - Track which tables have RLS enabled/forced.
- **Transpiler Integration**:
  - Identify the operation type (`SELECT`, `INSERT`, `UPDATE`, `DELETE`).
  - Retrieve relevant policies for the target table and current user/role.
  - Generate a combined RLS expression.
  - Inject the expression into the AST:
    - `SELECT`: Add to `WHERE`.
    - `INSERT`: Add to `WITH CHECK` (might need transformation if SQLite doesn't support it directly, or handled via triggers/pre-check).
    - `UPDATE`: Add to `WHERE` (for `USING`) and `WITH CHECK`.
    - `DELETE`: Add to `WHERE`.
- **RBAC Integration**:
  - Ensure RLS respects the current session's role/user.

## 4. Implementation Steps
1.  **Metadata Layer**: Implement storage and retrieval of RLS policies.
2.  **DDL Support**: Implement `ALTER TABLE ... ENABLE RLS` and `CREATE POLICY` commands in the transpiler.
3.  **DML Injection**:
    - Implement the logic to combine policies.
    - Integrate with `pg_query` AST manipulation.
4.  **Session Management**: Ensure the current user is correctly identified during transpilation.

## 5. Testing Strategy
- **Unit Tests**:
  - Test policy combination logic (OR/AND).
  - Test AST injection for various query types.
- **Integration Tests**:
  - Test the full RLS lifecycle: Enable RLS -> Create Policy -> Execute DML.
  - Test multi-user scenarios.
- **E2E Tests**:
  - Use a PostgreSQL client (e.g., `psql` or a library) to connect to `pgqt` and verify RLS behavior against expected PG results.

## 6. Documentation
- Update `README.md` with RLS support details.
- Update `docs/TODO-FEATURES.md`.
- Create `docs/RLS.md` with detailed usage examples.
