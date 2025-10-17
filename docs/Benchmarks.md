# Benchmarks

The project ships a small set of Criterion benchmarks that target two different questions:

- **Bulk insert throughput** for each backend (`benches/database_benchmark.rs`).
- **Single-row lookup overhead** comparing raw `rusqlite` usage with the middleware surface (`benches/bench_rusqlite_single_row_lookup.rs`).
- **SQLx lookup reference** in a stand-alone harness (`bench-harnesses/sqlx_lookup`).

Use this guide to see how each target is wired, which parts of the stack they exercise, and the knobs available when running `cargo bench`.

## Benchmark targets
- `database_benchmark` – runs the traditional bulk insert groups for SQLite and PostgreSQL (LibSQL remains opt-in behind the `libsql` feature).
- `bench_rusqlite_single_row_lookup` – measures repeated `SELECT ... WHERE id = ?` calls through raw rusqlite and the middleware abstraction.
- `sqlx_lookup_bench` (stand-alone crate) – measures the same lookup workload using SQLx in isolation.

Run the bundled benches with `cargo bench`, or focus on a single target via `CRITERION_HOME=$(pwd)/bench_results cargo bench --bench bench_rusqlite_single_row_lookup -- --save-baseline latest`. Criterion substring filters work, e.g. `cargo bench --bench database_benchmark -- sqlite`. To capture middleware & SQLx numbers, we also need the harness crate: 

```shell
CRITERION_HOME=$(pwd)/bench_results cargo bench --bench bench_rusqlite_single_row_lookup -- --save-baseline latest
CRITERION_HOME=$(pwd)/bench_results cargo bench --manifest-path bench-harnesses/sqlx_lookup/Cargo.toml -- --save-baseline latest
```

Or, to generate flamegraphs:

```shell
CRITERION_HOME=$(pwd)/bench_results BENCH_LOOKUPS=1000 cargo flamegraph --bench bench_rusqlite_single_row_lookup -- --bench middleware
CRITERION_HOME=$(pwd)/bench_results BENCH_LOOKUPS=1000 cargo flamegraph --manifest-path bench-harnesses/sqlx_lookup/Cargo.toml --bench sqlite_single_row_lookup_sqlx -- --bench sqlx
```

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
This comparison focuses on per-call overhead for the rusqlite baseline versus the middleware when fetching individual rows by primary key:
1. On first use, build a deterministic SQLite file with `row_count` entries and a shuffled list of ids.
2. `rusqlite` baseline: open a single `rusqlite::Connection`, prepare a cached statement, and loop over the id list with `query_row`, mapping results into `BenchRow`.
3. `sql_middleware` path: construct a `ConfigAndPool`, borrow a pooled connection each iteration, prepare the statement once via `prepare_sqlite_statement`, and then call `PreparedSqliteStatement::query` for every id while recycling a `RowValues` buffer. The first row of each `ResultSet` converts into the same `BenchRow` struct.

Throughput is reported as lookups per iteration. Adjust `BENCH_LOOKUPS` (or `BENCH_ROWS`) to scale the workload.

Additional micro-benches in the same Criterion group isolate specific parts of the middleware stack:
- `middleware_checkout` – measures connection checkout/drop latency.
- `middleware_interact` – times the `deadpool_sqlite::Object::interact` hop with an empty closure.
- `middleware_marshalling` – runs `build_result_set` directly to capture row materialisation cost.
- `middleware_param_convert` – benchmarks parameter conversion from `RowValues` into `rusqlite::Value`s.

### SQLx harness (`bench-harnesses/sqlx_lookup`)
- Rebuilds the same on-disk dataset (`benchmark_sqlite_single_lookup.db` in the repo root) using SQLx with the identical seed and schema.
- Times `SqlitePool` plus an explicit `prepare()` + manual `SqliteRow` → `BenchRow` decode, so the hot loop mirrors the middleware benchmark’s prepared-statement reuse.
- Lives outside the main crate so it can track the latest SQLx release without conflicting with `rusqlite`'s `libsqlite3-sys` requirements.
- Run it with `cargo bench --manifest-path bench-harnesses/sqlx_lookup/Cargo.toml -- --save-baseline latest` (optionally setting `CRITERION_HOME` as above).

The harness also exposes SQLx-specific micro-benches:
- `sqlx` – explicit prepared statement + manual `SqliteRow` → `BenchRow` decoding, mirroring the middleware benchmark.
- `sqlx_query_raw` – fetches rows as `SqliteRow` and stops before decoding to isolate driver overhead.
- `sqlx_pool_acquire` – isolates connection acquisition overhead.
- `sqlx_param_bind` – records the cost of constructing and binding parameters on the query builder.

## Interpreting results
- Treat `database_benchmark` output as a proxy for raw insert bandwidth of each backend/driver pair; it does not capture higher-level middleware helpers such as `QueryAndParams` or cross-backend abstractions.
- Treat `bench_rusqlite_single_row_lookup` output as the relative overhead of routing a point lookup through the middleware versus calling rusqlite directly. Both flows share the same on-disk dataset and decoding logic, so the difference primarily reflects connection dispatch, parameter conversion, and result materialisation cost.
- Treat the SQLx harness output as a parallel data point for the same workload; compare its metrics with `rusqlite`/middleware results to gauge how your middleware stacks up against a modern async client.
- When designing additional benchmarks, decide whether you want engine-level comparisons (like the bulk inserts) or API-level comparisons (like the single-row lookup) and structure the workload accordingly.

## Great llm test:
```shell
git log --oneline --grep="sqlx bench results"
git checkout <sha from above>
"Let's add targeted micro-benches: interact hop-only, marhsalling only, parameter conversion only, and pool check/interact overhead. Make sure to add to the sqlx crate as well as our bench_rusqlite_single_row_lookup."
```
