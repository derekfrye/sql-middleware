# sql-middleware

![Unsafe Forbidden](https://img.shields.io/badge/unsafe-forbidden-success.svg)

Sql-middleware is a lightweight async wrapper for [tokio-postgres](https://crates.io/crates/tokio-postgres), [rusqlite](https://crates.io/crates/rusqlite), [libsql](https://crates.io/crates/libsql), experimental [turso](https://crates.io/crates/turso), and [tiberius](https://crates.io/crates/tiberius) (SQL Server), with [deadpool](https://github.com/deadpool-rs/deadpool) connection pooling (except Turso, which doesn't have deadpool backend yet), and an async api. A slim alternative to [SQLx](https://crates.io/crates/sqlx); fewer features, but striving toward a consistent api.

Motivated from trying SQLx and not liking some issue [others already noted](https://www.reddit.com/r/rust/comments/16cfcgt/seeking_advice_considering_abandoning_sqlx_after/?rdt=44192). 

This middleware performance is about 14% faster than SQlx for at least some SQLite workloads. (That could be my misunderstanding of SQLx rather than an inherent performance difference.) For current evidence, see our [benchmark results](/bench_results/index.md).

## Goals
* Convenience functions for common async SQL query patterns
* Keep underlying flexibility of `deadpool` connection pooling
* Minimal overhead (ideally, just syntax sugar/wrapper fns)
* See [Benchmarks](/docs/Benchmarks.md) for details on performance testing.

## Examples

More examples available in [tests](../tests/). Also in-use with a tiny little personal website app, [rusty-golf](https://github.com/derekfrye/rusty-golf).

### Importing

You can use the prelude to import everything you need, or import item by item.

```rust
use sql_middleware::prelude::*;
```


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
    config_and_pool: &sql_middleware::pool::ConfigAndPool,
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
    config_and_pool: &sql_middleware::pool::ConfigAndPool,
    event_id: i32,
) -> Result<ScoresAndLastRefresh, SqlMiddlewareDbError> {
    let mut conn = config_and_pool.get_connection().await?;
    let query = match &conn {
        MiddlewarePoolConnection::Postgres { .. } => {
            "SELECT grp, golfername, playername, eup_id, espn_id FROM sp_get_player_names($1) ORDER BY grp, eup_id"
        }
        MiddlewarePoolConnection::Sqlite { .. } | MiddlewarePoolConnection::Turso { .. } => {
            include_str!("../sql/functions/sqlite/03_sp_get_scores.sql")
        }
    };
    let params = vec![RowValues::Int(i64::from(event_id))];
    let res = conn.query(query).params(&params).select().await?;

    let z: Result<Vec<Scores>, SqlMiddlewareDbError> = res
        .results
        .iter()
        .map(|row| {
            Ok(Scores {
                golfer_name: row
                    .get("golfername")
                    .and_then(|v| v.as_text())
                    .unwrap_or_default()
                    .to_string(),
                detailed_statistics: Statistic {
                    ...<more retrieved fields here>
                },
            })
        })
        .collect::<Result<Vec<Scores>, SqlMiddlewareDbError>>();

    Ok(ScoresAndLastRefresh {
        score_struct: z?,
    })
}
```

### Get a connection from the pool

Similar api regardless of db backend.

<table>
<tr>
<th>
PostgreSQL
</th>
<th>
SQLite / LibSQL / Turso
</th>
</tr>
<tr>
<td>

```rust
let mut cfg = deadpool_postgres
    ::Config::new();
cfg.dbname = Some("test_db"
    .to_string());
cfg.host = Some("192.168.2.1"
    .to_string());
cfg.port = Some(5432);
cfg.user = Some("test user"
    .to_string());
cfg.password = Some("passwd"
    .to_string());

let c = ConfigAndPool
    ::new_postgres(cfg)
    .await?;
let mut conn = c.get_connection()
    .await?;

```

</td>
<td>

```rust
let cfg = 
    "file::memory:?cache=shared"
    .to_string();
// Or file-based:
// let cfg = "./data.db".to_string();





// same api for connection
// sqlite just has fewer required 
// config items (no port, etc.)
let c = ConfigAndPool::
    new_sqlite(cfg)
    .await?;
let mut conn = c.get_connection()
    .await?;

```

</td>
</tr>
</table>

Note: The SQLite example applies to SQLite, LibSQL, and Turso. Swap the constructor as needed: `new_sqlite(path)`, `new_libsql(path)`, or `new_turso(path)`. For Turso, there’s no deadpool pooling; `get_connection` creates a fresh connection.

SQLite pooling now uses a dedicated worker thread so blocking `rusqlite` calls do not hold the async runtime. Two helpers expose that surface when you need it:

```rust
use sql_middleware::prelude::*;

let cap = ConfigAndPool::new_sqlite("file::memory:?cache=shared".into()).await?;
let mut conn = cap.get_connection().await?;

// Borrow the raw rusqlite::Connection for batched work on the worker thread.
conn.with_sqlite_connection(|raw| {
    let tx = raw.transaction()?;
    tx.execute_batch("CREATE TABLE t (id INTEGER PRIMARY KEY, name TEXT);")?;
    tx.execute("INSERT INTO t (name) VALUES (?1)", ["alice"])?;
    tx.commit()?;
    Ok::<_, SqlMiddlewareDbError>(())
})
.await?;

// Prepare once and reuse the cached statement via the worker.
let prepared = conn
    .prepare_sqlite_statement("SELECT name FROM t WHERE id = ?1")
    .await?;
let rows = prepared.query(&[RowValues::Int(1)]).await?;
assert_eq!(rows.results[0].get("name").unwrap().as_text().unwrap(), "alice");
```

### Batch query w/o params

Same api regardless of db backend. Use `execute_batch` when you have no parameters to pass. 

```rust
// simple api for batch queries
let ddl_query =
    include_str!("/path/to/test1.sql");
conn.execute_batch(&ddl_query).await?;
```
### Queries without parameters

You can issue no-parameter queries directly, the same for PostgreSQL, SQLite, LibSQL, and Turso:

```rust
// Either build a QueryAndParams
let query = QueryAndParams::new_without_params("SELECT * FROM users");
let results = conn.query(&query.query).select().await?;

// Or pass the SQL string directly
let results2 = conn.query("SELECT * FROM users").select().await?;
```

### Custom logic in between transactions

Here, because the underlying libraries are different, unfortunately, if you need custom app logic between `transaction()` and `commit()`, the code becomes a little less DRY.

```rust
use sql_middleware::libsql::{
    begin_transaction as begin_libsql_tx, Params as LibsqlParams, Prepared as LibsqlPrepared,
    Tx as LibsqlTx,
};
use sql_middleware::prelude::*;
use sql_middleware::sqlite::{
    begin_transaction as begin_sqlite_tx, Params as SqliteParams, Prepared as SqlitePrepared,
    Tx as SqliteTx,
};
use sql_middleware::turso::{
    begin_transaction as begin_turso_tx, Params as TursoParams, Prepared as TursoPrepared,
    Tx as TursoTx,
};
use sql_middleware::postgres::{
    begin_transaction as begin_postgres_tx, Params as PostgresParams, Prepared as PostgresPrepared,
    Tx as PostgresTx,
};

enum BackendTx<'conn> {
    Turso(TursoTx<'conn>),
    Postgres(PostgresTx<'conn>),
    Sqlite(SqliteTx<'conn>),
    Libsql(LibsqlTx<'conn>),
}

enum PreparedStmt {
    Turso(TursoPrepared),
    Postgres(PostgresPrepared),
    Sqlite(SqlitePrepared),
    Libsql(LibsqlPrepared),
}

pub async fn get_scores_from_db(
    config_and_pool: &ConfigAndPool,
    event_id: i64,
) -> Result<ScoresAndLastRefresh, SqlMiddlewareDbError> {
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
        MiddlewarePoolConnection::Libsql { conn, .. } => {
            let tx = begin_libsql_tx(conn).await?;
            let q = translate_placeholders(base_query, PlaceholderStyle::Sqlite, true);
            let stmt = tx.prepare(q.as_ref())?;
            (BackendTx::Libsql(tx), PreparedStmt::Libsql(stmt))
        }
        MiddlewarePoolConnection::Sqlite { conn, .. } => {
            let tx = begin_sqlite_tx(conn).await?;
            let q = translate_placeholders(base_query, PlaceholderStyle::Sqlite, true);
            let stmt = tx.prepare(q.as_ref())?;
            (BackendTx::Sqlite(tx), PreparedStmt::Sqlite(stmt))
        }
        _ => {
            return Err(SqlMiddlewareDbError::Unimplemented(
                "expected Turso, Postgres, SQLite, or LibSQL connection".to_string(),
            ));
        }
    };

    // Run some custom Rust code to fetch parameters.
    // Or do whatever custom code you wanted in here.
    let dynamic_params: Vec<RowValues> =
        todo!("fetch parameters for event_id {event_id}");

    let rows = match (tx, stmt) {
        (BackendTx::Turso(tx), PreparedStmt::Turso(mut stmt)) => {
            let result = tx.query_prepared(&mut stmt, &dynamic_params).await;
            match result {
                Ok(rows) => {
                    tx.commit().await?;
                    rows
                }
                Err(e) => {
                    let _ = tx.rollback().await;
                    return Err(e);
                }
            }
        }
        (BackendTx::Postgres(tx), PreparedStmt::Postgres(stmt)) => {
            let result = tx.query_prepared(&stmt, &dynamic_params).await;
            match result {
                Ok(rows) => {
                    tx.commit().await?;
                    rows
                }
                Err(e) => {
                    let _ = tx.rollback().await;
                    return Err(e);
                }
            }
        }
        (BackendTx::Libsql(tx), PreparedStmt::Libsql(stmt)) => {
            let result = tx.query_prepared(&stmt, &dynamic_params).await;
            match result {
                Ok(rows) => {
                    tx.commit().await?;
                    rows
                }
                Err(e) => {
                    let _ = tx.rollback().await;
                    return Err(e);
                }
            }
        }
        (BackendTx::Sqlite(tx), PreparedStmt::Sqlite(stmt)) => {
            let result = tx.query_prepared(&stmt, &dynamic_params).await;
            match result {
                Ok(rows) => {
                    tx.commit().await?;
                    rows
                }
                Err(e) => {
                    let _ = tx.rollback().await;
                    return Err(e);
                }
            }
        }
        _ => unreachable!("transaction and statement variants always align"),
    };

    let parsed_scores = rows
        .results
        .iter()
        .map(|row| {
            Ok(Scores {
                golfer_name: row
                    .get("golfername")
                    .and_then(|v| v.as_text())
                    .unwrap_or_default()
                    .to_string(),
                detailed_statistics: Statistic {
                    /* fetch additional fields */
                },
            })
        })
        .collect::<Result<Vec<Scores>, SqlMiddlewareDbError>>()?;

    Ok(ScoresAndLastRefresh {
        score_struct: parsed_scores,
    })
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
- [SQLite test example](/tests/test5c_sqlite.rs), [SQLite bench example 1](../benches/bench_rusqlite_single_row_lookup.rs), [SQLite bench example 2](../benches/bench_rusqlite_multithread_pool_checkout.rs)
- [Turso test example](/tests/test5d_turso.rs), [Turso bench example 1](../benches/bench_turso_single_row_lookup.rs)
- [PostgreSQL test example](/tests/test5a_postgres.rs)
- [LibSQL test example](/tests/test5b_libsql.rs)

## Placeholder Translation

- Default off. Enable at pool creation with the `*_with_translation(..., true)` constructors (or by toggling `translate_placeholders` on `ConfigAndPool`) to translate SQLite-style `?1` to Postgres `$1` (or the inverse) automatically for parameterised calls.
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
- `turso`: Enables Turso (in-process, SQLite-compatible). Experimental. No deadpool support (yet).
- `default`: Enables common backends (sqlite, postgres). Enable others as needed.
- `test-utils`: Enables test utilities for internal testing

## Developing and Testing

- Build with defaults (sqlite, postgres): `cargo build`
- Include Turso backend: `cargo build --features turso`
- Run tests (defaults): `cargo test` or `cargo nextest run`
    - Notice that `test4_trait` does have hard-coded testing postgres connection strings. I can't get codex to work with postgres embedded anymore, so when working on this test w codex I've hardcoded those values so I can work around it's lack of network connectivity. You'll have to change them if you want that test to compile in your environment. 
- Run with Turso: `cargo test --features turso`
- Run with LibSQL: `cargo test --features libsql`

### Our use of `[allow(...)]`s

- `#[allow(clippy::unused_async)]` keeps public constructors async so the signature stays consistent even when the current body has no awaits. You’ll see this on `ConfigAndPool::new_postgres` (`src/postgres/config.rs:10`), `ConfigAndPool::new_mssql` (`src/mssql/config.rs:19`), and `MiddlewarePool::get` (`src/pool/types.rs:66`). We also call out the rationale in **[Async Design Decisions](async.md)**.
- `#[allow(clippy::useless_conversion)]` is used once to satisfy `rusqlite::params_from_iter`, which requires an iterator type that Clippy would otherwise collapse away (`src/sqlite/params.rs:79`).
- `#[allow(unreachable_patterns)]` guards catch-all branches that only fire when a backend feature is disabled, preventing false positives when matching on `MiddlewarePoolConnection` (`src/pool/connection.rs:102`, `src/executor.rs:64`, `src/executor.rs:97`, `src/executor.rs:130`, `src/pool/interaction.rs:40`, `src/pool/interaction.rs:78`).
- `#[allow(unused_variables)]` appears around the interaction helpers because the higher-order functions take arguments that are only needed for certain backend combinations (`src/pool/interaction.rs:10`, `src/pool/interaction.rs:51`).

## Release Notes

- 0.3.0 (unreleased): Defaulted to the fluent query builder for prepared statements (older `execute_select`/`execute_dml` helpers on `MiddlewarePoolConnection` were removed), expanded placeholder translation docs and examples, and improved Postgres integer binding to downcast to `INT2/INT4` when inferred.
- 0.1.9 (unreleased): Switched the project license from BSD-2-Clause to MIT, added third-party notice documentation, and introduced optional placeholder translation (pool defaults + per-call `QueryOptions`).
