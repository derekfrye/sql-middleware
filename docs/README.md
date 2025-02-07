# sql-middleware

Sql-middleware is a lightweight wrapper for [tokio-postgres](https://crates.io/crates/tokio-postgres) and [rusqlite](https://crates.io/crates/rusqlite), with [deadpool](https://github.com/deadpool-rs/deadpool) connection pooling, and an async api (via [deadpool-sqlite](https://github.com/deadpool-rs/deadpool) and tokio-postgres). Think of it as a slim alternative to [SQLx](https://crates.io/crates/sqlx); fewer features, but focused on a consistent api for postgres and sqlite.

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
<pre lang="rust">
let mut cfg = deadpool_postgres::Config::new();
cfg.dbname = Some("test_db".to_string());
cfg.host = Some("192.168.2.1".to_string());
cfg.port = Some(5432);
cfg.user = Some("test user".to_string());
cfg.password = Some("passwd".to_string());

let config_and_pool = ConfigAndPool::new_postgres(cfg).await?;
let pool = config_and_pool.pool.get().await?;
let conn = MiddlewarePool::get_connection(pool).await?;

// simple api for running non-paramaterized queries
let ddl_query = include_str!("test1.sql");
conn.execute_batch(&ddl_query).await?;

// consistent way for defining queries and parameters regardless of db backend
let query_and_params = QueryAndParams {
    query: "INSERT INTO test (espn_id, name, ins_ts) VALUES ($1, $2, $3)".to_string(),
    params: vec![
        RowValues::Int(123456),
        RowValues::Text("test name".to_string()),
        RowValues::Timestamp(
            NaiveDateTime::parse_from_str("2021-08-06 16:00:00", "%Y-%m-%d %H:%M:%S")?,
        ),
    ],
};

let converted_params = convert_sql_params::<PostgresParams>(
    &query_and_params.params,
    ConversionMode::Execute
)?;

// full control over your transactions, can mix in rust logic
let tx = pgconn.transaction().await?;
// could do some other business logic first, like loop through stuff and 
tx.prepare(query_and_params.query.as_str()).await?;
tx.execute(query_and_params.query.as_str(), &converted_params.as_refs()).await?;
tx.commit().await?;
</pre>
</td>
<td>
<pre lang="rust">
let cfg = "file::memory:?cache=shared".to_string();






let config_and_pool = ConfigAndPool::new_sqlite(cfg).await?;
let pool = config_and_pool.pool.get().await?;
let conn = MiddlewarePool::get_connection(pool).await?;

// simple api for running non-paramaterized queries
let ddl_query = include_str!("test1.sql");
conn.execute_batch(&ddl_query).await?;

// consistent way for defining queries and parameters regardless of db backend
let query_and_params = QueryAndParams {
    query: "INSERT INTO test (espn_id, name, ins_ts) VALUES (?1, ?2, ?3)".to_string(),
    params: vec![
        RowValues::Int(123456),
        RowValues::Text("test name".to_string()),
        RowValues::Timestamp(
            NaiveDateTime::parse_from_str("2021-08-06 16:00:00", "%Y-%m-%d %H:%M:%S")?,
        ),
    ],
};

let converted_params = convert_sql_params::<SqliteParamsExecute>(
    &query_and_params.params,
    ConversionMode::Execute
)?;

let sconn = match &conn {
    MiddlewarePoolConnection::Sqlite(sconn) => sconn,
    _ => panic!("Only sqlite is supported "),
};

// full control over your transactions, can mix in rust logic
{
    sconn
        .interact(move |xxx| {
            let tx = xxx.transaction()?;
            for query in query_and_params_vec.iter() {
                let mut stmt = tx.prepare(&query.query)?;
                let converted_params = convert_sql_params::<SqliteParamsExecute>(
                    &query.params,
                    ConversionMode::Execute,
                )?;
                stmt.execute(converted_params.0)?;
            }

            tx.commit()?;
            Ok::<_, SqlMiddlewareDbError>(())
        })
        .await?
}?;
</pre>
</td>
</tr>
</table>