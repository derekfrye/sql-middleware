# sql-middleware

Sql-middleware is a lightweight wrapper for [tokio-postgres](https://crates.io/crates/tokio-postgres) and [rusqlite](https://crates.io/crates/rusqlite), with [deadpool](https://github.com/deadpool-rs/deadpool) connection pooling, and an async api (via [deadpool-sqlite](https://github.com/deadpool-rs/deadpool) and tokio-postgres). A slim alternative to [SQLx](https://crates.io/crates/sqlx); fewer features, but striving toward a consistent api regardless of database backend.

Motivated from trying SQLx, not liking some issue [others already noted](https://www.reddit.com/r/rust/comments/16cfcgt/seeking_advice_considering_abandoning_sqlx_after/?rdt=44192), and wanting an alternative. 

## Goals
* Provide convenience functions for common sql query patterns while offering the underlying flexibility of deadpool-sqlite and deadpool-postgres.
* Minimal overhead (just synatax convenience).

## Example

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
let pool = c.pool.get().await?;
let conn = MiddlewarePool
    ::get_connection(pool)
    .await?;

// simple api for batch queries
let ddl_query = 
    include_str!("test1.sql");
conn.
    execute_batch(&ddl_query).await?;

// consistent struct for 
// queries and params
// regardless of db backend
let q = QueryAndParams {
    query: "INSERT INTO test (espn_id, name
        , ins_ts) VALUES ($1, $2
        , $3)".to_string(),
    params: vec![
        RowValues::Int(123456),
        RowValues::Text("test name"
            .to_string()),
        RowValues::Timestamp(
            NaiveDateTime::parse_from_str(
                "2021-08-06 16:00:00"
                , "%Y-%m-%d %H:%M:%S")?,
        ),
    ],
};

// consistent way to
// convert query & params
// to what db backend expects
let converted_params = 
    convert_sql_params::<PostgresParams>(
    &q.params,
    ConversionMode::Execute
)?;





// full control over transactions
let tx = conn.transaction().await?;
// could do other rust code here first...
tx.prepare(q.query
    .as_str())
    .await?;
tx.execute(q.query
    .as_str()
    , &converted_params.as_refs()
    ).await?;
tx.commit().await?






















```

</td>
<td>

```rust
let cfg = "file::memory:?cache=shared"
    .to_string();



// same api for connection
// sqlite just has fewer required 
// things (no port, etc.)
let c = ConfigAndPool::
    new_sqlite(cfg).await?;
let pool = c.pool.get().await?;
let conn = MiddlewarePool
    ::get_connection(pool)
    .await?;







// same api for non-paramaterized queries
let ddl_query = include_str!("test1.sql");
conn.execute_batch(&ddl_query).await?;



// same struct for query and params
// notice ?1 instead of $1 w postgres
let q = QueryAndParams {
    query: "INSERT INTO test (espn_id, name
        , ins_ts) VALUES (?1, ?2
        , ?3)".to_string(),
    params: vec![
        RowValues::Int(123456),
        RowValues::Text("test name"
            .to_string()),
        RowValues::Timestamp(
            NaiveDateTime::parse_from_str(
                "2021-08-06 16:00:00"
                , "%Y-%m-%d %H:%M:%S")?,
        ),
    ],
};




// similar api for query parameters
let converted_params = convert_sql_params
    ::<SqliteParamsExecute>(
        &query_and_params.params,
        ConversionMode::Execute
    )?;

// a little more gymnastics needed
// to get an object from deadpool for
// custom code within transactions
let sconn = match &conn {
    MiddlewarePoolConnection::Sqlite(sconn) 
        => sconn,
    _
        => panic!("Sqlite only demo."),
};

// full control over your transactions
// here the api is sqlite specific
sconn
    .interact(move |xxx| {
        let tx = xxx.transaction()?;
        for query in <custom obj>.iter() {
            let mut stmt = tx.
                prepare(&query.query)?;
            let converted_params = 
                convert_sql_params
                    ::<SqliteParamsExecute>(
                        &query.params,
                        ConversionMode
                            ::Execute,
                    )?;

            stmt.execute(
                    converted_params
                    .0
                )?;
        }

        tx.commit()?;
        Ok::<_, SqlMiddlewareDbError>(())
    })
    .await?
```

</td>
</tr>
</table>