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
use sql_middleware::prelude::*;

async fn sqlite_example() -> Result<(), SqlMiddlewareDbError> {
    // Create a SQLite connection pool
    let config = ConfigAndPool::new_sqlite("my_database.db".to_string()).await?;
    
    // Get a connection from the pool
    let pool = config.pool.get().await?;
    let mut conn = MiddlewarePool::get_connection(&pool).await?;
    
    // Execute a query with parameters
    let result = conn.execute_select(
        "SELECT * FROM users WHERE id = ?",
        &[RowValues::Int(1)]
    ).await?;
    
    // Process the results
    for row in result.results {
        println!("User: {}", row.get("name").unwrap().as_text().unwrap());
    }
    
    Ok(())
}

async fn postgres_example() -> Result<(), SqlMiddlewareDbError> {
    // Create a PostgreSQL connection pool
    let mut pg_config = deadpool_postgres::Config::new();
    pg_config.host = Some("localhost".to_string());
    pg_config.port = Some(5432);
    pg_config.dbname = Some("mydatabase".to_string());
    pg_config.user = Some("user".to_string());
    pg_config.password = Some("password".to_string());
    
    let config = ConfigAndPool::new_postgres(pg_config).await?;
    
    // Get a connection and execute a query
    let pool = config.pool.get().await?;
    let mut conn = MiddlewarePool::get_connection(&pool).await?;
    
    let result = conn.execute_select(
        "SELECT * FROM users WHERE id = $1",
        &[RowValues::Int(1)]
    ).await?;
    
    Ok(())
}

async fn libsql_example() -> Result<(), SqlMiddlewareDbError> {
    use sql_middleware::prelude::*;

    // In-memory LibSQL (or use a file path like "./data.db")
    let config = ConfigAndPool::new_libsql(":memory:".to_string()).await?;

    // Remote LibSQL (Turso) example:
    // let config = ConfigAndPool::new_libsql_remote(
    //     "libsql://your-url".to_string(),
    //     "your_auth_token".to_string(),
    // ).await?;

    let pool = config.pool.get().await?;
    let mut conn = MiddlewarePool::get_connection(&pool).await?;

    let result = conn.execute_select(
        "SELECT 1 as one",
        &[]
    ).await?;

    assert_eq!(*result.results[0].get("one").unwrap().as_int().unwrap(), 1);
    Ok(())
}

async fn turso_example() -> Result<(), SqlMiddlewareDbError> {
    use sql_middleware::prelude::*;

    // In-memory Turso (or use a file path like "./data.db")
    let config = ConfigAndPool::new_turso(":memory:".to_string()).await?;

    let pool = config.pool.get().await?;
    let mut conn = MiddlewarePool::get_connection(&pool).await?;

    conn.execute_batch("CREATE TABLE t (id INTEGER, name TEXT);").await?;
    conn.execute_dml(
        "INSERT INTO t (id, name) VALUES (?, ?)",
        &[RowValues::Int(1), RowValues::Text("alice".into())],
    ).await?;

    let rs = conn
        .execute_select("SELECT name FROM t WHERE id = ?", &[RowValues::Int(1)])
        .await?;
    assert_eq!(rs.results[0].get("name").unwrap().as_text().unwrap(), "alice");
    Ok(())
}

async fn sqlserver_example() -> Result<(), SqlMiddlewareDbError> {
    // Create an SQL Server connection pool
    let config = ConfigAndPool::new_mssql(
        "localhost".to_string(),
        "mydatabase".to_string(),
        "sa".to_string(),
        "strong_password".to_string(),
        Some(1433),
        None,
    ).await?;
    
    // Get a connection and execute a query
    let pool = config.pool.get().await?;
    let mut conn = MiddlewarePool::get_connection(&pool).await?;
    
    let result = conn.execute_select(
        "SELECT * FROM users WHERE id = @p1",
        &[RowValues::Int(1)]
    ).await?;
    
    Ok(())
}

// For more in-depth examples (parameter conversion, batch queries, per-backend
// transactions, AsyncDatabaseExecutor usage), see the project README:
// https://github.com/derekfrye/sql-middleware/blob/main/docs/README.md
```
