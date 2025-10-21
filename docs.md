    # SQL Middleware - A lightweight wrapper SQL backends

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
    let pool = config_and_pool.pool.get().await?;
    let mut conn = MiddlewarePool::get_connection(pool).await?;

    // Pick the appropriate placeholder syntax based on the backend.
    let (insert_sql, fetch_sql) = match &conn {
        MiddlewarePoolConnection::Postgres(_) => (
            "INSERT INTO scores (espn_id, score, updated_at) VALUES ($1, $2, $3)",
            "SELECT espn_id, score, updated_at FROM scores ORDER BY updated_at DESC LIMIT $1",
        ),
        MiddlewarePoolConnection::Sqlite(_)
        | MiddlewarePoolConnection::Libsql(_)
        | MiddlewarePoolConnection::Turso(_) => (
            "INSERT INTO scores (espn_id, score, updated_at) VALUES (?1, ?2, ?3)",
            "SELECT espn_id, score, updated_at FROM scores ORDER BY updated_at DESC LIMIT ?1",
        ),
        #[allow(unreachable_patterns)]
        _ => {
            return Err(SqlMiddlewareDbError::Unimplemented(
                "Backend not enabled in this build".to_string(),
            ))
        }
    };

    // Reuse the same binding logic for every backend.
    for change in updates {
        let params = vec![
            RowValues::Int(change.espn_id),
            RowValues::Int(i64::from(change.score)),
            RowValues::Timestamp(change.updated_at),
        ];
        let statement = QueryAndParams::new(insert_sql, params);
        conn.execute_dml(&statement.query, &statement.params).await?;
    }

    // Fetch the latest rows to confirm the write path succeeded.
    let limit = updates.len().max(1) as i64;
    let latest = QueryAndParams::new(fetch_sql, vec![RowValues::Int(limit)]);
    conn.execute_select(&latest.query, &latest.params).await
}

// For more in-depth examples (batch queries, AsyncDatabaseExecutor usage, benchmarks),
// see the project README: https://github.com/derekfrye/sql-middleware/blob/main/docs/README.md
```
