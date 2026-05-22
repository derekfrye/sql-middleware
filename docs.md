# SQL Middleware

Lightweight async SQL wrappers for SQLite, PostgreSQL, Turso, and SQL Server.

The crate provides a small, shared execution surface over the
supported backends: pooled connections, parameter binding, result sets, prepared execution, and a typed
connection API for code that wants compile-time transaction-state tracking. This
crate is not an ORM.

Most applications should start with [`prelude`].

## Backends and Features

Default features are `sqlite` and `postgres`.

```toml
# Default: SQLite + PostgreSQL
sql-middleware = "0"

# SQLite only
sql-middleware = { version = "0", default-features = false, features = ["sqlite"] }

# All backends
sql-middleware = { version = "0", features = ["sqlite", "postgres", "turso", "mssql"] }
```

## Main API Shape

```rust,no_run
use sql_middleware::prelude::*;
pub async fn insert_and_read(cap: &ConfigAndPool) -> Result<ResultSet, SqlMiddlewareDbError> {
    // Acquire a pooled connection (Postgres, SQLite, Turso, or MSSQL have same code here).
    let mut conn = cap.get_connection().await?;

    conn.execute_batch("CREATE TABLE IF NOT EXISTS users (id INTEGER PRIMARY KEY, name TEXT)")
        .await?;

    let params = [RowValues::Int(1), RowValues::Text("alice".to_string())];
    conn.query("INSERT INTO users (id, name) VALUES ($1, $2)")
        // translate between backends for $1 vs. ?1, depending on which backend is used
        .translation(TranslationMode::ForceOn)
        .params(&params)
        .dml()
        .await?;

    let params = [RowValues::Int(1)];
    conn.query("SELECT name FROM users WHERE id = $1")
        .translation(TranslationMode::ForceOn)
        .params(&params)
        .select()
        .await
}
```

Placeholder translation (like in above example) is optional. When enabled, the scanner rewrites
placeholder styles for the target backend while skipping quoted strings and SQL
comments. Use backend-native SQL for complex SQL bodies where lightweight
translation is not enough.

## More Examples

See the repository README for larger examples, free-from transaction styles, benchmark
notes, and backend-specific setup:
<https://github.com/derekfrye/sql-middleware/blob/main/docs/README.md>. Some smaller notes 
on the API follow.

## Results and Values

Rows are returned as [`ResultSet`] values containing [`CustomDbRow`] entries.
Column values use [`RowValues`]:

- `Null`
- `Int(i64)`
- `Float(f64)`
- `Text(String)`
- `Bool(bool)`
- `Timestamp(chrono::NaiveDateTime)`
- `JSON(serde_json::Value)`
- `Blob(Vec<u8>)`

Convenience accessors such as `as_int`, `as_text`, `as_bool`, `as_timestamp`,
and `get("column_name")` are available for result inspection.

## Builder Pattern

If you prefer backend builder pattern:

- [`ConfigAndPool::sqlite_builder`] and [`ConfigAndPool::sqlite_path_builder`]
- [`ConfigAndPool::postgres_builder`]
- `ConfigAndPool::turso_builder` and `ConfigAndPool::turso_path_builder`
- `ConfigAndPool::mssql_builder`

The `new_*` constructors take backend option structs, such as
[`SqliteOptions`], [`PostgresOptions`], `TursoOptions`, and `MssqlOptions`.
They do not take raw strings directly.

```rust,no_run
use sql_middleware::prelude::*;

pub async fn sqlite_without_checkout_validation() -> Result<ConfigAndPool, SqlMiddlewareDbError> {
    ConfigAndPool::sqlite_builder("app.db".to_string())
        .test_on_check_out(false)
        .build()
        .await
}
```

SQLite and Turso default to cached statements. For dynamic SQL workloads, set
[`StatementCacheMode::Uncached`] at the pool level or override a single query:

```rust,no_run
use sql_middleware::prelude::*;

pub async fn sqlite_uncached_query() -> Result<ResultSet, SqlMiddlewareDbError> {
    let cap = ConfigAndPool::sqlite_builder("app.db".to_string())
        .statement_cache(StatementCacheMode::Uncached)
        .build()
        .await?;
    let mut conn = cap.get_connection().await?;

    conn.query("SELECT id FROM users WHERE name = ?1")
        .params(&[RowValues::Text("alice".to_string())])
        .statement_cache(StatementCacheMode::Cached)
        .select()
        .await
}
```

For lower-level construction, you can use the option structs directly:

```rust,no_run
use sql_middleware::prelude::*;

pub async fn sqlite_from_options() -> Result<ConfigAndPool, SqlMiddlewareDbError> {
    let opts = SqliteOptions::new("app.db".to_string())
        .with_translation(true)
        .with_pool_options(MiddlewarePoolOptions::new().with_test_on_check_out(false));

    ConfigAndPool::new_sqlite(opts).await
}
```

## Typed API example

The typed API keeps idle and in-transaction connections as distinct Rust types, and the
`AnyIdle` / `AnyTx` wrappers let generic code work across backends.

```rust,no_run
use sql_middleware::prelude::*;
use sql_middleware::typed::{AnyIdle, BeginTx, Queryable, TxConn, TypedConnOps};

pub async fn run_typed_flow(mut conn: AnyIdle) -> Result<AnyIdle, SqlMiddlewareDbError> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS typed_users (id BIGINT PRIMARY KEY, name TEXT)",
    )
    .await?;

    let mut tx = conn.begin().await?;
    let params = [RowValues::Int(1), RowValues::Text("alice".to_string())];
    tx.query("INSERT INTO typed_users (id, name) VALUES ($1, $2)")
        .translation(TranslationMode::ForceOn)
        .params(&params)
        .dml()
        .await?;

    tx.commit().await
}
```

### Typed API transactions
The typed API provides a higher-level transaction model:

- `BeginTx::begin()` moves an idle connection into an in-transaction state.
- `TxConn::commit()` returns the corresponding idle connection.
- `TxConn::rollback()` returns the corresponding idle connection.
- Dropping a typed transaction without commit or rollback triggers a best-effort
  rollback.

The legacy transaction wrappers return [`TxOutcome`] on commit or rollback.
This carries a restored type-erased connection when the backend needs one.

## SQLite Worker Helpers

SQLite uses a worker thread so blocking `rusqlite` calls do not stall the async
runtime. [`MiddlewarePoolConnection`] exposes SQLite-specific helpers when the
active connection is SQLite.

```rust,no_run
use sql_middleware::prelude::*;

async fn demo() -> Result<(), SqlMiddlewareDbError> {
    let cap = ConfigAndPool::sqlite_builder("file::memory:?cache=shared".to_string())
        .build()
        .await?;
    let mut conn = cap.get_connection().await?;

    conn.with_blocking_sqlite(|raw| {
        let tx = raw.transaction()?;
        tx.execute_batch("CREATE TABLE t (id INTEGER PRIMARY KEY, name TEXT);")?;
        tx.execute("INSERT INTO t (name) VALUES (?1)", ["alice"])?;
        tx.commit()?;
        Ok::<_, SqlMiddlewareDbError>(())
    })
    .await?;

    let mut prepared = conn
        .prepare_sqlite_statement("SELECT name FROM t WHERE id = ?1")
        .await?;
    let rows = prepared.query(&[RowValues::Int(1)]).await?;

    assert_eq!(rows.results[0].get("name").unwrap().as_text().unwrap(), "alice");
    Ok(())
}
```
## Public Re-exports

The crate root and [`prelude`] re-export the common API:

- Pools and connections: [`ConfigAndPool`], [`MiddlewarePool`],
  [`MiddlewarePoolConnection`], [`AnyConnWrapper`], [`MiddlewarePoolOptions`]
- Querying: [`QueryBuilder`], [`QueryAndParams`], [`QueryTarget`],
  [`BatchTarget`], [`execute_batch`]
- Values and results: [`RowValues`], [`ResultSet`], [`CustomDbRow`]
- Translation: [`TranslationMode`], [`PrepareMode`], [`QueryOptions`],
  [`StatementCacheMode`], [`PlaceholderStyle`], [`translate_placeholders`]
- Errors and metadata: [`SqlMiddlewareDbError`], [`DatabaseType`],
  [`ConversionMode`], [`TxOutcome`]
- Typed API: [`typed`], [`typed_api`], and backend modules `typed_postgres`,
  `typed_sqlite`, `typed_turso`, and `typed_mssql`

Backend modules also expose lower-level helpers for callers that already manage
native backend clients or need backend-specific prepared/transaction handles.
