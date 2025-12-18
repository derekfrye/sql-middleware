Test suite feature gates
=======================

This project’s integration tests are heavily feature-gated. Use the notes below to understand what `cargo test` will and will not build/run under different feature sets.

High-level behavior
-------------------
- Default `cargo test` enables features `{postgres, sqlite}` plus `{benchmarks, mssql, turso, test-utils}` through the dev-dependency. The only backend feature disabled by default is `libsql`, so libsql-only tests are the primary ones skipped in a normal run.
- `cargo test --all-features` turns on every feature flag (including `libsql`, `mssql`, `turso`, `test-utils`), so all test files and their conditional branches become eligible. The only code still not exercised is explicitly commented out (e.g., libsql branches in `test04_AnyConnWrapper.rs`). External services/credentials are still required for some backends (Postgres at 10.3.0.201, MSSQL password file, etc.).

Per-test gating
---------------
- `tests/test01.rs`, `tests/test03_sqlite.rs`: compiled when `sqlite` **or** `turso` is enabled. Turso-specific branches execute only with `turso`.
- `tests/test02_postgres.rs`: assumes `test-utils` is enabled so `setup_postgres_container` is available; uses Postgres only.
- `tests/test04_AnyConnWrapper.rs`: always runs SQLite cases; adds Postgres when `postgres` is on, MSSQL when `mssql` is on (requires `tests/sql_server_pwd.txt`), Turso when `turso` is on. LibSQL branches are commented out.
- `tests/test05a_postgres.rs`: `#![cfg(feature = "test-utils")]`; Postgres only.
- `tests/test05b_libsql.rs`, `tests/test06_libsql.rs`, `tests/test06_libsql_simple.rs`: `#![cfg(feature = "libsql")]`; skipped unless `libsql` is enabled.
- `tests/test05c_sqlite.rs`: `sqlite` only. `tests/test05d_turso.rs`: `turso` only.
- `tests/test06_postgres_translation.rs`: `postgres` only. `tests/test06_turso_translation.rs`: `turso` only.
- `tests/test07_new_rusqlite.rs`: `sqlite` only.
- `tests/test08_custom_logic_between_txn.rs`: file compiles if any of `sqlite/postgres/libsql/turso` is enabled; branches are per-backend. Typed-Postgres block is guarded by `cfg(all(feature = "postgres", feature = "postgres"))` (effectively `postgres`).
- `tests/test09_typed_api_generic.rs`, `tests/test10_bad_drop.rs`: require **all** of `postgres`, `turso`, and `sqlite`.
- `tests/test11_issue_2.rs`: compiled when any of `postgres/sqlite/turso` is on; backend arms are individually gated.
- `src/test_utils/postgres/tests.rs`: only with `test-utils`.

Practical takeaways
-------------------
- To exercise the full suite, run `cargo test --all-features`.
- To exercise libsql tests specifically, include `--features libsql` (or `--all-features`).
- MSSQL coverage depends on `tests/sql_server_pwd.txt` being present and `mssql` enabled.
