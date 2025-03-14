# sql-middleware

Sql-middleware is a lightweight async wrapper for [tokio-postgres](https://crates.io/crates/tokio-postgres) and [rusqlite](https://crates.io/crates/rusqlite), with [deadpool](https://github.com/deadpool-rs/deadpool) connection pooling, and an async api (via [deadpool-sqlite](https://github.com/deadpool-rs/deadpool) and tokio-postgres). A slim alternative to [SQLx](https://crates.io/crates/sqlx); fewer features, but striving toward a consistent api regardless of database backend.

Motivated from trying SQLx, not liking some issue [others already noted](https://www.reddit.com/r/rust/comments/16cfcgt/seeking_advice_considering_abandoning_sqlx_after/?rdt=44192), and wanting an alternative. 

## Goals
* Provide convenience functions for common sql query patterns while offering the underlying flexibility of deadpool-sqlite and deadpool-postgres.
* Minimal overhead (just syntax convenience).

## Examples

More examples available in the [tests dir](../tests/).

### Importing

Use the prelude to import everything you need with a single line:

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
SQLite
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




// same api for connection
// sqlite just has fewer required 
// config items (no port, etc.)
let c = ConfigAndPool::
    new_sqlite(cfg).await?;
let conn = MiddlewarePool
    ::get_connection(&c.pool)
    .await?;


```

</td>
</tr>
</table>

### Batch query w/o params

Same api regardless of db backend.

<table>
<tr>
<th>
PostgreSQL
</th>
<th>
SQLite
</th>
</tr>
<tr>
<td>

```rust
// simple api for batch queries
let ddl_query =
    include_str!("test1.sql");
conn.execute_batch(&ddl_query).await?;
```

</td>
<td>

```rust
// same api
let ddl_query = 
    include_str!("test1.sql");
conn.execute_batch(&ddl_query).await?;
```

</td>
</td>
</tr>
</table>

### Parameterized Queries

Consistent api for running parametrized queries using the `QueryAndParams` struct. Database-specific parameter syntax is handled by the middleware.

<table>
<tr>
<th>
PostgreSQL
</th>
<th>
SQLite
</th>
</tr>
<tr>
<td>

```rust
// Create query with parameters
let q = QueryAndParams::new(
    "INSERT INTO test (espn_id, name, ins_ts) 
     VALUES ($1, $2, $3)",
    vec![
        RowValues::Int(123456),
        RowValues::Text("test name"
            .to_string()),
        RowValues::Timestamp(
            NaiveDateTime::parse_from_str(
                "2021-08-06 16:00:00"
                , "%Y-%m-%d %H:%M:%S")?,
        ),
    ]);

// Convert params using the ParamConverter trait
let converted_params = 
    convert_sql_params::<PostgresParams>(
        &q.params,
        ConversionMode::Execute
    )?;

// Execute the query
conn.execute_dml(
    &q.query, 
    &converted_params)
.await?;
```

</td>
<td>

```rust
// Create query with parameters
let q = QueryAndParams::new(
    "INSERT INTO test (espn_id, name, ins_ts) 
     VALUES (?1, ?2, ?3)",
    vec![
        RowValues::Int(123456),
        RowValues::Text("test name"
            .to_string()),
        RowValues::Timestamp(
            NaiveDateTime::parse_from_str(
                "2021-08-06 16:00:00"
                , "%Y-%m-%d %H:%M:%S")?,
        ),
    ]);

// Similar API for parameter conversion
let converted_params = 
    convert_sql_params::<SqliteParamsExecute>(
        &q.params,
        ConversionMode::Execute
    )?;

// Execute the query
conn.execute_dml(
    &q.query, 
    &converted_params)
.await?;
```

</td>
</td>
</tr>
</table>

### Queries without parameters

You can create queries without parameters using `new_without_params`:

```rust
let query = QueryAndParams::new_without_params(
    "SELECT * FROM users"
);
let results = conn.execute_select(&query.query, &[]).await?;
```

### Transactions with custom logic

Here, the APIs do differ as they use the underlying database's transaction capabilities. Each database backend has its own transaction handling approach.

<table>
<tr>
<th>
PostgreSQL
</th>
<th>
SQLite
</th>
</tr>
<tr>
<td>

```rust
// Get PostgreSQL-specific connection
let pg_conn = match &conn {
    MiddlewarePoolConnection::Postgres(pg) => pg,
    _ => panic!("Expected Postgres connection"),
};

// Get client
let pg_client = &pg_conn.client;

// Start transaction
let tx = pg_client.transaction().await?;

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

// Commit transaction
tx.commit().await?;
```

</td>
<td>

```rust
// Get SQLite-specific connection
let sqlite_conn = match &conn {
    MiddlewarePoolConnection::Sqlite(sqlite) => sqlite,
    _ => panic!("Expected SQLite connection"),
};

// Use interact for async transaction handling
let rows = sqlite_conn
    .interact(move |conn| {
        // Start transaction
        let tx = conn.transaction()?;
        
        // Convert parameters
        let converted_params = 
            convert_sql_params::<SqliteParamsExecute>(
                &q.params,
                ConversionMode::Execute
            )?;
            
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
        
        // Commit transaction
        tx.commit()?;
        
        Ok::<_, SqlMiddlewareDbError>(rows)
    })
    .await?;
```

</td>
</tr>
</table>

### Using the AsyncDatabaseExecutor trait

The `AsyncDatabaseExecutor` trait provides a consistent interface for database operations:

```rust
// This works for both PostgreSQL and SQLite connections
async fn insert_user<T: AsyncDatabaseExecutor>(
    conn: &T,
    user_id: i32,
    name: &str
) -> Result<(), SqlMiddlewareDbError> {
    let query = QueryAndParams::new(
        // Use appropriate parameter syntax for your DB
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