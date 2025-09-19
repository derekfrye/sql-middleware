# sql-middleware

![Unsafe Forbidden](https://img.shields.io/badge/unsafe-forbidden-success.svg)

Sql-middleware is a lightweight async wrapper for [tokio-postgres](https://crates.io/crates/tokio-postgres), [rusqlite](https://crates.io/crates/rusqlite), [libsql](https://crates.io/crates/libsql), experimental [turso](https://crates.io/crates/turso), and [tiberius](https://crates.io/crates/tiberius) (SQL Server), with [deadpool](https://github.com/deadpool-rs/deadpool) connection pooling (except Turso, which doesn't have deadpool backend yet), and an async api. A slim alternative to [SQLx](https://crates.io/crates/sqlx); fewer features, but striving toward a consistent api regardless of database backend.

Motivated from trying SQLx, not liking some issue [others already noted](https://www.reddit.com/r/rust/comments/16cfcgt/seeking_advice_considering_abandoning_sqlx_after/?rdt=44192), and wanting an alternative. 

## Goals
* Convenience functions for common SQL query patterns
* Keep underlying flexibility of `deadpool`
* Minimal overhead (ideally, just syntaxs sugar/wrapper fns)

## Feature Flags

By default, `postgres` and `sqlite` database backends are enabled. You can selectively enable only the backends you need:

```toml
# Only include SQLite and LibSQL support
sql-middleware = { version = "0", features = ["sqlite", "libsql"] }
```

Available features:
- `sqlite`: Enables SQLite support
- `postgres`: Enables PostgreSQL support
- `mssql`: Enables SQL Server support
- `libsql`: Enables LibSQL support (local or remote)
- `turso`: Enables Turso (in-process, SQLite-compatible). Experimental. No deadpool support (yet).
- `default`: Enables common backends (sqlite, postgres). Enable others as needed.
- `test-utils`: Enables test utilities for internal testing

## Examples

More examples available in the [tests dir](../tests/), and this is in-use with a tiny little website app, [rusty-golf](https://github.com/derekfrye/rusty-golf).

### Importing

You can use the prelude to import everything you need, or import item by item.

```rust
use sql_middleware::prelude::*;
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
let conn = MiddlewarePool
    ::get_connection(&c.pool)
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
let conn = MiddlewarePool
    ::get_connection(&c.pool)
    .await?;

```

</td>
</tr>
</table>

Note: The SQLite example applies to SQLite, LibSQL, and Turso. Swap the constructor as needed: `new_sqlite(path)`, `new_libsql(path)`, or `new_turso(path)`. For Turso, there’s no deadpool pooling; `get_connection` creates a fresh connection.

### Batch query w/o params

Same api regardless of db backend.

```rust
// simple api for batch queries
let ddl_query =
    include_str!("/path/to/test1.sql");
conn.execute_batch(&ddl_query).await?;
```


### Parameterized Queries

Consistent API using `QueryAndParams`. Only the placeholder syntax differs.

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
// PostgreSQL uses $-style placeholders
let q = QueryAndParams::new(
    "INSERT INTO test (espn_id, name, ins_ts) VALUES ($1, $2, $3)",
    vec![
        RowValues::Int(123456),
        RowValues::Text("test name".to_string()),
        RowValues::Timestamp(NaiveDateTime::parse_from_str(
            "2021-08-06 16:00:00",
            "%Y-%m-%d %H:%M:%S",
        )?),
    ],
);

// Execute directly with RowValues; middleware converts internally
conn.execute_dml(&q.query, &q.params).await?;
```

</td>
<td>

```rust
// SQLite-compatible backends use ? or ?N placeholders
let q = QueryAndParams::new(
    "INSERT INTO test (espn_id, name, ins_ts) VALUES (?1, ?2, ?3)",
    vec![
        RowValues::Int(123456),
        RowValues::Text("test name".to_string()),
        RowValues::Timestamp(NaiveDateTime::parse_from_str(
            "2021-08-06 16:00:00",
            "%Y-%m-%d %H:%M:%S",
        )?),
    ],
);

// Works the same for SQLite, LibSQL, and Turso
conn.execute_dml(&q.query, &q.params).await?;
```

</td>
</tr>
</table>

Note: For LibSQL, construct with `ConfigAndPool::new_libsql(path)`. For Turso, use `ConfigAndPool::new_turso(path)`; there is no deadpool pooling for Turso — `get_connection` creates a new connection.

### Queries without parameters

You can issue no-parameter queries directly, the same for PostgreSQL, SQLite, LibSQL, and Turso:

```rust
// Either build a QueryAndParams
let query = QueryAndParams::new_without_params("SELECT * FROM users");
let results = conn.execute_select(&query.query, &[]).await?;

// Or pass the SQL string directly
let results2 = conn.execute_select("SELECT * FROM users", &[]).await?;
```

### Transactions with custom logic

Here, the APIs differ, because the underlying database's transaction approach differs. It doesn't appear easy to make these consistent. But this is the way to do queries if you need custom app logic between `connection.transaction()` and `connection.commit()`.

<table>
<tr>
<th>
PostgreSQL
</th>
<th>
SQLite
</th>
<th>
LibSQL
</th>
</tr>
<tr>
<td>

```rust
// Get PostgreSQL-specific connection
let pg_conn = match &conn {
    MiddlewarePoolConnection::Postgres(pg) 
        => pg,
    _ => panic!("Expected Postgres connection"),
};

// Get client
let pg_client = &pg_conn.client;

let tx = pg_client.transaction().await?;

// could run custom logic anywhere between stmts

// Prepare statement
let stmt = tx.prepare(&q.query).await?;

// Convert parameters
let converted_params = 
    convert_sql_params::<PostgresParams>(
        &q.params,
        ConversionMode::Execute
    )?;

// Execute query
let rows = tx.execute(
    &stmt, 
    converted_params.as_refs()
).await?;

tx.commit().await?;







```

</td>
<td>

```rust
// Get SQLite-specific connection
let sqlite_conn = match &conn {
    MiddlewarePoolConnection::Sqlite(sqlite) 
        => sqlite,
    _ => panic!("Expected SQLite connection"),
};

// Use interact for async tx
let rows = sqlite_conn
    .interact(move |conn| {
        let tx = conn.transaction()?;
        
        // Convert parameters
        let converted_params = 
            convert_sql_params::<SqliteParamsExecute>(
                &q.params,
                ConversionMode::Execute
            )?;
            
// could run custom logic anywhere between stmts

        // Create parameter references
        let param_refs: Vec<&dyn ToSql> =
            converted_params.0.iter()
                .map(|v| v as &dyn ToSql)
                .collect();
        
        // Prepare and execute
        let rows = {
            let mut stmt = tx.prepare(&q.query)?;
            stmt.execute(&param_refs[..])?
        };
        
        tx.commit()?;
        
        Ok::<_, SqlMiddlewareDbError>(rows)
    })
    .await?;
```

</td>
<td>

```rust
// Get LibSQL-specific connection
let libsql_conn = match &conn {
    MiddlewarePoolConnection::Libsql(libsql) 
        => libsql,
    _ => panic!("Expected LibSQL connection"),
};

// Get client
let libsql_client = &libsql_conn.client;

let tx = libsql_client.transaction().await?;

// could run custom logic anywhere between stmts

// Convert parameters
let converted_params = 
    convert_sql_params::<LibsqlParams>(
        &q.params,
        ConversionMode::Execute
    )?;

// Execute query directly
// For repeated queries, you could use:
// let stmt = tx.prepare(&q.query).await?;
// let rows = stmt.execute(converted_params.0).await?;
let rows = tx.execute(
    &q.query,
    converted_params.0
).await?;

tx.commit().await?;







```

</td>
</tr>
</table>

### Using the AsyncDatabaseExecutor trait

The `AsyncDatabaseExecutor` trait provides a consistent interface for database operations:

```rust
// This works for PostgreSQL, SQLite, LibSQL, and Turso connections
async fn insert_user<T: AsyncDatabaseExecutor>(
    conn: &T,
    user_id: i32,
    name: &str
) -> Result<(), SqlMiddlewareDbError> {
    let query = QueryAndParams::new(
        // Use appropriate parameter syntax for your DB
        // PostgreSQL: "VALUES ($1, $2)"
        // SQLite/LibSQL/Turso: "VALUES (?, ?)" or "VALUES (?1, ?2)"
        "INSERT INTO users (id, name) VALUES ($1, $2)",
        vec![
            RowValues::Int(user_id),
            RowValues::Text(name.to_string()),
        ]
    );
    
    // Execute query through the trait
    conn.execute_dml(
        &query.query,
        &conn.convert_params(&query.params, ConversionMode::Execute)?
    ).await?;
    
    Ok(())
}
```

## Design Documents

- **[Async Design Decisions](async.md)** - Explains why some functions are marked with `#[allow(clippy::unused_async)]` and our async API design philosophy.

## Developing and Testing

- Build with defaults (sqlite, postgres): `cargo build`
- Include Turso backend: `cargo build --features turso`
- Run tests (defaults): `cargo test`
- Run with Turso: `cargo test --features turso`
- Run with LibSQL: `cargo test --features libsql`
