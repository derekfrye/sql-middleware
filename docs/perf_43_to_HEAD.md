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

## Fix status
- `SqliteConnection` now uses `prepare_cached` for select and DML helpers so warmed statements are reused instead of recompiling every query.
- The SQLite pool now wraps each rusqlite connection in a dedicated worker thread driven by a `crossbeam-channel` queue, removing per-call `tokio::spawn_blocking` hops and mutex locking on the hot path.

## TODO
1. Reintroduce a per-connection worker to remove per-call `spawn_blocking` overhead while keeping bb8 pooling (or otherwise batch multiple lookups under one blocking task).
   - Status: implemented with a crossbeam-channel-backed worker thread per pooled connection; async paths now dispatch to the worker instead of `spawn_blocking`.
   - Expected churn: contained to the SQLite pool/connection plumbing (~150–250 LOC). Likely touch `src/sqlite/config.rs` (manager returns worker-backed handles instead of `Arc<Mutex<Connection>>`), `src/sqlite/connection/core.rs` and `typed/core.rs` (new “send to worker” helper replacing `spawn_blocking + lock`), `select.rs`/`dml.rs` call sites, and `typed/tx.rs` drop path plus related tests (e.g., `tests/test10_bad_drop.rs`). Public surface can stay stable.
2. Re-run `sqlite_single_row_lookup/middleware/1000` after the worker change to see if the ~3–4 ms gap to the pre-bb8 baseline closes.
   - Status: completed; latest run shows ~14.16 ms mean / 13.92 ms median (change: -39.5% mean, CI [-42.56%, -36.91%]), beating the ~19 ms baseline.
   - Expected churn: light; re-run Criterion bench and capture new `bench_results/sqlite_single_row_lookup/middleware/1000/{latest,new}` artifacts.
3. Verify prepared statements stay hot across iterations (consider warming once per checkout and reusing handles) if the worker change alone doesn’t recover the baseline.
   - Expected churn: small to medium depending on approach. Likely localized to prepared-statement creation/checkout flow; may need a per-connection cache or “warm on checkout” path plus minor test adjustments.
