# SQLite single-row lookup regression (8a3bb77 → HEAD)

- Baseline bench commit: `8a3bb77` (2025-11-25, “benches ck'd after big chgs”).
- Current benches: `bench_results/sqlite_single_row_lookup/middleware/1000/{latest,new}/estimates.json`.
- Observed regression: mean ~19.05 ms (baseline) → ~23.30 ms (current); median ~18.22 ms → ~21.26 ms. Criterion change summary: +21.5% mean, +17.3% median (`change/estimates.json`).

Key code changes since baseline that affect this benchmark
- SQLite prepared statements now re-prepare on every call: `SqlitePreparedStatement::query/execute` (`src/sqlite/prepared.rs`) calls `SqliteConnection::execute_select/execute_dml`, which `prepare` a fresh statement each time (`src/sqlite/connection.rs`). The old worker-backed path used `prepare_cached` for prepared statements (`8a3bb77:src/sqlite/worker/dispatcher.rs`), so the benchmark now recompiles the statement for each of ~1000 IDs per iteration.
- SQLite backend refactor removed the dedicated worker thread in favor of a bb8 pool of `tokio::sync::Mutex<rusqlite::Connection>` with `spawn_blocking` per call (`src/sqlite/config.rs`, `src/sqlite/connection.rs`). Each lookup now takes a mutex lock and a `spawn_blocking` hop, adding per-call latency compared to the prior worker-channel model.
- Benchmark harness changes are minimal (switched to `sqlite_builder` and mutable prepared handles), so the regression points to the middleware path rather than the bench logic.

Likely fixes to regain performance
- Restore cached prepared execution for SQLite prepared statements (reuse `prepare_cached` when executing) and avoid re-preparing inside `SqlitePreparedStatement`.
- Evaluate the bb8 + `spawn_blocking` overhead vs. the previous worker-thread approach for short, single-row lookups; consider restoring a dedicated worker or reducing locking/`spawn_blocking` hops on the hot path.
  - Wrap bb8 pooled connections in a dedicated worker thread (old deadpool pattern) to remove per-call `spawn_blocking`/`tokio::Mutex` overhead while keeping bb8 pooling.
  - If keeping `spawn_blocking`, batch work: keep the mutex/connection locked for a batch (or a prepared statement’s lifetime) so one `spawn_blocking` covers many calls.
  - Use a non-async mutex (e.g., `parking_lot::Mutex`) and lock inside the blocking section to cut async mutex overhead on hot paths.
  - Pre-warm and reuse prepared statements inside the blocking section (or per-connection cache) to avoid repeated `prepare` calls on every query execution.
  - If “no extra hops” is required, revert to a worker-thread-backed connection (even without deadpool) to mirror the prior behavior.
