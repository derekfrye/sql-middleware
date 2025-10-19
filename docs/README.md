# sql-middleware

![Unsafe Forbidden](https://img.shields.io/badge/unsafe-forbidden-success.svg)

Sql-middleware is a lightweight async wrapper for [tokio-postgres](https://crates.io/crates/tokio-postgres), [rusqlite](https://crates.io/crates/rusqlite), [libsql](https://crates.io/crates/libsql), experimental [turso](https://crates.io/crates/turso), and [tiberius](https://crates.io/crates/tiberius) (SQL Server), with [deadpool](https://github.com/deadpool-rs/deadpool) connection pooling (except Turso, which doesn't have deadpool backend yet), and an async api. A slim alternative to [SQLx](https://crates.io/crates/sqlx); fewer features, but striving toward a consistent api.

Motivated from trying SQLx and not liking some issue [others already noted](https://www.reddit.com/r/rust/comments/16cfcgt/seeking_advice_considering_abandoning_sqlx_after/?rdt=44192). 

This middleware performance is about 14% faster than SQlx for at least some SQLite workloads. That could just be my misunderstanding of SQLx rather than an inherent performance difference. For current evidence, see our [benchmark results](./bench_results/index.md).

## Goals
* Convenience functions for common async SQL query patterns
* Keep underlying flexibility of `deadpool` connection pooling
* Minimal overhead (ideally, just syntax sugar/wrapper fns)

## Examples

More examples available in [tests](../tests/). Also in-use with a tiny little personal website app, [rusty-golf](https://github.com/derekfrye/rusty-golf).

See [Benchmarks](./Benchmarks.md) for details on performance testing.

### Importing

You can use the prelude to import everything you need, or import item by item.

```rust
use sql_middleware::prelude::*;
```

### Multi-database support without copy/pasting query logic

An example using multiple different backends (sqlite, postgres, turso). Notice the need to not repeat the query logic regardless of backend connection type. 

```rust
use sql_middleware::prelude::*;

pub async fn get_scores_from_db(
    config_and_pool: &sql_middleware::pool::ConfigAndPool,
    event_id: i32,
) -> Result<ScoresAndLastRefresh, SqlMiddlewareDbError> {
    let pool = config_and_pool.pool.get().await?;
    let mut conn = MiddlewarePool::get_connection(pool).await?;
    let query = match &conn {
        MiddlewarePoolConnection::Postgres(_) => {
            "SELECT grp, golfername, playername, eup_id, espn_id FROM sp_get_player_names($1) ORDER BY grp, eup_id"
        }
        MiddlewarePoolConnection::Sqlite(_) | MiddlewarePoolConnection::Turso(_) => {
            include_str!("../sql/functions/sqlite/03_sp_get_scores.sql")
        }
    };
    let params = vec![RowValues::Int(i64::from(event_id))];
    let res = conn.execute_select(query, &params).await?;

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

Same api regardless of db backend. Use `execute_batch` when you have no parameters to pass. 

```rust
// simple api for batch queries
let ddl_query =
    include_str!("/path/to/test1.sql");
conn.execute_batch(&ddl_query).await?;
```


### Parameterized queries for reading or changing data

Consistent API using `QueryAndParams` and `execute_select` (reading data) or `execute_dml` (changing data). Only the parameter syntax differs.

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
use chrono::NaiveDateTime;
// PostgreSQL uses $-style placeholders
let q = QueryAndParams::new(
    "INSERT INTO test (espn_id, name
        , ins_ts) VALUES ($1, $2, $3)",
    vec![
        RowValues::Int(123456),
        RowValues::Text(
            "test name".to_string()),
        RowValues::Timestamp(
            parse_from_str(
            "2021-08-06 16:00:00",
            "%Y-%m-%d %H:%M:%S",
        )?),
    ],
);

// Execute directly with RowValues
conn.execute_dml(&q.query, &q.params)
    .await?;
```

</td>
<td>

```rust
use chrono::NaiveDateTime;
// SQLite-compatible backends use ? or ?N
let q = QueryAndParams::new(
    "INSERT INTO test (espn_id, name
        , ins_ts) VALUES (?1, ?2, ?3)",
    vec![
        RowValues::Int(123456),
        RowValues::Text(
            "test name".to_string()),
        RowValues::Timestamp(
            parse_from_str(
            "2021-08-06 16:00:00",
            "%Y-%m-%d %H:%M:%S",
        )?),
    ],
);

// The same for all `sqlite` variants
conn.execute_dml(&q.query, &q.params)
    .await?;
```

</td>
</tr>
</table>

### Queries without parameters

You can issue no-parameter queries directly, the same for PostgreSQL, SQLite, LibSQL, and Turso:

```rust
// Either build a QueryAndParams
// And you could structure like previous example to pass param values
let query = QueryAndParams::new_without_params("SELECT * FROM users");
let results = conn.execute_select(&query.query, &[]).await?;

// Or pass the SQL string directly
let results2 = conn.execute_select("SELECT * FROM users", &[]).await?;
```

### Transactions with custom logic

Here, because the underlying libraries are different, unfortunately, if you need custom app logic between `transaction()` and `commit()`, the code will become a bit more repetitive.

See further examples in the tests directory:
- [SQLite test example](/tests/test5c_sqlite.rs), [SQLite bench example](../benches/bench_rusqlite_single_row_lookup.rs)
- [Turso test example](/tests/test5d_turso.rs)
- [PostgreSQL test example](/tests/test5a_postgres.rs)
- [LibSQL test example](/tests/test5b_libsql.rs)

<table>
<tr>
<th>
PostgreSQL / LibSQL / Turso
</th>
<th>
SQLite
</th>
</tr>
<tr>
<td>

```rust
// Get db-specific connection
let pg_conn = match &mut conn {
    Postgres(pg)
        => pg,
    _ => panic!("Expected connection"),
};

// Get client
// (N/A for Turso; no deadpool yet)
let client: &mut tokio_postgres::Client 
    = pg_conn.as_mut();

// Begin transaction
// Turso: `let tx = 
//  turso::begin_transaction(t).await?;`
let tx = client.transaction().await?;

// could run custom logic between stmts

// Prepare statement
// Turso: `let mut stmt 
//    = tx.prepare("... ?1, ?2 ...").await?;`
let stmt = tx.prepare(&q.query).await?;

// Convert params (Postgres)
// Turso/LibSQL not necessary
let converted_params = 
    convert_sql_params::<PostgresParams>(
        &q.params,
        ConversionMode::Execute
    )?;

// Execute query
// Turso: `tx.execute_prepared(
//  &mut stmt, &q.params).await?;`
// LibSQL: `tx.execute_prepared(
//  &stmt, &q.params).await?;`
let rows = tx.execute(
    &stmt, 
    converted_params.as_refs()
).await?;

// Commit
tx.commit().await?;
```

</td>
<td>

```rust
// Use the helper to run blocking SQLite code
let rows = match &mut conn {
    Sqlite(_) => conn
        .with_sqlite_connection(move |conn| {
            let tx = conn.transaction()?;
            
            // Convert parameters
            let converted_params = 
                convert_sql_params::
                <SqliteParamsExecute>(
                &q.params,
                ConversionMode::Execute
            )?;
            
            // could run custom logic between stmts

            // Prepare and execute
            let rows = {
                let mut stmt = tx.prepare(
                    &q.query)?;
                stmt.execute(converted_params.0)?
            };
            
            tx.commit()?;
            
            Ok::<_, SqlMiddlewareDbError>(rows)
        })
        .await?,
    _ => panic!("Expected connection"),
};

// Or prepare once and reuse the compiled statement across many calls
if let Sqlite(_) = &mut conn {
    let prepared = conn
        .prepare_sqlite_statement("SELECT name FROM users WHERE id = ?1")
        .await?;
    for user_id in ids {
        let result = prepared.query(&[RowValues::Int(*user_id)]).await?;
        // ...
    }
}

// Turso connections now expose the same prepared-statement helper.
if let Turso(_) = &mut conn {
    let prepared = conn
        .prepare_turso_statement("SELECT name FROM users WHERE id = ?1")
        .await?;
    for user_id in ids {
        let result = prepared.query(&[RowValues::Int(*user_id)]).await?;
        // ...
    }
}
```

</td>

</tr>
</table>

### Example function with custom logic transaction

This variant mirrors the earlier multi-backend example but expands it with backend-specific transaction steps so you can insert bespoke logic between `BEGIN` and `COMMIT`.

```rust
use sql_middleware::conversion::convert_sql_params;
use sql_middleware::exports::PostgresParams;
use sql_middleware::libsql::{begin_transaction as begin_libsql_tx, Params as LibsqlParams};
use sql_middleware::prelude::*;
use sql_middleware::turso::{begin_transaction, Params as TursoParams};

pub async fn get_scores_from_db(
    config_and_pool: &ConfigAndPool,
    event_id: i64,
) -> Result<ScoresAndLastRefresh, SqlMiddlewareDbError> {
    let pool = config_and_pool.pool.get().await?;
    let mut conn = MiddlewarePool::get_connection(pool).await?;

    let turso_query = "SELECT grp, golfername, playername, eup_id, espn_id \
                       FROM sp_get_player_names(?1) ORDER BY grp, eup_id";
    let postgres_query = "SELECT grp, golfername, playername, eup_id, espn_id \
                          FROM sp_get_player_names($1) ORDER BY grp, eup_id";

// Run some custom Rust code to fetch parameters.
            // Or do whatever custom code you wanted in here
            let dynamic_params: Vec<RowValues> =
                todo!("fetch parameters for event_id {event_id}");

    let rows = match &mut conn {
        MiddlewarePoolConnection::Turso(client) => {
            let mut tx = begin_transaction(client).await?;
            let mut stmt = tx.prepare(turso_query).await?;

            

            let _converted_params =
                convert_sql_params::<TursoParams>(&dynamic_params, ConversionMode::Query)?;

            let result = tx
                .query_prepared(&mut stmt, &dynamic_params)
                .await?;
            tx.commit().await?;
            result
        }
        MiddlewarePoolConnection::Postgres(pg_conn) => {
            let mut tx = pg_conn.transaction().await?;
            let stmt = tx.prepare(postgres_query).await?;

            let converted_params =
                convert_sql_params::<PostgresParams>(&dynamic_params, ConversionMode::Query)?;

            let result = sql_middleware::postgres::build_result_set(
                &stmt,
                converted_params.as_refs(),
                &tx,
            )
            .await?;
            tx.commit().await?;
            result
        }
        MiddlewarePoolConnection::Libsql(conn) => {
            let tx = begin_libsql_tx(conn).await?;
            let prepared = tx.prepare(turso_query)?;

            let _converted_params =
                convert_sql_params::<LibsqlParams>(&dynamic_params, ConversionMode::Query)?;

            let result = tx.query_prepared(&prepared, &dynamic_params).await?;
            tx.commit().await?;
            result
        }
        _ => {
            return Err(SqlMiddlewareDbError::Unimplemented(
                "expected Turso, Postgres, or LibSQL connection".to_string(),
            ));
        }
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

### Using the AsyncDatabaseExecutor trait

The `AsyncDatabaseExecutor` trait provides a consistent interface for database operations if you prefer designing this way.

```rust
// This works for PostgreSQL, SQLite, LibSQL, and Turso connections
async fn insert_user<T: AsyncDatabaseExecutor>(
    conn: &mut T,
    user_id: i32,
    name: &str
) -> Result<(), SqlMiddlewareDbError> {
    let query = QueryAndParams::new(
        // Use appropriate parameter syntax for your DB
        // PostgreSQL: "VALUES ($1, $2)"
        // SQLite/LibSQL/Turso: "VALUES (?, ?)" or "VALUES (?1, ?2)"
        "INSERT INTO users (id, name) VALUES ($1, $2)",
        vec![
            RowValues::Int(i64::from(user_id)),
            RowValues::Text(name.to_string()),
        ]
    );
    
    // Execute query through the trait. Placeholder style in `query.query`
    // must match the active backend.
    conn.execute_dml(&query.query, &query.params).await?;
    
    Ok(())
}
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

- 0.1.9 (unreleased): Switched the project license from BSD-2-Clause to MIT and added third-party notice documentation.
