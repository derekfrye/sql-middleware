# Benchmarks

The project ships a small set of Criterion benchmarks that target two different questions:

- **Bulk insert throughput** for each backend (`benches/database_benchmark.rs`).
- **Single-row lookup overhead** comparing raw `rusqlite` usage with the middleware surface (`benches/bench_rusqlite_single_row_lookup.rs`).

Use this guide to see how each target is wired, which parts of the stack they exercise, and the knobs available when running `cargo bench`.

## Benchmark targets
- `database_benchmark` – runs the traditional bulk insert groups for SQLite and PostgreSQL (LibSQL remains opt-in behind the `libsql` feature).
- `bench_rusqlite_single_row_lookup` – measures repeated `SELECT ... WHERE id = ?` calls through raw rusqlite, the middleware abstraction, and SQLx.

Run the whole suite with `cargo bench`, or focus on a single target via `CRITERION_HOME=bench_results  cargo bench --bench bench_rusqlite_single_row_lookup -- --save-baseline latest`. Criterion substring filters still apply, e.g. `cargo bench --bench database_benchmark -- sqlite`.

## Configuration knobs
- `BENCH_ROWS` controls the number of rows generated for bulk insert runs (default `10`).
- `BENCH_LOOKUPS` controls how many ids are exercised per iteration in the single-row lookup benchmark (falls back to `BENCH_ROWS`, default `1_000`).

Set either variable inline, for example `BENCH_ROWS=1000 cargo bench --bench database_benchmark`.

## Bulk insert benchmark flow (`benches/database_benchmark.rs`)
The insert-oriented groups follow the same high-level pattern:
1. Lazily create and cache a `ConfigAndPool` plus any supporting state (embedded Postgres, on-disk SQLite file, etc.).
2. Generate a deterministic batch of `INSERT` statements with synthetic payloads.
3. For each timed iteration: drop/recreate the `test` table, borrow a concrete connection from the pool, execute the entire batch inside one transaction, and accumulate the elapsed time.

Because the timed section delegates directly to the backend driver (rusqlite, tokio-postgres, libsql), the results mostly reflect the engine’s raw insert throughput along with driver transaction cost. Middleware code is involved only in connection acquisition and dispatch.

### SQLite (`src/benchmark/sqlite.rs`)
- Uses a shared on-disk database (`benchmark_sqlite_global.db`) that is created once per benchmark run and removed during cleanup.
- Executes the batch via `rusqlite::Connection::transaction().execute_batch` inside the timed block.
- The middleware contributes only pool acquisition and the routing that exposes the underlying `rusqlite` connection.

### PostgreSQL (`src/benchmark/postgres.rs`)
- Spins up an embedded PostgreSQL instance on first use, creates the schema, and caches both the instance and the associated connection pool.
- Each iteration recreates the `test` table, then runs the SQL batch inside a single `tokio_postgres` transaction before committing.
- Timings therefore capture embedded Postgres performance and driver costs; middleware code runs only up to yielding the `tokio_postgres::Client`.

### LibSQL (`src/benchmark/libsql.rs`)
- Mirrors the SQLite flow but targets LibSQL, optionally using an on-disk file or an in-memory database (`:memory:` by default).
- Invokes `crate::libsql::execute_batch` on the direct LibSQL connection during the timed section.
- Currently excluded from the default `criterion_main!`, so it runs only when you enable the `libsql` feature and rewire the benchmark entry point.

## Single-row lookup benchmark flow (`benches/bench_rusqlite_single_row_lookup.rs`)
This comparison focuses on per-call overhead for different client layers when fetching individual rows by primary key:
1. On first use, build a deterministic SQLite file with `row_count` entries and a shuffled list of ids.
2. `rusqlite` baseline: open a single `rusqlite::Connection`, prepare a cached statement, and loop over the id list with `query_row`, mapping results into `BenchRow`.
3. `sql_middleware` path: construct a `ConfigAndPool`, borrow a pooled connection, and call `execute_select` for each id while recycling a `RowValues` buffer. The returned `ResultSet` converts into the same `BenchRow` struct.
4. `sqlx` path: create a `SqlitePool`, then issue `query_as` lookups for each id using the async driver stack.

Throughput is reported as lookups per iteration. Adjust `BENCH_LOOKUPS` (or `BENCH_ROWS`) to scale the workload.

## Interpreting results
- Treat `database_benchmark` output as a proxy for raw insert bandwidth of each backend/driver pair; it does not capture higher-level middleware helpers such as `QueryAndParams` or cross-backend abstractions.
- Treat `bench_rusqlite_single_row_lookup` output as the relative overhead of routing a point lookup through each client layer (rusqlite, middleware, SQLx). All flows share the same dataset and decoding logic so differences primarily reflect connection dispatch, parameter conversion, and result materialisation cost.
- When designing additional benchmarks, decide whether you want engine-level comparisons (like the bulk inserts) or API-level comparisons (like the single-row lookup) and structure the workload accordingly.
