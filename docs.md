    # SQL Middleware - A lightweight wrapper SQL backends

*Keywords: postgres, sqlite, libsql, turso, deadpool, async, pool, query builder*

This crate provides a lightweight async wrapper for `SQLite`, `PostgreSQL`, `LibSQL`, `Turso` (experimental), and SQL Server (`tiberius`). The goal is a
similar, async-compatible API consistent across databases.

## Features

- Similar API regardless of backend (as much as possible)
- Asynchronous (where available)
- `deadpool` connection pooling (where available)
- Transaction support
- Not an ORM

## Feature Flags

Default features are `sqlite` and `postgres`. Enable others as needed:

```toml
# Only SQLite and LibSQL
sql-middleware = { version = "0", features = ["sqlite", "libsql"] }

# All backends
sql-middleware = { version = "0", features = ["sqlite", "postgres", "mssql", "libsql", "turso"] }
```

Additional flags:
- `libsql`: `LibSQL` (local has been tested, remote is present)
- `turso`: Turso (in-process, SQLite-compatible). Experimental; no remote support.
- `test-utils`: Test helpers for internal testing
- `mssql`: SQL Server via `tiberius` (untested, but present)
- `benchmarks`: Criterion helpers for benches

## Example

```rust,no_run
use chrono::NaiveDateTime;
use sql_middleware::prelude::*;

pub struct ScoreChange {
    pub espn_id: i64,
    pub score: i32,
    pub updated_at: NaiveDateTime,
}

/// Update scores across any supported backend without copy/pasting per-database functions.
pub async fn set_scores_in_db(
    config_and_pool: &ConfigAndPool,
    updates: &[ScoreChange],
) -> Result<ResultSet, SqlMiddlewareDbError> {
    // Acquire a pooled connection (Postgres, SQLite, LibSQL, or Turso).
    let mut conn = config_and_pool.get_connection().await?;

    // Author once; translation rewrites placeholders per backend.
    let insert_sql = "INSERT INTO scores (espn_id, score, updated_at) VALUES ($1, $2, $3)";
    let fetch_sql = "SELECT espn_id, score, updated_at FROM scores ORDER BY updated_at DESC LIMIT $1";

    // Reuse the same binding logic for every backend.
    for change in updates {
        let params = vec![
            RowValues::Int(change.espn_id),
            RowValues::Int(i64::from(change.score)),
            RowValues::Timestamp(change.updated_at),
        ];
        let statement = QueryAndParams::new(insert_sql, params);
        conn.query(&statement.query)
            .translation(TranslationMode::ForceOn)
            .params(&statement.params)
            .dml()
            .await?;
    }

    // Fetch the latest rows to confirm the write path succeeded.
    let limit = updates.len().max(1) as i64;
    let latest = QueryAndParams::new(fetch_sql, vec![RowValues::Int(limit)]);
    conn.query(&latest.query)
        .translation(TranslationMode::ForceOn)
        .params(&latest.params)
        .select()
        .await
}

### SQLite worker helpers

SQLite pooling runs through a worker thread so blocking `rusqlite` calls never stall the async runtime. Two helpers expose that surface:

```rust,no_run
use sql_middleware::prelude::*;

let cap = ConfigAndPool::new_sqlite("file::memory:?cache=shared".into()).await?;
let mut conn = cap.get_connection().await?;

// Borrow the raw rusqlite::Connection on the worker for batched work.
conn.with_blocking_sqlite(|raw| {
    let tx = raw.transaction()?;
    tx.execute_batch("CREATE TABLE t (id INTEGER PRIMARY KEY, name TEXT);")?;
    tx.execute("INSERT INTO t (name) VALUES (?1)", ["alice"])?;
    tx.commit()?;
    Ok::<_, SqlMiddlewareDbError>(())
})
.await?;

// Prepare once and reuse via the worker queue.
let prepared = conn
    .prepare_sqlite_statement("SELECT name FROM t WHERE id = ?1")
    .await?;
let rows = prepared.query(&[RowValues::Int(1)]).await?;
assert_eq!(rows.results[0].get("name").unwrap().as_text().unwrap(), "alice");
```

// For more in-depth examples (batch queries, query builder usage, benchmarks),
// see the project README: https://github.com/derekfrye/sql-middleware/blob/main/docs/README.md
```
