# Benchmarks

The project ships a small set of Criterion benchmarks that target three complementary questions.

- **Bulk insert throughput** for each backend (`benches/database_benchmark.rs`).
- **Single-row lookup overhead** comparing raw `rusqlite` usage with the middleware surface (`benches/bench_rusqlite_single_row_lookup.rs`) and the SQLx harness (`bench-harnesses/sqlx_lookup`).
- **Connection pool fan-out** measuring multi-threaded checkout/query patterns through the middleware and SQLx mirrors (`benches/bench_rusqlite_multithread_pool_checkout.rs`, `bench-harnesses/sqlx_lookup/benches/bench_sqlx_multithread_pool_checkout.rs`).

Use this guide to see how each target is wired, which parts of the stack they exercise, and the adjustments available when running `cargo bench`.

## Benchmark targets
- `database_benchmark` – runs the traditional bulk insert groups for SQLite and PostgreSQL.
- `bench_rusqlite_single_row_lookup` – measures repeated `SELECT ... WHERE id = ?` calls through raw rusqlite and the middleware abstraction (sqlite and turso).
- `bench_rusqlite_multithread_pool_checkout` – fans out the same lookup workload across multiple async workers to isolate connection checkout overheads.
- `bench_turso_single_row_lookup` – covers the Turso deployment path for the single-row lookup scenario.
- SQLx harness targets (stand-alone crate):
  - `sqlite_single_row_lookup_sqlx` – mirrors the single-row lookup benchmark using SQLx.
  - `bench_sqlx_multithread_pool_checkout` – mirrors the multi-thread pool checkout benchmark using SQLx.

Run the bundled benches with `cargo bench`, or focus on a single target via `CRITERION_HOME=$(pwd)/bench_results cargo bench --bench bench_rusqlite_single_row_lookup -- --save-baseline latest`. Criterion substring filters work, e.g. `cargo bench --bench database_benchmark -- sqlite`. To capture middleware & SQLx numbers, we also need the harness crate (since it requires a conflicting libsqlite version):

```shell
CRITERION_HOME=$(pwd)/bench_results cargo bench --bench bench_rusqlite_single_row_lookup -- --save-baseline latest
CRITERION_HOME=$(pwd)/bench_results cargo bench --bench bench_rusqlite_multithread_pool_checkout -- --save-baseline latest
CRITERION_HOME=$(pwd)/bench_results cargo bench --manifest-path bench-harnesses/sqlx_lookup/Cargo.toml -- --save-baseline latest
CRITERION_HOME=$(pwd)/bench_results cargo bench --manifest-path bench-harnesses/sqlx_lookup/Cargo.toml --bench bench_sqlx_multithread_pool_checkout -- --save-baseline latest
CRITERION_HOME=$(pwd)/bench_results cargo bench --bench bench_turso_single_row_lookup -- --save-baseline latest
```

Or, to generate flamegraphs:

```shell
CRITERION_HOME=$(pwd)/bench_results BENCH_LOOKUPS=1000 cargo flamegraph --bench bench_rusqlite_single_row_lookup -- --bench middleware
CRITERION_HOME=$(pwd)/bench_results BENCH_LOOKUPS=1000 BENCH_CONCURRENCY=8 cargo flamegraph --bench bench_rusqlite_multithread_pool_checkout -- --bench middleware_parallel
CRITERION_HOME=$(pwd)/bench_results BENCH_LOOKUPS=1000 cargo flamegraph --manifest-path bench-harnesses/sqlx_lookup/Cargo.toml --bench sqlite_single_row_lookup_sqlx -- --bench sqlx
CRITERION_HOME=$(pwd)/bench_results BENCH_LOOKUPS=1000 BENCH_CONCURRENCY=8 cargo flamegraph --manifest-path bench-harnesses/sqlx_lookup/Cargo.toml --bench bench_sqlx_multithread_pool_checkout -- --bench sqlx_parallel
CRITERION_HOME=$(pwd)/bench_results BENCH_LOOKUPS=1000 cargo flamegraph --bench bench_turso_single_row_lookup -- --bench middleware
```

## Adjustment knobs
- `BENCH_ROWS` controls the number of rows generated for bulk insert runs (default `10`).
- `BENCH_LOOKUPS` controls how many ids are exercised per iteration in the single-row lookup benchmark (falls back to `BENCH_ROWS`, default `1_000`).
- `BENCH_CONCURRENCY` controls the number of worker tasks used in the multi-thread pool checkout benchmarks (default `8`).

## Single-row lookup benchmark flow (`benches/bench_rusqlite_single_row_lookup.rs`)
Current [overall results](../bench_results/index.md). This comparison focuses on per-call overhead for the rusqlite baseline versus the middleware when fetching individual rows by primary key:
1. On first use, build a deterministic SQLite file with `row_count` entries and a shuffled list of ids.
2. `rusqlite` baseline: open a single `rusqlite::Connection`, prepare a cached statement, and loop over the id list with `query_row`, mapping results into `BenchRow`.
3. `sql_middleware` path: construct a `ConfigAndPool`, borrow a pooled connection each iteration, prepare the statement once via `prepare_sqlite_statement`, and then call `PreparedSqliteStatement::query` for every id while recycling a `RowValues` buffer. The first row of each `ResultSet` converts into the same `BenchRow` struct.

Throughput is reported as lookups per iteration. Adjust `BENCH_LOOKUPS` (or `BENCH_ROWS`) to scale the workload.

Additional micro-benches in the same Criterion group isolate specific parts of the middleware stack:
- `pool_acquire` – measures connection checkout/drop latency. Current [results](../bench_results/pool_acquire.md).
- `middleware_prepare` – times statement preparation through the middleware. Current [results](../bench_results/prepare.md).
- `middleware_interact` (legacy name) – measures the worker hand-off when calling `with_blocking_sqlite` on a pooled SQLite handle without executing SQL. Current [results](../bench_results/interact.md).
- `middleware_marshalling` – runs `build_result_set` directly to capture row materialisation cost. Current [results](../bench_results/query_raw.md).
- `middleware_decode` – decodes a cached `CustomDbRow` into the bench struct to isolate row conversion cost. Current [results](../bench_results/decode.md).
- `middleware_param_convert` – benchmarks parameter conversion from `RowValues` into `rusqlite::Value`s. Current [results](../bench_results/param_bind.md).

#### Differences vs SQLx
- Middleware materialises an entire `Vec<CustomDbRow>` for each call before decoding the first row, while the SQLx manual decode path works with the `SqliteRow` returned from the driver and never builds an intermediate result-set buffer. We do this because materializing a Vec<CustomDbRow> is typical usage of the middleware library whereas SQLx calling code would obviously not materialize a `Vec<CustomDbRow>`.
- Middleware reuses and mutates a single `Vec<RowValues>` to avoid allocation during parameter binding; the SQLx harness creates a new builder via `.bind(id)` on every loop iteration. The reusable buffer keeps allocations out of the hot loop for benchmarking; production code can adopt the same pattern when profiles show parameter allocation cost, but many callers today likely build fresh parameter collections (likely a minor perf hit).
- Middleware iterations run through `MiddlewarePool` (backed by `deadpool_sqlite` and a worker thread for SQLite) with a single pooled connection and can emit `BENCH_TRACE` timing breakdowns, whereas the SQLx run uses an `SqlitePool` with up to five connections and no per-row tracing.

### SQLx harness (`bench-harnesses/sqlx_lookup`)
- Rebuilds the same on-disk datasets as the middleware benches (`benchmark_sqlite_single_lookup.db` for single-row, `benchmark_sqlite_multithread_lookup.db` for multi-thread) using SQLx with identical seeds and schemas.
- Times `SqlitePool` plus explicit `prepare()` calls and manual `SqliteRow` → `BenchRow` decoding so the hot loops mirror the middleware benchmarks.
- Lives outside the main crate so it can track the latest SQLx release without conflicting with `rusqlite`'s `libsqlite3-sys` requirements.
- Exposes two primary Criterion groups: `sqlite_single_row_lookup_sqlx` (single-thread lookup) and `sqlite_multithread_pool_checkout_sqlx` (multi-thread fan-out). Each group contains SQLx variants that correspond to the middleware baselines.
- Run them with `cargo bench --manifest-path bench-harnesses/sqlx_lookup/Cargo.toml -- --save-baseline latest` (optionally setting `CRITERION_HOME` as above). Target a specific benchmark binary with `--bench sqlite_single_row_lookup_sqlx` or `--bench bench_sqlx_multithread_pool_checkout`. 

The harness also exposes SQLx-specific micro-benches:
- `sqlx_pool_acquire` – isolates connection acquisition overhead. This is functionally equivalent to middleware micro-benchmark `pool_acquire`.
- `sqlx_prepare` – measures SQLx statement preparation without executing it. This is functionally equivalent to middleware micro-benchmark `middleware_prepare`.
- `sqlx_query_raw` – fetches rows as `SqliteRow` and stops before decoding to isolate driver overhead. This is functionally equivalent to middleware micro-benchmark `middleware_marshalling`.
- `sqlx_decode` – decodes previously fetched `SqliteRow` values into the bench struct. This is functionally equivalent to middleware micro-benchmark `middleware_decode`.
- `sqlx_param_bind` – records the cost of constructing and binding parameters on the query builder. This is functionally equivalent to middleware micro-benchmark `middleware_param_convert`.
- There's no equivalent SQLx micro-benchmark to `middleware_interact` because it times the worker hand-off the middleware uses to run blocking SQLite work off the async runtime. SQLx's sqlite driver is async and doesn't take that detour, so there's nothing similar to measure.

#### Differences vs middleware
- SQLx fetches a single `SqliteRow` per query and decodes immediately, whereas middleware calls populate a vector of `CustomDbRow` values before picking the first entry to decode. This part of the benchmark favors SQLx and is likely more similar to how SQLx users would query the data. 
- SQLx parameter binding relies on the builder API (`query().bind(id)`) each iteration; middleware benchmarks mutate one reusable `RowValues` array so conversion happens in place. Adopt that approach in production only when profiling shows its worth addressing.
- SQLx benches use an `SqlitePool` with a higher max connection count and no optional tracing, while middleware benches run through `MiddlewarePool` (deadpool-based) and can record per-row query/decode timings when `BENCH_TRACE` is enabled.

## Multi-thread pool checkout benchmark flow (`benches/bench_rusqlite_multithread_pool_checkout.rs`)
This group extends the lookup scenario to measure how the middleware and a blocking rusqlite baseline behave when multiple async workers hammer the pool at once:
1. Prepare a shared `benchmark_sqlite_multithread_lookup.db` with deterministic rows and a shuffled id list (reused across runs).
2. Read `BENCH_LOOKUPS` to size the dataset and `BENCH_CONCURRENCY` to decide how many tasks to spawn per iteration.
3. `middleware_parallel_select` – each worker borrows a pooled connection, reuses a prepared statement, and cycles through its chunk of ids. Reflects end-to-end overhead in an async workload.
4. `middleware_pool_checkout` – focuses solely on connection checkout/drop cost by borrowing and returning pooled connections without querying.
5. `rusqlite_blocking` – mirrors the workload with a simple blocking pool to highlight the baseline cost of skipping the middleware entirely.

SQLx mirrors live in `bench-harnesses/sqlx_lookup/benches/bench_sqlx_multithread_pool_checkout.rs` with the same dataset, concurrency controls, and Criterion group name (`sqlite_multithread_pool_checkout_sqlx`). Use those numbers when you need another middleware's reference point.

## Bulk insert benchmark flow (`benches/database_benchmark.rs`)
The insert-oriented groups follow the same high-level pattern:
1. Lazily create and cache a `ConfigAndPool` plus any supporting state (external Postgres, on-disk SQLite file, etc.).
2. Generate a deterministic batch of `INSERT` statements with synthetic payloads.
3. For each timed iteration: drop/recreate the `test` table, borrow a concrete connection from the pool, execute the entire batch inside one transaction, and accumulate the elapsed time.

Because the timed section delegates directly to the backend driver (rusqlite, tokio-postgres), the results mostly reflect the engine’s raw insert throughput along with driver transaction cost. Middleware code is involved only in connection acquisition and dispatch.

### SQLite (`src/benchmark/sqlite.rs`)
- Uses a shared on-disk database (`benchmark_sqlite_global.db`) that is created once per benchmark run and removed during cleanup.
- Executes the batch via `rusqlite::Connection::transaction().execute_batch` inside the timed block.
- The middleware contributes only pool acquisition and the routing that exposes the underlying `rusqlite` connection.

### PostgreSQL (`src/benchmark/postgres.rs`)
- Connects to the configured PostgreSQL instance on first use, creates the schema, and caches the associated connection pool.
- Each iteration recreates the `test` table, then runs the SQL batch inside a single `tokio_postgres` transaction before committing.
- Timings therefore capture Postgres performance and driver costs; middleware code runs only up to yielding the `tokio_postgres::Client`.

## Turso (`benches/bench_turso_single_row_lookup.rs`)
- Runs the single-row lookup suite against a local Turso database (`benchmark_turso_single_lookup.db`), reusing the same dataset across variants.
- Exercises raw Turso queries, middleware prepared statements, pool checkout, and the same micro-benchmarks used to isolate decode/marshalling/param conversion overhead in the SQLite flow.

### Latest results
Checked-in change notes vs last check-in of turso bench results show a regression across the Turso suite. This appears to be driven exclusively by turso 0.3.2 v 0.4.0 (as code paths in this proj are essentially unchanged for turso).
- `turso_raw`: +11.2% mean.
- `middleware`: +11.4% mean.
- `middleware_interact`: +17.3% mean.
- `middleware_marshalling`: +18.0% mean.
- `middleware_prepare`: +22.7% mean.
- `pool_acquire`: +106% mean (about 2.06x).

## Interpreting results
- Treat `database_benchmark` output as a proxy for raw insert bandwidth of each backend/driver pair; it does not capture higher-level middleware helpers such as `QueryAndParams` or cross-backend abstractions.
- Treat `bench_rusqlite_single_row_lookup` output as the relative overhead of routing a point lookup through the middleware versus calling rusqlite directly. Both flows share the same on-disk dataset and decoding logic, so the difference primarily reflects connection dispatch, parameter conversion, and result materialisation cost. Treat the SQLx harness output as a parallel data point for the same workload; compare its metrics with `rusqlite`/middleware results.
- Treat `bench_rusqlite_multithread_pool_checkout` output (and the SQLx mirror) as a measure of how checkout/prepare/query costs scale with concurrency. Look at the sub-benchmarks to separate pure checkout overhead from full query execution.

## An observed complex task for llms:
```shell
git log --oneline --grep="sqlx bench results"
git checkout <sha from above>
"Let's add targeted micro-benches: interact hop-only, marhsalling only, parameter conversion only, and pool check/interact overhead. Make sure to add to the sqlx crate as well as our bench_rusqlite_single_row_lookup."
```
