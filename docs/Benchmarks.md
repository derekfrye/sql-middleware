# Benchmarks

This project ships Criterion benchmarks that focus on bulk `INSERT` performance for each supported backend. They exercise the middleware just enough to obtain a pooled connection, then measure how quickly the underlying database executes a batch of generated SQL statements. Use this guide to understand what happens when you run `cargo bench` and how to interpret the results.

## Running the suite
- `cargo bench` compiles `benches/database_benchmark.rs` and executes the `sqlite` and `postgres` Criterion groups by default. The LibSQL group is compiled only when the `libsql` feature is enabled, and it is currently excluded from the `criterion_main!` macro.
- Row counts are controlled by the `BENCH_ROWS` environment variable (default: `10`). The helper `generate_*_insert_statements` functions in `benches/common.rs` prebuild a single SQL string containing that many `INSERT` statements, including JSON, blob, and text payloads.
- Criterion’s `iter_custom` API wraps the actual work. Only the time spent inside the database execution block contributes to the reported measurements; setup, teardown, and pool access run outside the timed section.

## Shared execution pattern
1. Fetch (or lazily construct) a cached `ConfigAndPool` for the targeted backend.
2. Drop and recreate the `test` table to guarantee a clean slate before every timed iteration.
3. Borrow a concrete connection from the middleware pool.
4. Start a transaction, execute the pre-generated SQL batch, commit, and accumulate the elapsed duration.

Because the batch string is executed verbatim against the backend-specific driver, the results primarily report the database engine’s raw insert throughput plus the driver’s transaction overhead. Middleware logic beyond connection dispatch is not part of the measurement window.

## Backend specifics
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

## Interpreting results
- Treat the reported numbers as the baseline insert throughput of each backend’s native driver under a controlled payload. They are **not** an end-to-end measurement of higher-level middleware features (query builders, parameter binding helpers, etc.).
- If you need middleware-level benchmarks—e.g., measuring `QueryAndParams` preparation or cross-backend abstractions—you will need to add dedicated Criterion groups that exercise those code paths instead of calling the raw driver APIs directly.
- Use the `BENCH_ROWS` variable to scale workload size, but remember that all rows are inserted within a single transaction per iteration; adjust the workload generator if you need different access patterns.
