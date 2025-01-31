# sql-middleware

Sql-middleware is an alternative to [SQLx](https://crates.io/crates/sqlx), focused on far fewer features, but a closer-to-native API for PostgreSQL & SQLite. Offering just a few convenience functions, its a small library with a similar API regardless of database backend. Includes the ability to drop-down to the underlying features of [tokio-postgres](https://crates.io/crates/tokio-postgres) or [rusqlite](https://crates.io/crates/rusqlite) libraries.

Created after first trying SQLx, being annoyed [as others already noted](https://www.reddit.com/r/rust/comments/16cfcgt/seeking_advice_considering_abandoning_sqlx_after/?rdt=44192), and wanting an alternative. 

## Goals
* Give calling code convenience methods for common query patterns without preventing calling code to interact with the underlying deadpool-sqlite and deadpool-postgres libraries.
* Minimal overhead (just synatax convenience).
* Built in connection pooling using [deadpool-postgres](https://crates.io/crates/deadpool-postgres) and [deadpool-sqlite](https://crates.io/crates/deadpool-sqlite).

## Example

Similar syntax whether SQLite or Postgres. Here's a postgres example:
```rust
let mut cfg = deadpool_postgres::Config::new();
cfg.dbname = Some("test_db".to_string());
cfg.host = Some("192.168.2.1".to_string());
cfg.port = Some(5432);
cfg.user = Some("test user".to_string());
cfg.password = Some("passwd".to_string());
let sql_configandpool = DbConfigAndPool::new(cfg, DatabaseType::Postgres).await;
let sql_db = Db::new(sql_configandpool.clone()).unwrap();
let qry = "SELECT * from $fn1 where eventname in ($1,$3,$4);";
let param = [RowValues::Text("event1".to_string()), RowValues::Text("event1".to_string()), RowValues::Text("event1".to_string()), RowValues::Text("event1".to_string())];
let sql_fn = ["sp_get_event_name($1)"];
let query_and_params = QueryAndParams {
    query: qry.to_string(),
    params: param.to_vec(),
    fns: sql_fn.to_vec(),
};
let res = sql_db.exec_general_query(vec![query_and_params], true).await.unwrap();
```
Just change definition of `cfg`, `DatabaseType`, and specify the sql for the `fn` with your values if using sqlite. (We still use `deadpool_postgres` and everything else the same.) Here's the sqlite equivalent:
```rust
let mut cfg = deadpool_postgres::Config::new();
cfg.dbname = Some(":memory:".to_string());
// no need to define cfg.host or other cfg values for sqlite
let sql_configandpool = DbConfigAndPool::new(cfg, DatabaseType::Sqlite).await;
let sql_db = Db::new(sql_configandpool.clone()).unwrap();
let qry = "SELECT * from $fn1 where eventname in ($1,$3,$4);";
let param = [RowValues::Int(10), RowValues::Int(25), RowValues::Int(3), RowValues::Int(10)];
let sql_fn = ["SELECT e.name AS eventname FROM event AS e WHERE e.espn_id = $1;"];
let query_and_params = QueryAndParams {
    query: qry.to_string(),
    params: param.to_vec(),
    fns: sql_fn.to_vec(),
};
let res = sql_db.exec_general_query(vec![query_and_params], true).await.unwrap();
```
This library is being used by the tiny family golf tournament website [rusty-golf](https://github.com/derekfrye/rusty-golf.git). I doubt it'll be used anywhere else!