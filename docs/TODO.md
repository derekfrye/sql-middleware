# Refactor TODOs (Length Targets)

Targets
- Keep files ≤ 200 LOC where practical.
- Keep functions ≤ 50 LOC where practical.

Backend Duplication Hotspots
- Repeated `ConfigAndPool::new_*` constructors across backends (`src/sqlite/config.rs`, `src/libsql/config.rs`, `src/postgres/config.rs`, `src/mssql/config.rs`, `src/turso/config.rs`) follow the same pool-init + smoke-test pattern with backend-specific wiring.
- Execution helpers (`execute_batch`, `execute_select`, `execute_dml`) share nearly identical control flow in `src/*/executor.rs`, differing mostly in adapter calls and error wording.
- Parameter conversion layers duplicate the mapping from `RowValues` into driver-native types and timestamp formatting logic in `src/*/params.rs`.
- Result-set builders mirror each other when walking columns/rows to populate `ResultSet` (`src/*/query.rs`).
- Transaction wrappers for libsql-like engines (`src/libsql/transaction.rs`, `src/turso/transaction.rs`) expose the same BEGIN/COMMIT/ROLLBACK and prepared-statement surface.
- Module scaffolding (`src/*/mod.rs`) re-exports the same API sets with only backend names changed.

Proposed Next Steps
- Extract shared traits/helpers for pool creation and CRUD execution paths so backends only provide driver-specific pieces (e.g., pool builder + type aliases).
- Centralise `RowValues` conversion and timestamp formatting into reusable helpers to avoid diverging behaviour between adapters.
- Consolidate result-set assembly into backend-agnostic utilities (parameterised by column/value extractor) to trim repeated loops.
- Explore a lightweight macro or template to cut down on the identical `mod.rs` re-export boilerplate per backend.

Scan Summary
- Files > 200 LOC
  - 493 lines — `tests/test4_AnyConnWrapper.rs`
- Near-threshold files (watch for growth)
  - 198 lines — `src/test_utils/postgres/tests.rs`
  - 193 lines — `tests/test2_postgres.rs`
  - 192 lines — `src/benchmark/postgres.rs`

- Functions > 50 lines (approx)
  - `tests/test4_AnyConnWrapper.rs`
    - 374 lines — `async fn run_test_logic(...)`
    - 91 lines — `fn test4_trait(...)`
  - `tests/test2_postgres.rs`
    - 171 lines — `fn test2_postgres_cr_and_del_tbls(...)`
  - `tests/test3_sqlite.rs`
    - 142 lines — `fn sqlite_and_turso_multiple_column_test_db2(...)`
  - `tests/test_libsql.rs`
    - 110 lines — `fn test_libsql_basic_operations(...)`
  - `tests/test1.rs`
    - 95 lines — `fn sqlite_and_turso_core_logic(...)`
  - `src/test_utils/postgres/embedded.rs`
    - 91 lines — `pub fn setup_postgres_embedded(...)`
  - `src/benchmark/postgres.rs`
    - 89 lines — `async fn setup_postgres_db(...)`
  - `src/benchmark/common.rs`
    - 72 lines — `pub fn generate_postgres_insert_statements(...)`
    - 72 lines — `pub fn generate_insert_statements(...)`
  - `benches/common.rs`
    - 72 lines — mirror of the above
  - `tests/test_libsql_simple.rs`
    - 58 lines — `fn test_libsql_simple(...)`
  - `src/mssql/query.rs`
    - 58 lines — `pub async fn build_result_set(...)`
    - 58 lines — `fn extract_value(...)`
  - `src/libsql/query.rs`
    - 53 lines — `pub async fn build_result_set(...)`

Notes
- Detector may miscount trait signatures; list above filters to concrete fns.
- Test files can exceed targets, but still worth splitting for readability.

Recommended Refactor Order (highest impact first)
1) `tests/test4_AnyConnWrapper.rs`
   - Split into smaller helpers:
     - `setup_db(db_type)`, `apply_schema(conn)`, `seed_basic(conn)`,
       `bulk_insert_mw(conn, params)`, `bulk_insert_tx_postgres(...)`,
       `bulk_insert_tx_sqlite(...)`, `verify_counts(conn, expected)`.
   - Keep top-level tests thin; group backend-specific branches into dedicated helpers.

2) `src/mssql/query.rs`
   - Extract helpers from `build_result_set`:
     - `mssql_column_names(stmt) -> Arc<Vec<String>>`
     - `mssql_row_to_values(row, col_count) -> Vec<RowValues>`
   - Consider moving `extract_value` per-type mapping into a small focused module function.

3) `src/libsql/query.rs`
   - Split `build_result_set` into:
     - `libsql_column_names(rows) -> Arc<Vec<String>>`
     - `libsql_process_rows(rows, col_count) -> ResultSet`

4) `src/test_utils/postgres/embedded.rs`
   - Break `setup_postgres_embedded` into phases:
     - `config_from_env()`, `start_embedded()`, `wait_ready()`, `create_db_if_missing()`, `return_handles()`.
   - Improves reuse and test readability.

5) `tests/test2_postgres.rs`
   - Factor monolithic test into multiple `#[test]`s or helper functions:
     - `create_tables()`, `seed_data()`, `verify_rows()`, `drop_tables()`.

6) `tests/test3_sqlite.rs`, `tests/test1.rs`
   - Already table-driven; extract shared helpers into a small test util (e.g., `tests/util/sqlite_like.rs`):
     - `create_test_table(conn)`, `seed_from_sql(conn, &str)`, `select_and_assert(...)`.

7) Bench code (`src/benchmark/common.rs`, `benches/common.rs`)
   - Reduce `generate_*_insert_statements` length by:
     - Extract `render_row(i) -> String`, `join_statements(rows) -> String`.

Acceptance Criteria
- After refactors, longest functions in src/ should be ≤ 50 LOC.
- `tests/test4_AnyConnWrapper.rs` reduced below ~200–250 lines, or split into multiple files under `tests/`.
- No public API changes in library modules; refactors are internal.

Nice-to-Haves
- Add a simple `tests/util/mod.rs` for cross-backend test helpers.
- Where possible, prefer small async helpers over large inlined blocks in tests.

Turso Parity and Follow-ups
---------------------------

Context
- Added Turso transaction helpers: `turso::Tx`, `begin_transaction`, `with_transaction`.
- Extended `test4` to cover Turso using a Turso-specific DDL set derived from SQLite’s.
- Current `turso_core` (0.1.5) translation gaps required relaxing some SQL types/constraints to pass.

Implemented now
- tests/turso/test4 DDL (relaxed where needed):
  - 00_event.sql: DATETIME -> TEXT for `ins_ts`, default literal timestamp; removed AUTOINCREMENT.
  - 02_golfer.sql: DATETIME -> TEXT, default literal timestamp; removed AUTOINCREMENT.
  - 03_bettor.sql: DATETIME -> TEXT, default literal timestamp.
  - 04_event_user_player.sql: prepared with FK/REFERENCES removed and DATETIME -> TEXT, but NOT executed yet.
  - 05_eup_statistic.sql: prepared with JSON affinity -> TEXT and FK removed, but NOT executed yet.
  - setup.sql: currently a no-op; main data setup still uses tests/test4.sql for other backends.

Test adjustments
- For Turso, DDL is applied per-file (other backends batch join).
- For Turso, middleware-based operations mirror LibSQL in test4 while DDL converges.

TODOs (as Turso evolves)
- Re-enable tests/turso/test4/04_event_user_player.sql in Turso DDL list; restore FK REFERENCES and DATETIME defaults.
- Re-enable tests/turso/test4/05_eup_statistic.sql in Turso DDL list; restore JSON affinity, FK REFERENCES, DATETIME defaults.
- Switch Turso DDL execution back to a single batched `execute_batch(ddl.join("\n"))` once stable.
- Expand tests/turso/test4/setup.sql to match tests/test4.sql as constraints become supported.
- Add a dedicated Turso integration test that exercises `with_transaction` end-to-end.
- Clean up `unused mut` warning in `src/turso/transaction.rs`.

LibSQL prepared (wrapper) note
- Our current `libsql::Prepared` is a thin wrapper around the SQL string and executes via the pooled connection.
- If/when a real async `prepare` is exposed by `deadpool-libsql`/`libsql`, we can switch `Prepared` to hold a real Statement under the hood without changing the public API.
- Plan: replace `Prepared { sql: String }` with `Prepared { stmt, cols }` + keep the same `Tx::prepare/execute_prepared/query_prepared` signatures.

- Investigate a `with_sqlite_connection` API so callers can run multiple statements while holding a connection guard. Benchmark the lookup flow using the closure-based guard (single async<->blocking hop) versus today’s per-`execute_select` interact calls.
- If we add the guard, benchmark loops would switch from repeated `execute_select`
  calls to something like:
  ```rust
  MiddlewarePool::with_sqlite_connection(&pool, |conn| {
      let mut stmt = conn.prepare_cached(query)?;
      let mut params = [RowValues::Int(0)];
      for &id in &ids {
          params[0] = RowValues::Int(id);
          let result = sqlite_execute_select_sync(&mut stmt, &params)?;
          let row = result.results.first().unwrap();
          let data = BenchRow::from_result_row(row);
          std::hint::black_box(data);
      }
      Ok(())
  }).await?
  ```
  so the async↔blocking hand-off happens once per batch instead of once per
  lookup.
- Explore swapping `deadpool_sqlite::Object::interact` for a per-connection worker
  task that owns the rusqlite handle and processes an async command queue. That
  would trim per-call closure allocation/wake-ups for existing APIs while still
  letting us layer a `with_sqlite_connection` guard on top for bulk workloads.
  High-level shape would be:
  * spawn a worker task per pooled connection when it’s created;
  * have the worker own the `rusqlite::Connection` and listen on an `mpsc` for
    `Command` structs (e.g. `ExecuteSelect { sql, params, responder }`);
  * have `execute_select` et al. create and send commands instead of calling
    `interact`, then await the response future;
  * ensure drop/cancellation cleanly drains the queue and shuts down the worker.
  Afterwards, layer the `with_sqlite_connection` guard on top so bulk callers
  can send a single “run this closure” command and stay on that worker for the
  whole batch.
