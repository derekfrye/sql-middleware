# sql-middleware

![Unsafe Forbidden](https://img.shields.io/badge/unsafe-forbidden-success.svg)

Sql-middleware is a lightweight async wrapper for [tokio-postgres](https://crates.io/crates/tokio-postgres), [rusqlite](https://crates.io/crates/rusqlite), [libsql](https://crates.io/crates/libsql), experimental [turso](https://crates.io/crates/turso), and [tiberius](https://crates.io/crates/tiberius) (SQL Server), with bb8-backed pools for Postgres/SQLite (and Turso handles) plus [deadpool](https://github.com/deadpool-rs/deadpool) pools for LibSQL/SQL Server, and an async api. A slim alternative to [SQLx](https://crates.io/crates/sqlx); fewer features, but striving toward a consistent api.

Motivated from trying SQLx and not liking some issue [others already noted](https://www.reddit.com/r/rust/comments/16cfcgt/seeking_advice_considering_abandoning_sqlx_after/?rdt=44192). 

This middleware performance is about 14% faster than SQlx for at least some SQLite workloads. (That could be my misunderstanding of SQLx rather than an inherent performance difference.) For current evidence, see our [benchmark results](/bench_results/index.md).

See also: [API test coverage](docs/api_test_coverage.md) for a map of the public surface to current tests.

## Goals
* Convenience functions for common async SQL query patterns
* Keep underlying flexibility of connection pooling (`bb8` for Postgres/SQLite, `deadpool` where available)
* Minimal overhead (ideally, just syntax sugar/wrapper fns)
* See [Benchmarks](/docs/Benchmarks.md) for details on performance testing.

## Examples

More examples available in [tests](../tests/). Also in-use with a tiny little personal website app, [rusty-golf](https://github.com/derekfrye/rusty-golf).

## Feature Flags

By default, `postgres` and `sqlite` database backends are enabled. You can selectively enable only the backends you need:

```toml
# Only include SQLite and Turso support
sql-middleware = { version = "0", features = ["sqlite", "turso"] }
```

Available features:
- `sqlite`: Enables SQLite support
- `postgres`: Enables PostgreSQL support
- `mssql`: Enables SQL Server support
- `libsql`: Enables LibSQL support (local or remote)
- `turso`: Enables Turso (in-process, SQLite-compatible). Experimental. Uses direct handles (no pool backend yet).
- `default`: Enables common backends (sqlite, postgres). Enable others as needed.
- `test-utils`: Enables test utilities for internal testing

### Parameterized queries for reading or changing data

`QueryAndParams` gives you a single API for both reads and writes through the query builder. The query builder optionally supports same SQL regardless of backend, even with different parameter placeholders ([`$1` or `?1`, with some limitations](#placeholder-translation)). Here is an example that supports PostgreSQL, SQLite, LibSQL, or Turso without duplicating logic.

```rust
use chrono::NaiveDateTime;
use sql_middleware::prelude::*;

pub struct ScoreChange {
    pub espn_id: i64,
    pub score: i32,
    pub updated_at: NaiveDateTime,
}

pub async fn set_scores_in_db(
    config_and_pool: &ConfigAndPool,
    updates: &[ScoreChange],
) -> Result<ResultSet, SqlMiddlewareDbError> {
    let mut conn = config_and_pool.get_connection().await?;

    // Author once; translation rewrites placeholders as needed across backends.
    let insert_sql = "INSERT INTO scores (espn_id, score, updated_at) VALUES ($1, $2, $3)";
    let fetch_sql = "SELECT espn_id, score, updated_at FROM scores ORDER BY updated_at DESC LIMIT $1";

    for change in updates {
        let params = vec![
            RowValues::Int(change.espn_id),
            RowValues::Int(i64::from(change.score)),
            RowValues::Timestamp(change.updated_at),
        ];
        let bound = QueryAndParams::new(insert_sql, params);
        conn.query(&bound.query)
            .translation(TranslationMode::ForceOn)
            .params(&bound.params)
            .dml()
            .await?;
    }

    let limit = (updates.len().max(1)) as i64;
    let latest = QueryAndParams::new(fetch_sql, vec![RowValues::Int(limit)]);
    let rows = conn
        .query(&latest.query)
        .translation(TranslationMode::ForceOn)
        .params(&latest.params)
        .select()
        .await?;

    Ok(rows)
}
```

### Multi-database support without copy/pasting query logic

An example using multiple different backends (sqlite, postgres, turso). Notice the need to not repeat the query logic regardless of backend connection type. 

```rust
use sql_middleware::prelude::*;

pub async fn get_scores_from_db(
    config_and_pool: &ConfigAndPool,
    event_id: i32,
) -> Result<ResultSet, SqlMiddlewareDbError> {
    let mut conn = config_and_pool.get_connection().await?;
    let query = match &conn {
        MiddlewarePoolConnection::Postgres { .. } => {
            "SELECT grp, golfername, playername, eup_id, espn_id FROM sp_get_player_names($1) ORDER BY grp, eup_id"
        }
        MiddlewarePoolConnection::Sqlite { .. } | MiddlewarePoolConnection::Turso { .. } => {
            "SELECT grp, golfername, playername, eup_id, espn_id FROM sp_get_player_names(?1) ORDER BY grp, eup_id"
        }
    };
    let params = vec![RowValues::Int(i64::from(event_id))];
    let res = conn.query(query).params(&params).select().await?;
    Ok(res)
}
```

### Batch query w/o params

Same API regardless of db backend. Full setup, including imports and pool creation. See test8 for compile-ready example.

```rust
use sql_middleware::middleware::ConfigAndPool;
use sql_middleware::prelude::execute_batch;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Choose backend via CLI: `sqlite` (default) or `turso`.
    let app_backend = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "sqlite".to_string());

    let cap = match app_backend.as_str() {
        "sqlite" => ConfigAndPool::sqlite_builder("file::memory:?cache=shared".to_string())
            .build()
            .await?,
        "turso" => ConfigAndPool::turso_builder(":memory:".to_string())
            .build()
            .await?,
        other => return Err(format!("unsupported backend {other}").into()),
    };
    let mut conn = cap.get_connection().await?;

    // simple api for batch queries
    let ddl_query = "CREATE TABLE demo (id INTEGER PRIMARY KEY, name TEXT);";
    // on a pooled connection (auto BEGIN/COMMIT per backend helper)
    conn.execute_batch(&ddl_query).await?;

    // or use the unified top-level helper with either a connection or a transaction
    execute_batch((&mut conn).into(), ddl_query).await?;
    Ok(())
}
```

You can also pass a backend transaction to keep manual control of commit/rollback:
```rust
use sql_middleware::middleware::{ConfigAndPool, MiddlewarePoolConnection, SqlMiddlewareDbError};
use sql_middleware::prelude::execute_batch;

async fn create_temp_table(cap: &ConfigAndPool) -> Result<(), SqlMiddlewareDbError> {
    let mut conn = cap.get_connection().await?;
    let mut tx = match &mut conn {
        MiddlewarePoolConnection::Postgres { client, .. } => {
            sql_middleware::postgres::begin_transaction(client).await?
        }
        _ => {
            return Err(SqlMiddlewareDbError::Unimplemented(
                "expected Postgres connection".to_string(),
            ))
        }
    };

    // run a batch inside the caller-managed transaction
    execute_batch((&mut tx).into(), "CREATE TEMP TABLE t (id INT);").await?;
    // caller decides when to commit/rollback
    tx.commit().await
}
```
### Queries without parameters

You can issue no-parameter queries directly, the same for PostgreSQL, SQLite, LibSQL, and Turso:

```rust
async fn list_users(pool: &ConfigAndPool) -> Result<ResultSet, SqlMiddlewareDbError> {
    let mut conn = pool.get_connection().await?;

    // Either build a QueryAndParams
    let query = QueryAndParams::new_without_params("SELECT * FROM users");
    let _results = conn.query(&query.query).select().await?;

    // Or pass the SQL string directly
    let _results2 = conn.query("SELECT * FROM users").select().await?;

    // Using the unified builder entry point (works with pooled connections or transactions)
    let results3 = sql_middleware::prelude::query((&mut conn).into(), "SELECT * FROM users")
        .select()
        .await?;

    Ok(results3)
}
```

### Custom logic in between transactions

Here, because the underlying libraries are different, the snippets can get chatty. You can still tuck the commit/rollback and dispatch logic behind a couple helpers to avoid repeating the same block across backends.

```rust
use sql_middleware::prelude::*;
use sql_middleware::libsql::{
    begin_transaction as begin_libsql_tx, Prepared as LibsqlPrepared, Tx as LibsqlTx,
};
use sql_middleware::postgres::{
    begin_transaction as begin_postgres_tx, Prepared as PostgresPrepared, Tx as PostgresTx,
};
use sql_middleware::sqlite::{
    begin_transaction as begin_sqlite_tx, Prepared as SqlitePrepared, Tx as SqliteTx,
};
use sql_middleware::turso::{
    begin_transaction as begin_turso_tx, Prepared as TursoPrepared, Tx as TursoTx,
};

enum BackendTx<'conn> {
    Turso(TursoTx<'conn>),
    Postgres(PostgresTx<'conn>),
    Sqlite {
        tx: SqliteTx,
        translate: bool,
    },
    Libsql(LibsqlTx<'conn>),
}

enum PreparedStmt {
    Turso(TursoPrepared),
    Postgres(PostgresPrepared),
    Sqlite(SqlitePrepared),
    Libsql(LibsqlPrepared),
}

impl<'conn> BackendTx<'conn> {
    async fn commit(self) -> Result<Option<MiddlewarePoolConnection>, SqlMiddlewareDbError> {
        match self {
            BackendTx::Turso(tx) => tx.commit().await.map(|_| None),
            BackendTx::Postgres(tx) => tx.commit().await.map(|_| None),
            BackendTx::Sqlite { tx, translate } => {
                let conn = tx.commit().await?;
                // SQLite tx consumes the connection; rewrap so the pooled wrapper remains usable.
                Ok(Some(MiddlewarePoolConnection::from_sqlite_parts(
                    conn, translate,
                )))
            }
            BackendTx::Libsql(tx) => tx.commit().await.map(|_| None),
        }
    }

    async fn rollback(self) -> Result<Option<MiddlewarePoolConnection>, SqlMiddlewareDbError> {
        match self {
            BackendTx::Turso(tx) => tx.rollback().await.map(|_| None),
            BackendTx::Postgres(tx) => tx.rollback().await.map(|_| None),
            BackendTx::Sqlite { tx, translate } => {
                let conn = tx.rollback().await?;
                // Same as commit: rewrap after rollback to avoid "connection already taken".
                Ok(Some(MiddlewarePoolConnection::from_sqlite_parts(
                    conn, translate,
                )))
            }
            BackendTx::Libsql(tx) => tx.rollback().await.map(|_| None),
        }
    }
}

impl PreparedStmt {
    async fn query_prepared(
        &mut self,
        tx: &mut BackendTx<'_>,
        params: &[RowValues],
    ) -> Result<ResultSet, SqlMiddlewareDbError> {
        match (tx, self) {
            (&BackendTx::Turso(tx), PreparedStmt::Turso(stmt)) => {
                tx.query_prepared(stmt, params).await
            }
            (&BackendTx::Postgres(tx), PreparedStmt::Postgres(stmt)) => {
                tx.query_prepared(stmt, params).await
            }
            (&mut BackendTx::Sqlite(ref mut tx), PreparedStmt::Sqlite(stmt)) => {
                tx.query_prepared(stmt, params).await
            }
            (&BackendTx::Libsql(tx), PreparedStmt::Libsql(stmt)) => {
                tx.query_prepared(stmt, params).await
            }
        }
    }
}

async fn run_prepared_with_finalize<'conn>(
    mut tx: BackendTx<'conn>,
    mut stmt: PreparedStmt,
    params: Vec<RowValues>,
    restored: &mut Option<MiddlewarePoolConnection>,
) -> Result<ResultSet, SqlMiddlewareDbError> {
    let result = stmt.query_prepared(&mut tx, &params).await;
    match result {
        Ok(rows) => {
            if let Some(conn) = tx.commit().await? {
                // Hand connection back to caller so the pooled wrapper remains usable.
                *restored = Some(conn);
            }
            Ok(rows)
        }
        Err(e) => {
            if let Some(conn) = tx.rollback().await.unwrap_or(None) {
                *restored = Some(conn);
            }
            Err(e)
        }
    }
}

pub async fn get_scores_from_db(
    config_and_pool: &ConfigAndPool,
    event_id: i64,
) -> Result<ResultSet, SqlMiddlewareDbError> {
    let mut conn = config_and_pool.get_connection().await?;

    // Author once; translate for SQLite-family backends when preparing.
    let base_query = "SELECT grp, golfername, playername, eup_id, espn_id \
                      FROM sp_get_player_names($1) ORDER BY grp, eup_id";
    let (tx, stmt) = match &mut conn {
        MiddlewarePoolConnection::Turso { conn: client, .. } => {
            let tx = begin_turso_tx(client).await?;
            let q = translate_placeholders(base_query, PlaceholderStyle::Sqlite, true);
            let stmt = tx.prepare(q.as_ref()).await?;
            (BackendTx::Turso(tx), PreparedStmt::Turso(stmt))
        }
        MiddlewarePoolConnection::Postgres {
            client: pg_conn, ..
        } => {
            let tx = begin_postgres_tx(pg_conn).await?;
            let stmt = tx.prepare(base_query).await?;
            (BackendTx::Postgres(tx), PreparedStmt::Postgres(stmt))
        }
        MiddlewarePoolConnection::Sqlite {
            conn,
            translate_placeholders,
        } => {
            // Take ownership of the SQLite connection (tx API consumes it).
            let sqlite_conn = conn
                .take()
                .ok_or_else(|| {
                    SqlMiddlewareDbError::ExecutionError(
                        "SQLite connection already taken".to_string(),
                    )
            })?;
            let mut tx = begin_sqlite_tx(sqlite_conn).await?;
            let q = translate_placeholders(base_query, PlaceholderStyle::Sqlite, true);
            let stmt = tx.prepare(q.as_ref())?;
            // Preserve the pool's translation default when rewrapping.
            let translate = *translate_placeholders;
            (BackendTx::Sqlite { tx, translate }, PreparedStmt::Sqlite(stmt))
        }
        MiddlewarePoolConnection::Libsql { conn, .. } => {
            let tx = begin_libsql_tx(conn).await?;
            let q = translate_placeholders(base_query, PlaceholderStyle::Sqlite, true);
            let stmt = tx.prepare(q.as_ref())?;
            (BackendTx::Libsql(tx), PreparedStmt::Libsql(stmt))
        }
        _ => {
            return Err(SqlMiddlewareDbError::Unimplemented(
                "expected Turso, Postgres, SQLite, or LibSQL connection".to_string(),
            ));
        }
    };

    // Build params however you like in your business logic.
    let dynamic_params = vec![RowValues::Int(event_id)];
    let mut restored = None;
    let rows = run_prepared_with_finalize(tx, stmt, dynamic_params, &mut restored).await?;
    if let Some(restored_conn) = restored.take() {
        // Reinsert SQLite connection so this pooled wrapper stays usable for subsequent work.
        *conn = restored_conn;
    }
    Ok(rows)
}
```

### Using the query builder in helpers

```rust
// This works for PostgreSQL, SQLite, LibSQL, and Turso connections
async fn insert_user(
    conn: &mut MiddlewarePoolConnection,
    user_id: i32,
    name: &str,
) -> Result<(), SqlMiddlewareDbError> {
    let query = QueryAndParams::new(
        // Author once; translation rewrites placeholders for SQLite-family backends.
        "INSERT INTO users (id, name) VALUES ($1, $2)",
        vec![
            RowValues::Int(i64::from(user_id)),
            RowValues::Text(name.to_string()),
        ],
    );

    conn.query(&query.query)
        .translation(TranslationMode::ForceOn)
        .params(&query.params)
        .dml()
        .await?;

    Ok(())
}
```

### Further examples

See further examples in the tests directory:
- [SQLite test example](/tests/test05c_sqlite.rs), [SQLite bench example 1](../benches/bench_rusqlite_single_row_lookup.rs), [SQLite bench example 2](../benches/bench_rusqlite_multithread_pool_checkout.rs)
- [Turso test example](/tests/test05d_turso.rs), [Turso bench example 1](../benches/bench_turso_single_row_lookup.rs)
- [PostgreSQL test example](/tests/test05a_postgres.rs)
- [LibSQL test example](/tests/test05b_libsql.rs)

## Placeholder Translation

- Default off. Enable at pool creation via backend options/builders (e.g., `PostgresOptions::new(cfg).with_translation(true)` or `ConfigAndPool::sqlite_builder(path).translation(true)`) to translate SQLite-style `?1` to Postgres `$1` (or the inverse) automatically for parameterised calls.
- Override per call via the query builder: `.translation(TranslationMode::ForceOff | ForceOn)` or `.options(...)`.
- Manual path: `translate_placeholders(sql, PlaceholderStyle::{Postgres, Sqlite}, enabled)` to reuse translated SQL with your own prepare/execute flow.
- *Limitations*: Translation runs only when parameters are non-empty and skips quoted strings, identifiers, comments, and dollar-quoted blocks; MSSQL is left untouched. Basically, don't rely on this to try to translate `?X` to `$X` in complicated, per-dialect specific stuff (like `$tag$...$tag$` in postgres, this translation is meant to cover 90% of use cases).
- More design notes and edge cases live in [documentation of the feature](./docs/feat_translation.md).

```rust
use sql_middleware::prelude::*;

let mut conn = config_and_pool.get_connection().await?;
let rows = conn
    .query("select * from t where id = $1")
    .translation(TranslationMode::ForceOn)
    .params(&[RowValues::Int(1)])
    .select()
    .await?;
```

## Developing and Testing

- Build with defaults (sqlite, postgres): `cargo build`
- Include Turso backend: `cargo build --features turso`
- Run tests (defaults): `cargo test` or `cargo nextest run`
    - Notice that `test4_trait` does have hard-coded testing postgres connection strings. I can't get codex to work with postgres embedded anymore, so when working on this test w codex I've hardcoded those values so I can work around it's lack of network connectivity. You'll have to change them if you want that test to compile in your environment. 
- Run with Turso: `cargo test --features turso`
- Run with LibSQL: `cargo test --features libsql`

### Our use of `[allow(...)]`s

- `#[allow(clippy::unused_async)]` keeps public constructors async so the signature stays consistent even when the current body has no awaits. You’ll see this on `ConfigAndPool::new_postgres` (src/postgres/config.rs), `ConfigAndPool::new_mssql` (src/mssql/config.rs), and `MiddlewarePool::get` (src/pool/types.rs). We also call out the rationale in **[Async Design Decisions](async.md)**.
- `#[allow(clippy::useless_conversion)]` is used once to satisfy `rusqlite::params_from_iter`, which requires an iterator type that Clippy would otherwise collapse away (`src/sqlite/params.rs:79`).
- `#[allow(unreachable_patterns)]` guards catch-all branches that only fire when a backend feature is disabled, preventing false positives when matching on `MiddlewarePoolConnection` (`src/pool/connection.rs:102`, `src/executor.rs:64`, `src/executor.rs:97`, `src/executor.rs:130`, `src/pool/interaction.rs:40`, `src/pool/interaction.rs:78`).
- `#[allow(unused_variables)]` appears around the interaction helpers because the higher-order functions take arguments that are only needed for certain backend combinations (`src/pool/interaction.rs:10`, `src/pool/interaction.rs:51`).

## Release Notes

- 0.3.0: Defaulted to the fluent query builder for prepared statements (older `execute_select`/`execute_dml` helpers on `MiddlewarePoolConnection` were removed), expanded placeholder translation docs and examples, switched pool constructors to backend options + builders (instead of per-feature constructor permutations), and improved Postgres integer binding to downcast to `INT2/INT4` when inferred.
- 0.1.9: Switched the project license from BSD-2-Clause to MIT, added third-party notice documentation, and introduced optional placeholder translation (pool defaults + per-call `QueryOptions`).
