// use rusty_golf::controller::score;
// use rusty_golf::{controller::score::get_data_for_scores_page, model::CacheMap};

use common::postgres::{setup_postgres_container, stop_postgres_container};
// use deadpool_sqlite::rusqlite::{ self, params };
use sql_middleware::{
    convert_sql_params,
    middleware::{
        AnyConnWrapper, AsyncDatabaseExecutor, ConfigAndPool as ConfigAndPool2, ConversionMode,
        DatabaseType, MiddlewarePool, MiddlewarePoolConnection, QueryAndParams, RowValues,
    },
    postgres_build_result_set, sqlite_build_result_set, PostgresParams, SqlMiddlewareDbError,
    SqliteParamsExecute, SqliteParamsQuery,
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

async fn run_test_logic(
    conn: &mut MiddlewarePoolConnection,
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

    let test_tbl_query = "CREATE TABLE test (id bigint, name text);";
    conn.execute_batch(test_tbl_query).await?;

    let parameterized_query = match db_type {
        DatabaseType::Postgres => "INSERT INTO test (id, name) VALUES ($1, $2);",
        DatabaseType::Sqlite => "INSERT INTO test (id, name) VALUES (?1, ?2);",
    };

    // generate 100 params
    let params: Vec<Vec<RowValues>> = (0..100)
        .map(|i| vec![RowValues::Int(i), RowValues::Text(format!("name_{}", i))])
        .collect();
    // dbg!(&params);

    // conn.execute_dml(parameterized_query, &params[0]).await?;

    // lets first run this through 100 transactions, yikes
    for param in params {
        // println!("param: {:?}", param);
        conn.execute_dml(&parameterized_query, &param).await?;
    }

    let query = "select count(*) as cnt from test;";
    let result_set = conn.execute_select(query, &[]).await?;
    assert_eq!(
        *result_set.results[0].get("cnt").unwrap().as_int().unwrap(),
        100
    );

    // generate 100 more params
    let params: Vec<Vec<RowValues>> = (100..200)
        .map(|i| vec![RowValues::Int(i), RowValues::Text(format!("name_{}", i))])
        .collect();

    // now let's be a little smarter and write our own loop to exec a 100 inserts

    match db_type {
        DatabaseType::Postgres => {
            if let MiddlewarePoolConnection::Postgres(pg_handle) = conn {
                let tx = pg_handle.transaction().await?;
                for param in params {
                    let postgres_params = PostgresParams::convert(&param)?;
                    tx.execute(parameterized_query, &postgres_params.as_refs())
                        .await?;
                }
                tx.commit().await?;
            } else {
                // or return an error if we expect only Postgres here
                unimplemented!();
            }
        }
        DatabaseType::Sqlite => {
            let res = conn
                .interact_sync({
                    let parameterized_query = parameterized_query.to_string();
                    let params = params.clone();
                    move |wrapper| match wrapper {
                        AnyConnWrapper::Sqlite(sql_conn) => {
                            let tx = sql_conn.transaction()?;
                            for param in params {
                                let converted_params = convert_sql_params::<SqliteParamsExecute>(
                                    &param,
                                    ConversionMode::Execute,
                                )?;
                                tx.execute(&parameterized_query, converted_params.0)?;
                            }
                            tx.commit()?;
                            Ok(())
                        }
                        _ => Err(SqlMiddlewareDbError::Other(
                            "Unexpected database type".into(),
                        )),
                    }
                })
                .await?;
            res?;
        }
    }

    let query = "select count(*) as cnt from test;";
    let result_set = conn.execute_select(query, &[]).await?;
    assert_eq!(
        *result_set.results[0].get("cnt").unwrap().as_int().unwrap(),
        200
    );

    // generate 200 more params
    let params: Vec<Vec<RowValues>> = (0..200)
        .map(|i| vec![RowValues::Int(i), RowValues::Text(format!("name_{}", i))])
        .collect();

    // now let's be a little smarter and write our own loop to exec inserts
    // yes, this isn't as quick as bulk loading, which isn't implemented yet

    match db_type {
        DatabaseType::Postgres => {
            if let MiddlewarePoolConnection::Postgres(pg_handle) = conn {
                let tx = pg_handle.transaction().await?;
                for param in params {
                    let postgres_params = PostgresParams::convert(&param)?;
                    tx.execute(parameterized_query, &postgres_params.as_refs())
                        .await?;
                }
                tx.commit().await?;
            } else {
                // or return an error if we expect only Postgres here
                unimplemented!();
            }
        }
        DatabaseType::Sqlite => {
            let res = conn
                .interact_sync({
                    let parameterized_query = parameterized_query.to_string();
                    let params = params.clone();
                    move |wrapper| {
                        match wrapper {
                            AnyConnWrapper::Sqlite(sql_conn) => {
                                let tx = sql_conn.transaction()?;
                                {
                                    let query = "select count(*) as cnt from test;";

                                    let mut stmt = tx.prepare(query)?;
                                    let mut res = stmt.query([])?;
                                    // let cnt: i64 = res.next().unwrap().get(0)?;
                                    let x: i32 = if let Some(row) = res.next()? {
                                        row.get(0)?
                                    } else {
                                        0
                                    };
                                    assert_eq!(x, 200);
                                }
                                {
                                    for param in params {
                                        let converted_params = convert_sql_params::<
                                            SqliteParamsExecute,
                                        >(
                                            &param, ConversionMode::Execute
                                        )?;
                                        tx.execute(&parameterized_query, converted_params.0)?;
                                    }
                                }
                                {
                                    let query = "select count(*) as cnt from test;";

                                    let mut stmt = tx.prepare(query)?;
                                    let mut res = stmt.query([])?;
                                    // let cnt: i64 = res.next().unwrap().get(0)?;
                                    let x: i32 = if let Some(row) = res.next()? {
                                        row.get(0)?
                                    } else {
                                        0
                                    };
                                    assert_eq!(x, 400);
                                }
                                tx.commit()?;
                                Ok(())
                            }
                            _ => Err(SqlMiddlewareDbError::Other(
                                "Unexpected database type".into(),
                            )),
                        }
                    }
                })
                .await?;
            res?;
        }
    }

    let query = "select count(*) as cnt from test;";
    let result_set = conn.execute_select(query, &[]).await?;
    assert_eq!(
        *result_set.results[0].get("cnt").unwrap().as_int().unwrap(),
        400
    );

    // let's test a common pattern in rusty-golf
    // generate 1 more param
    let params: Vec<RowValues> = vec![
        RowValues::Int(990),
        RowValues::Text(format!("name_{}", 990)),
    ];

    let query_and_params = QueryAndParams {
        query: parameterized_query.to_string(),
        params,
    };

    (match &mut *conn {
        MiddlewarePoolConnection::Postgres(xx) => {
            let tx = xx.transaction().await?;
            let converted_params = PostgresParams::convert_for_batch(&query_and_params.params)?;

            let stmt = tx.prepare(&query_and_params.query).await?;
            postgres_build_result_set(&stmt, &converted_params, &tx).await?;

            let stmt = tx.prepare(&query_and_params.query).await?;
            postgres_build_result_set(&stmt, &converted_params, &tx).await?;

            tx.commit().await?;
            Ok::<_, SqlMiddlewareDbError>(result_set)
        }
        MiddlewarePoolConnection::Sqlite(ref mut xx) => {
            xx.interact(move |xxx| {
                let tx = xxx.transaction()?;
                {
                    let converted_params = convert_sql_params::<SqliteParamsQuery>(
                        &query_and_params.params,
                        ConversionMode::Query,
                    )?;
                    let mut stmt = tx.prepare(&query_and_params.query)?;
                    sqlite_build_result_set(&mut stmt, &converted_params.0)?;
                }
                // should be able to read that val even tho we're not done w tx
                {
                    let query_and_params_vec = QueryAndParams {
                        query: "select count(*) as cnt from test;".to_string(),
                        params: vec![],
                    };
                    let converted_params = convert_sql_params::<SqliteParamsQuery>(
                        &query_and_params_vec.params,
                        ConversionMode::Query,
                    )?;
                    let mut stmt = tx.prepare(&query_and_params_vec.query)?;
                    let result_set = {
                        let rs = sql_middleware::sqlite_build_result_set(
                            &mut stmt,
                            &converted_params.0,
                        )?;
                        rs
                    };
                    assert_eq!(result_set.results.len(), 1);
                    assert_eq!(
                        *result_set.results[0].get("cnt").unwrap().as_int().unwrap(),
                        401
                    );
                }
                {
                    let converted_params = convert_sql_params::<SqliteParamsQuery>(
                        &query_and_params.params,
                        ConversionMode::Query,
                    )?;
                    let mut stmt = tx.prepare(&query_and_params.query)?;
                    sqlite_build_result_set(&mut stmt, &converted_params.0)?;
                }
                tx.commit()?;
                Ok::<_, SqlMiddlewareDbError>(result_set)
            })
            .await?
        }
    })?;

    // println!("dbdrive: {:?}, res: {:?}", db_type, res);

    // make sure to match the val frmo above
    let query = "select count(*) as cnt,name from test where id = 990 group by name;";
    let result_set = conn.execute_select(query, &[]).await?;
    // theres two in here now, we inserted same val 2x above
    assert_eq!(
        *result_set.results[0].get("cnt").unwrap().as_int().unwrap(),
        2
    );
    assert_eq!(
        *result_set.results[0]
            .get("name")
            .unwrap()
            .as_text()
            .unwrap(),
        *"name_990"
    );

    let query = "select count(*) as cnt from test ;";
    let result_set = conn.execute_select(query, &[]).await?;
    assert_eq!(
        *result_set.results[0].get("cnt").unwrap().as_int().unwrap(),
        402
    );
    // assert_eq!(*result_set.results[0].get("name").unwrap().as_text().unwrap(), *"name_990");

    // assert_eq!(res.rows_affected, 1);

    Ok(())
}
