<img src="https://r2cdn.perplexity.ai/pplx-full-logo-primary-dark%402x.png" style="height:64px;margin-right:32px"/>

# I'm looking for a comprehensive test suite for postgresql compatability in order to test my application, which is a wire-compatible proxy for postgres on top of a sqlite database.  I'd like an open-source test suite I can run to test all features of postgres and plpgsql to discover and fix compatability bugs, missing features, etc.

For a truly comprehensive PostgreSQL‑compatibility test suite you can reuse for your SQLite‑backed, wire‑compatible proxy, the best starting point is the **official PostgreSQL regression test suite**, plus a few complementary open‑source projects that specifically target SQL and wire‑protocol conformance.[^1_1][^1_2]

Below are the main options and how you can adapt them to your setup.

***

### 1. PostgreSQL regression test suite (most comprehensive)

The canonical “comprehensive” test suite for PostgreSQL itself is the **PostgreSQL regression test suite** (`src/test/regress` in the PostgreSQL source tree). It includes:

- Thousands of SQL‑conformance tests covering DDL, DML, constraints, transactions, types, functions, and more.
- Tests that are regularly ported or referenced by other compatible engines (e.g., Firebolt, various PG‑wire‑compatible backends).[^1_3][^1_2]

**How to use it for your proxy:**

- Run `initdb` on a real PostgreSQL instance, run `make check` there once, then reuse the generated SQL scripts (`.sql` and `.out`) from the regression directory.
- Point your proxy to a backing SQLite instance, and replay the same SQL scripts through `psql` or your driver stack, then compare the result output (or errors) against the reference PostgreSQL outputs.[^1_2][^1_3]

This gives you PL/pgSQL‑like SQL‑level coverage (functions, triggers, views, etc.), but you’ll need to strip or skip PG‑specific extensions (e.g., `pg_catalog`‑only features, procedural languages beyond SQL/PL/pgSQL if you don’t expose them).

***

### 2. General SQL‑conformance test suite (cross‑database)

For broader SQL‑level testing independent of PostgreSQL, use:

- **[elliotchance/sqltest](https://github.com/elliotchance/sqltest)**: a large SQL test suite based on the SQL‑92 standard and other SQL specs, designed to test different SQL engines.[^1_1]

You can run this against your proxy by:

- Configuring your test runner to connect to your proxy (PostgreSQL DSN) as if it were PostgreSQL.
- Reporting which SQL standards tests pass or fail, then drilling into the corner‑case SQL that your SQLite‑based engine doesn’t support.

This is excellent for catching generic SQL‑standard gaps (joins, subqueries, window functions, etc.) that may not be exercised by your app’s own test suite.[^1_2][^1_1]

***

### 3. PL/pgSQL‑style unit‑testing frameworks

If you want to test **procedural logic** specifically (functions, triggers, stored procedures modeled after PL/pgSQL), consider:

- **[pgTAP](https://pgtap.org / theory/pgtap)**: a unit‑testing framework written in PL/pgSQL that lets you write SQL‑level tests and assertions and run them via `pg_prove`.[^1_4][^1_5][^1_6]
- **[plpgunit](linked from FINRA blog)**: lightweight PL/pgSQL unit‑testing framework that runs purely in SQL/PL/pgSQL and is easy to drop into your schema.[^1_7]

In your case, you can:

- Implement a minimal subset of `pgTAP`‑style assertions in your SQLite backend (or in a thin wrapper layer).
- Execute the same `.sql` test files through your proxy, and record which assertions fail or which PL/pgSQL constructs are unsupported.

This is especially useful if your proxy exposes any form of stored‑function emulation layer that mimics PL/pgSQL semantics.[^1_8][^1_6][^1_4]

***

### 4. Wire‑protocol and client‑compatibility testing

Since your proxy is wire‑compatible, you’ll also want to check:

- **Multiple client drivers** (e.g., `psql`, `pg8000`, `libpq`, `tokio‑postgres`, etc.) running the same test scripts against your proxy and comparing results to a real PostgreSQL instance.[^1_9][^1_10]
- **Protocol‑level tests** from projects like `pgwire` or `psql‑wire` that already include integration tests exercising startup, authentication, simple vs. extended query, prepared statements, and binary encode/decode.[^1_11][^1_12][^1_13]

You can effectively “borrow” their test suites by:

- Running your proxy instead of their server in CI.
- Asserting that the same test scripts pass or fail the same way as they do against a real PostgreSQL backend.

***

### 5. Real‑world “compatibility index”‑style suites

- **PG Scorecard (PG‑compatibility index)** runs a battery of tests comparing a vendor database to standard PostgreSQL behavior (data types, DDL, constraints, procedural features).[^1_14]
- While it’s mainly a scoring tool, its public tests give you a checklist (and often example SQL) for features your proxy should support. You can manually incorporate those test cases into your own runner.[^1_15][^1_14]

***

### Practical recipe for your setup

Given you’re on top of SQLite:

1. **Base SQL coverage:**
    - Pull the PostgreSQL regression SQL scripts and `sqltest` tests.
    - Run them through your proxy → SQLite, and diff outputs vs PostgreSQL.[^1_1][^1_2]
2. **PL/pgSQL‑style logic:**
    - Use `pgTAP`‑style or `plpgunit`‑style test scripts and run them over your proxy, skipping unsupported PG‑extensions.[^1_5][^1_4][^1_7]
3. **Wire‑protocol sanity:**
    - Repurpose or mimic the integration tests from `pgwire` or `psql‑wire` to exercise startup, auth, simple/extended query, and binary format.[^1_12][^1_13][^1_11]
4. **Feature checklist:**
    - Use PG Scorecard categories (data types, DDL, constraints, procedural features) as a feature‑coverage checklist and add your own SQL tests for each missing item.[^1_14][^1_15]

If you tell me your stack (e.g., Go vs Rust, which PG‑wire library you’re using), I can sketch a concrete CI/CD‑style test script layout (e.g., “one Postgres container, one proxy, one SQLite backend, and three stages: SQL regressions, pgTAP‑style PL tests, and client‑driver matrix”).
<span style="display:none">[^1_16][^1_17][^1_18][^1_19][^1_20][^1_21][^1_22][^1_23][^1_24][^1_25][^1_26][^1_27][^1_28][^1_29][^1_30][^1_31][^1_32][^1_33][^1_34][^1_35]</span>

<div align="center">⁂</div>

[^1_1]: https://github.com/elliotchance/sqltest

[^1_2]: https://arxiv.org/html/2410.21731v1

[^1_3]: https://www.firebolt.io/blog/making-a-query-engine-postgres-compliant-part-i-functions

[^1_4]: https://pgtap.org

[^1_5]: https://github.com/theory/pgtap

[^1_6]: https://pgtap.org/documentation.html

[^1_7]: https://www.finra.org/about/how-we-operate/technology/blog/how-to-tame-postgres-unit-testing

[^1_8]: https://aws.amazon.com/blogs/database/create-a-unit-testing-framework-for-postgresql-using-the-pgtap-extension/

[^1_9]: https://spacetimedb.com/docs/docs/sql/pg-wire/

[^1_10]: https://questdb.com/docs/query/pgwire/rust/

[^1_11]: https://github.com/sunng87/pgwire

[^1_12]: https://www.reddit.com/r/rust/comments/122evm5/pgwire_a_library_for_writing_postgresql/

[^1_13]: https://pkg.go.dev/github.com/jeroenrinzema/psql-wire

[^1_14]: https://www.pgedge.com/blog/postgresql-compatibility-cutting-through-the-hype-with-pg-scorecard

[^1_15]: https://wiki.postgresql.org/wiki/PGConf.EU_2025_Establishing_the_PostgreSQL_standard_What_is_Postgres_compatible

[^1_16]: https://cedardb.com/blog/postgres_compatibility/

[^1_17]: https://www.cockroachlabs.com/docs/stable/postgresql-compatibility

[^1_18]: https://datastation.multiprocess.io/blog/2022-02-08-the-world-of-postgresql-wire-compatibility.html

[^1_19]: https://www.postgresql.org/docs/7.4/features.html

[^1_20]: https://www.pgedge.com/blog/considering-distributed-postgres-how-to-evaluate-vendor-claims-of-postgres-compatibility

[^1_21]: https://www.tigerdata.com/learn/postgresql-extensions-pgtap

[^1_22]: https://www.crunchydata.com/blog/using-the-cis-postgresql-benchmark-to-enhance-your-security

[^1_23]: https://github.com/ydb-platform/postgres-compatibility-tests

[^1_24]: https://github.com/connectrpc/conformance

[^1_25]: https://www.crunchydata.com/blog/secure-postgresql-14-with-the-cis-benchmark

[^1_26]: https://neon.com/postgresql/postgresql-18/security-improvements

[^1_27]: https://www.enterprisedb.com/docs/pgd/6/known_issues/

[^1_28]: https://dev.to/flagsmith/five-open-source-tools-to-track-your-postgresql-database-performance-3gki

[^1_29]: https://www.reddit.com/r/Database/comments/1pw0xge/building_a_postgrescompatible_database_what_tool/

[^1_30]: https://www.digitalocean.com/community/tutorials/how-to-audit-a-postgresql-database-with-inspec-on-ubuntu-18-04

[^1_31]: https://github.com/erans/pgsqlite

[^1_32]: https://oneuptime.com/blog/post/2026-02-02-sqlite-testing/view

[^1_33]: https://www.bytebase.com/blog/top-open-source-postgres-explain-tool/

[^1_34]: https://roapi.github.io/docs/postgres.html

[^1_35]: https://neon.com/blog/testing-sqlite-postgres


---

# My proxy is written in Rust.

Perfect—being in Rust makes it easy to wire up a very strong, reusable test suite for your SQLite‑backed, PostgreSQL‑wire‑compatible proxy. Here’s how to structure it around open‑source Rust‑friendly tooling plus the canonical PostgreSQL test suites.

***

### 1. Rust‑native wire‑protocol tests

Since you’re on Rust, lean on the ecosystem of the `pgwire` crate and its examples:

- Use **[sunng87/pgwire](https://github.com/sunng87/pgwire)** for the wire‑protocol layer and its **`sqlite.rs` example** as a direct reference for a SQLite‑backed PG‑wire server.[^2_1]
- Re‑use or adapt the **integration tests from `pgwire`** (e.g., `integration.rs`, `transaction.rs`, `extended.rs`) to drive your proxy with `tokio_postgres` or `postgres` and assert that:
    - Startup, auth, `SimpleQuery`, `ExtendedQuery`, cancel, copy, and notifications behave the same as against a real PostgreSQL instance.
    - Binary vs text encoding round‑trips correctly for common types.

You can run these tests in CI with:

- One container running real PostgreSQL (reference).
- One instance of your proxy + SQLite backend.
- A shared test harness that runs the same SQL test scripts against both and diffs result sets / errors.

***

### 2. SQL‑level regression tests (Rust‑driven)

For comprehensive SQL‑feature coverage, combine:

- The **PostgreSQL regression‑test SQL scripts** (`src/test/regress/*.sql`) as your “golden” SQL corpus.
- A **Rust test runner** (e.g., `test` or `proptest`‑style) that:
    - Connects via `tokio_postgres` to both PostgreSQL and your proxy.
    - Executes each test script in a transaction, captures rows / errors, and compares them.

You can also integrate:

- **[elliotchance/sqltest](https://github.com/elliotchance/sqltest)**: pull its SQL test files and run them through your Rust test framework against your proxy, skipping dialect‑specific bits.[^2_3][^2_2]

This layer will smoke‑test data types, DDL/DML, constraints, transactions, and basic functions against your SQLite‑based backend.

***

### 3. PL/pgSQL‑style logic tests in Rust

To smoke‑test any PL/pgSQL‑like features you expose (functions, triggers, stored‑procedure semantics), you can:

- Use a minimal **`pgTAP`‑style test harness** written in Rust:
    - Prepare a set of SQL test files structured as `plan(…); ok(…); is(…);`‑style statements.[^2_4][^2_5][^2_6]
    - Run them through your proxy and check that the test result rows (e.g., a `results` table you ingest) match expectations.
- Or keep the tests in pure SQL and run them via `tokio_postgres` in Rust tests, treating PostgreSQL as the “truth” and logging differences.

***

### 4. Example CI‑style layout you can copy

A concrete pattern you might want in `tests/compatibility.rs`:

```rust
#[tokio::test]
async fn run_pg_regress_scenario() {
    let pg_client = connect_to_pg().await;
    let proxy_client = connect_to_proxy().await;

    let sql = include_str!("data/regress/01_basic.sql");  // or from sqltest

    let pg_results = pg_client.query(sql, &[]).await.unwrap();
    let proxy_results = proxy_client.query(sql, &[]).await.unwrap();

    assert_eq!(
        serialized_results(&pg_results),
        serialized_results(&proxy_results)
    );
}
```

You can then:

- Generate a **“compatibility matrix”** per test file (pass/fail + diff summary).
- Tag tests by category (types, DDL, transactions, functions, etc.) so you can report “we’re missing X% of PL/pgSQL‑style tests” as a measurable metric.[^2_7][^2_8]

***

### 5. What to prioritize if you’re time‑boxed

Given your use case (Rust proxy on top of SQLite):

1. Start with **`pgwire` integration‑style tests** to harden wire‑protocol behavior.[^2_9][^2_1]
2. Add **PostgreSQL regression SQL tests** (a small subset first) run via `tokio_postgres` against both Postgres and your proxy.
3. Add **`sqltest`** SQL conformance tests as a “wildcard” catch‑all for SQL‑standard quirks.[^2_3][^2_2]

If you want, you can paste a snippet of your current proxy entry point or `pgwire`‑based handler loop, and I can sketch an exact test module layout (crate structure, `Cargo.toml` deps, and a sample test that runs one SQL test file against both PostgreSQL and your SQLite‑backed proxy).
<span style="display:none">[^2_10][^2_11][^2_12][^2_13][^2_14][^2_15][^2_16][^2_17][^2_18][^2_19][^2_20][^2_21][^2_22]</span>

<div align="center">⁂</div>

[^2_1]: https://github.com/sunng87/pgwire

[^2_2]: https://github.com/elliotchance/sqltest

[^2_3]: https://arxiv.org/html/2410.21731v1

[^2_4]: https://pgtap.org

[^2_5]: https://github.com/theory/pgtap

[^2_6]: https://pgtap.org/documentation.html

[^2_7]: https://www.pgedge.com/blog/postgresql-compatibility-cutting-through-the-hype-with-pg-scorecard

[^2_8]: https://wiki.postgresql.org/wiki/PGConf.EU_2025_Establishing_the_PostgreSQL_standard_What_is_Postgres_compatible

[^2_9]: https://docs.rs/pgwire

[^2_10]: https://github.com/superfly/pgwire-1

[^2_11]: https://www.reddit.com/r/rust/comments/122evm5/pgwire_a_library_for_writing_postgresql/

[^2_12]: https://crates.io/crates/pgwire-replication

[^2_13]: https://questdb.com/docs/query/pgwire/r/

[^2_14]: https://crates.io/crates/pgwire-lite

[^2_15]: https://spacetimedb.com/docs/docs/sql/pg-wire/

[^2_16]: https://github.com/stackql/pgwire-lite-rs

[^2_17]: https://github.com/sunng87/pgwire/discussions/204

[^2_18]: https://roapi.github.io/docs/postgres.html

[^2_19]: https://questdb.com/docs/query/pgwire/rust/

[^2_20]: https://cedardb.com/blog/postgres_compatibility/

[^2_21]: https://github.com/misachi/pg_rusted_wire

[^2_22]: https://docs.rs/crate/pgwire/latest/source/Cargo.toml.orig
