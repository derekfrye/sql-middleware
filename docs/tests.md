Test suite feature gates
=======================

This project’s integration tests are heavily feature-gated. Use the notes below to understand what `cargo test` will and will not build/run under different feature sets.

High-level behavior
-------------------
- Default `cargo test` enables features `{postgres, sqlite}` plus `{benchmarks, mssql, turso}` through the dev-dependency.
- `cargo test --all-features` turns on every feature flag (including `mssql`, `turso`), so all test files and their conditional branches become eligible. External services/credentials are still required for some backends (Postgres at 10.3.0.201, MSSQL password file, etc.).

Per-test gating
---------------
- `tests/test01.rs`, `tests/test03_sqlite.rs`: compiled when `sqlite` **or** `turso` is enabled. Turso-specific branches execute only with `turso`.
- `tests/test02_postgres.rs`: Postgres only (requires external Postgres).
- `tests/test04_AnyConnWrapper.rs`: always runs SQLite cases; adds Postgres when `postgres` is on, MSSQL when `mssql` is on (requires `tests/sql_server_pwd.txt`), Turso when `turso` is on.
- `tests/test05a_postgres.rs`: `#![cfg(feature = "postgres")]`; Postgres only.
- `tests/test05c_sqlite.rs`: `sqlite` only. `tests/test05d_turso.rs`: `turso` only.
- `tests/test06_postgres_translation.rs`: `postgres` only. `tests/test06_turso_translation.rs`: `turso` only.
- `tests/test07_new_rusqlite.rs`: `sqlite` only.
- `tests/test08_custom_logic_between_txn.rs`: file compiles if any of `sqlite/postgres/turso` is enabled; branches are per-backend. Typed-Postgres block is guarded by `cfg(all(feature = "postgres", feature = "postgres"))` (effectively `postgres`).
- `tests/test09_typed_api_generic.rs`, `tests/test10_bad_drop.rs`: require **all** of `postgres`, `turso`, and `sqlite`.
- `tests/test11_issue_2.rs`: compiled when any of `postgres/sqlite/turso` is on; backend arms are individually gated.

Practical takeaways
-------------------
- To exercise the full suite, run `cargo test --all-features`.
- MSSQL coverage depends on `tests/sql_server_pwd.txt` being present and `mssql` enabled.
