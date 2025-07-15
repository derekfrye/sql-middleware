# SQL Middleware - A unified interface for SQL databases

This crate provides a middleware layer for SQL database access,
currently supporting SQLite, PostgreSQL, and SQL Server backends. The main goal is to
provide a unified, async-compatible API that works across different database systems.

## Features

- Asynchronous database access with deadpool connection pooling
- Support for SQLite, PostgreSQL, and SQL Server backends
- Unified parameter conversion system
- Consistent result handling across database engines
- Transaction support

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
```