## 0.4.0
- New typestate API (`typed` module with `AnyIdle`/`AnyTx`, backend wrappers, and `TxOutcome`) plus unified `query`/`execute_batch` targets that work with pooled connections or explicit transactions across Postgres, SQLite, Turso (and still LibSQL/MSSQL). See an example in [test11](../tests/test11_issue_2.rs).
- Swapped PostgreSQL/SQLite pooling to `bb8` with new backend-specific config builders (`postgres_builder`, `sqlite_builder`) and optional placeholder translation flags; `bb8` gives us custom managers/owned clients needed by the new typed connections and keeps pooling consistent across Postgres/SQLite/Turso. Version bumped to `0.4.0` with compatibility aliases for `typed-postgres`/`typed-turso`.
- Reworked SQLite around a pooled `rusqlite` guard using `spawn_blocking`, explicit `BEGIN/COMMIT/ROLLBACK`, safer prepared-statement handling, and clearer error paths when mixing transactional and auto-commit work.
- Added Postgres/Turso typed connection support and new per-backend config modules.
- Marked LibSQL as deprecated in favor of Turso (no one likely using this lib yet, let alone using LibSQL but lmk if you were).
- Added/adjusted rusqlite and turso benches to cover multithread scenarios.
- Tests broadened with new cases for SQLite transaction semantics, typed/generic APIs, placeholder translation, and transaction drop/rollback behavior (`tests/test07_new_rusqlite.rs`, `tests/test08_custom_logic_between_txn.rs`, `tests/test09_typed_api_generic.rs`, `tests/test10_bad_drop.rs`, `tests/test11_issue_2.rs`, fixture `tests/test04.sql`).
- Expanded docs (`docs/README.md`, `docs.md`) and added new references (`docs/api_test_coverage.md`, `docs/DRY_violations.md`, `docs/issue_2_adj_api.md`, `docs/perf_43_to_HEAD.md`, `docs/plan-tursoTypedDry.prompt.md`) describing coverage, perf, and the new API shape.
- Housekeeping: `.gitignore` now ignores multithread benchmark DBs and `tests/sql_server_pwd.txt`; `AGENTS.md` test references corrected.
