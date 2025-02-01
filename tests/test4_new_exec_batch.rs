// use rusty_golf::controller::score;
// use rusty_golf::{controller::score::get_data_for_scores_page, model::CacheMap};

use common::postgres::{setup_postgres_container, stop_postgres_container};
use deadpool_sqlite::rusqlite;
use sql_middleware::{
    middleware::{AnyConnWrapper, ConfigAndPool as ConfigAndPool2, DatabaseExecutor, DatabaseType, MiddlewarePool, QueryAndParams, RowValues}, sqlite_convert_params, PostgresParams, SqlMiddlewareDbError
};
use tokio::runtime::Runtime;
mod common {
    pub mod postgres;
}

#[test]
fn test4_trait() -> Result<(), Box<dyn std::error::Error>> {
    let db_user = "test_user";
    // don't use @ or # in here, it fails
    // https://github.com/launchbadge/sqlx/issues/1624
    let db_pass = "test_passwordx(!323341";
    let db_name = "test_db";

    let mut cfg = deadpool_postgres::Config::new();
    cfg.dbname = Some(db_name.to_string());
    cfg.host = Some("localhost".to_string());
    // cfg.port = Some(port);

    cfg.user = Some(db_user.to_string());
    cfg.password = Some(db_pass.to_string());
    let postgres_stuff = setup_postgres_container(&cfg)?;
    cfg.port = Some(postgres_stuff.port);

    let test_cases = vec![
        TestCase::Sqlite("file::memory:?cache=shared".to_string()),
        TestCase::Postgres(&cfg), // Adjust connection string
    ];

    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        for test_case in test_cases {
            match test_case {
                TestCase::Sqlite(connection_string) => {
                    // Initialize Sqlite pool
                    let config_and_pool = ConfigAndPool2::new_sqlite(connection_string).await?;
                    let pool = config_and_pool.pool.get().await?;
                    let mut conn = MiddlewarePool::get_connection(pool).await?;

                    // Execute test logic
                    run_test_logic(&mut conn, DatabaseType::Sqlite).await?;
                }
                TestCase::Postgres(cfg) => {
                    // Initialize Postgres pool
                    let config_and_pool = ConfigAndPool2::new_postgres(cfg.clone()).await?;
                    let pool = config_and_pool.pool.get().await?;
                    let mut conn = MiddlewarePool::get_connection(pool).await?;

                    // Execute test logic
                    run_test_logic(&mut conn, DatabaseType::Postgres).await?;
                }
            }
        }

        // Ok(())
        Ok::<(), Box<dyn std::error::Error>>(())

        // ... rest of your test code ...
    })?;
    stop_postgres_container(postgres_stuff);

    Ok(())
}

enum TestCase<'a> {
    Sqlite(String),
    Postgres(&'a deadpool_postgres::Config),
}

async fn run_test_logic<C: DatabaseExecutor>(
    conn: &mut C,
    db_type: DatabaseType,
) -> Result<(), SqlMiddlewareDbError> {
    // Define the DDL statements
    let ddl = match db_type {
        DatabaseType::Postgres => vec![
            include_str!("../tests/postgres/test4/00_event.sql"),
            // include_str!("../src/admin/model/sql/schema/sqlite/01_golfstatistic.sql"),
            include_str!("../tests/postgres/test4/02_golfer.sql"),
            include_str!("../tests/postgres/test4/03_bettor.sql"),
            include_str!("../tests/postgres/test4/04_event_user_player.sql"),
            include_str!("../tests/postgres/test4/05_eup_statistic.sql"),
        ],
        DatabaseType::Sqlite => vec![
            include_str!("../tests/sqlite/test4/00_event.sql"),
            // include_str!("../src/admin/model/sql/schema/sqlite/01_golfstatistic.sql"),
            include_str!("../tests/sqlite/test4/02_golfer.sql"),
            include_str!("../tests/sqlite/test4/03_bettor.sql"),
            include_str!("../tests/sqlite/test4/04_event_user_player.sql"),
            include_str!("../tests/sqlite/test4/05_eup_statistic.sql"),
        ],
    };

    let ddl_query = ddl.join("\n");
    conn.execute_batch(&ddl_query).await?;

    // Define the setup queries
    let setup_queries = include_str!("test4.sql");
    conn.execute_batch(setup_queries).await?;

    let test_tbl_query = "CREATE TABLE test (id int, name text);";
    conn.execute_batch(test_tbl_query).await?;

    let paramaterized_query = match db_type {
        DatabaseType::Postgres => "INSERT INTO test (id, name) VALUES ($1, $2);",
        DatabaseType::Sqlite => "INSERT INTO test (id, name) VALUES (?1, ?2);",
    };

    // generate 100 params
    let params: Vec<Vec<RowValues>> = (0..100)
        .map(|i| vec![RowValues::Int(i), RowValues::Text(format!("name_{}", i))])
        .collect();

// conn.execute_dml(paramaterized_query, &params[0]).await?;

    // lets first run this through 100 transactions, yikes
    for param in params {
        conn.execute_dml(&paramaterized_query, &param).await?;
    }

     // generate 100 params
     let params: Vec<Vec<RowValues>> = (100..200)
     .map(|i| vec![RowValues::Int(i), RowValues::Text(format!("name_{}", i))])
     .collect();

    // now let's be a little smarter and write our own loop to exec a 100 inserts
        
                let rows_affected = conn
                .interact_async(|wrapper| async move {
                    match wrapper {
                        AnyConnWrapper::Sqlite(sql_conn) => {
                            // SQLite is synchronous. Even though *this* is an async closure, 
                            // the actual calls block inside `block_on()`.
                            let tx = sql_conn.transaction()?;
                            for param in params {
                                let converted_params = sqlite_convert_params(&param)?;
                                let rows = tx.execute(paramaterized_query, &converted_params.iter().map(|v| v as &dyn rusqlite::ToSql).collect::<Vec<_>>()[..])?;
                            }
                            tx.commit()?;
                            Ok(())
                        }
                        AnyConnWrapper::Postgres(pg_handle) => {
                            // Postgres is async. We can await transaction calls normally:
                            let client = pg_handle.as_ref(); // &tokio_postgres::Client
                            let mut tx = client.transaction().await?;
                            for param in params {
                                let postgres_params = PostgresParams::convert(&param)?;
                                let rows = tx.execute(paramaterized_query, &postgres_params.as_refs()).await?;
                            }
                            tx.commit().await?;
                            Ok(())
                        }
                    }
                })
                .await?;

    //             let tx = conn.transaction().await?;
    //             for param in params{
    //             let params = PostgresParams::convert(&param)?;
    
    // let stmt = tx.prepare(query).await?;
    // let result_set = build_result_set(&stmt, params.as_refs(), &tx).await?;
    // tx.commit().await?;
                       
    

    // let parameterized_select = match db_type {
    //     DatabaseType::Postgres => "SELECT * FROM test WHERE id = $1;",
    //     DatabaseType::Sqlite => "SELECT * FROM test WHERE id = ?1;",
    // };

    // let result_set = conn.execute_select(parameterized_select, &[RowValues::Int(1)]).await?;

    Ok(())
}
