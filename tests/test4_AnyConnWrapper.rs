use sql_middleware::postgres::{
    Params as PostgresParams, build_result_set as postgres_build_result_set,
};
use sql_middleware::sqlite::{Params as SqliteParams, build_result_set as sqlite_build_result_set};
use sql_middleware::{
    SqlMiddlewareDbError, convert_sql_params,
    middleware::{
        AnyConnWrapper, ConfigAndPool as ConfigAndPool2, ConversionMode, DatabaseType,
        MiddlewarePoolConnection, QueryAndParams, RowValues,
    },
};
use tokio::runtime::Runtime;

fn unique_path(prefix: &str) -> String {
    let pid = std::process::id();
    let ns = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("{prefix}_{pid}_{ns}.db")
}

struct FileCleanup(Vec<String>);

impl Drop for FileCleanup {
    fn drop(&mut self) {
        for p in &self.0 {
            let _ = std::fs::remove_file(p);
            let _ = std::fs::remove_file(format!("{p}-wal"));
            let _ = std::fs::remove_file(format!("{p}-shm"));
        }
    }
}

#[test]
fn test4_trait() -> Result<(), Box<dyn std::error::Error>> {
    #[allow(unused_mut)]
    let mut test_cases = vec![
        TestCase::Sqlite("file::memory:?cache=shared".to_string()),
        TestCase::Sqlite(unique_path("test_sqlite")),
    ];
    #[cfg(feature = "postgres")]
    {
        let mut cfg = deadpool_postgres::Config::new();
        cfg.dbname = Some("testing".to_string());
        cfg.host = Some("10.3.0.201".to_string());
        cfg.port = Some(5432);
        cfg.user = Some("testuser".to_string());
        cfg.password = Some(String::new());
        test_cases.push(TestCase::Postgres(Box::new(cfg)));
    }
    #[cfg(feature = "turso")]
    {
        test_cases.push(TestCase::Turso(":memory:".to_string()));
        test_cases.push(TestCase::Turso(unique_path("test_turso")));
    }

    // #[cfg(feature = "libsql")]
    // {
    //     test_cases.push(TestCase::Libsql(":memory:".to_string()));
    //     test_cases.push(TestCase::Libsql("test_libsql.db".to_string()));
    // }

    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let mut db_type: DatabaseType;

        // Drop guard to clean up file-backed DBs even on failure
        for test_case in test_cases {
            // Clean up database files if they exist, and register cleanup guard
            let _cleanup_guard = match &test_case {
                TestCase::Sqlite(connection_string) => {
                    db_type = DatabaseType::Sqlite;
                    if connection_string == "file::memory:?cache=shared" {
                        None
                    } else {
                        let _ = std::fs::remove_file(connection_string);
                        Some(FileCleanup(vec![connection_string.clone()]))
                    }
                }
                #[cfg(feature = "turso")]
                TestCase::Turso(connection_string) => {
                    db_type = DatabaseType::Turso;
                    if connection_string == ":memory:" {
                        None
                    } else {
                        let _ = std::fs::remove_file(connection_string);
                        let _ = std::fs::remove_file(format!("{connection_string}-wal"));
                        let _ = std::fs::remove_file(format!("{connection_string}-shm"));
                        Some(FileCleanup(vec![connection_string.clone()]))
                    }
                }
                // #[cfg(feature = "libsql")]
                // TestCase::Libsql(connection_string) => {
                //     if connection_string != ":memory:" {
                //         let _ = std::fs::remove_file(&connection_string);
                //     }
                // }
                #[cfg(feature = "postgres")]
                TestCase::Postgres(_) => {
                    db_type = DatabaseType::Postgres;
                    None
                }
            };

            let mut conn: MiddlewarePoolConnection;
            match test_case {
                TestCase::Sqlite(connection_string) => {
                    // Initialize Sqlite pool
                    let config_and_pool = ConfigAndPool2::new_sqlite(connection_string).await?;
                    conn = config_and_pool.get_connection().await?;

                    // Execute test logic
                    // run_test_logic(&mut conn, DatabaseType::Sqlite).await?;
                }
                #[cfg(feature = "postgres")]
                TestCase::Postgres(cfg) => {
                    // Initialize Postgres pool
                    let config_and_pool = ConfigAndPool2::new_postgres(*cfg).await?;
                    conn = config_and_pool.get_connection().await?;

                    // Execute test logic
                    // run_test_logic(&mut conn, DatabaseType::Postgres).await?;
                }
                #[cfg(feature = "turso")]
                TestCase::Turso(connection_string) => {
                    // Initialize Turso connection (no deadpool pooling)
                    let config_and_pool = ConfigAndPool2::new_turso(connection_string).await?;
                    conn = config_and_pool.get_connection().await?;
                }
            }
            if db_type == DatabaseType::Postgres {
                // Ensure a clean slate when reusing a shared Postgres instance.
                conn.execute_batch(
                    r"
                    DROP TABLE IF EXISTS eup_statistic CASCADE;
                    DROP TABLE IF EXISTS event_user_player CASCADE;
                    DROP TABLE IF EXISTS bettor CASCADE;
                    DROP TABLE IF EXISTS golfer CASCADE;
                    DROP TABLE IF EXISTS event CASCADE;
                    DROP TABLE IF EXISTS test CASCADE;
                    ",
                )
                .await?;
            }
            run_test_logic(&mut conn, db_type).await?;
        }

        // Ok(())
        Ok::<(), Box<dyn std::error::Error>>(())

        // ... rest of your test code ...
    })?;

    Ok(())
}

enum TestCase {
    Sqlite(String),
    #[cfg(feature = "postgres")]
    Postgres(Box<deadpool_postgres::Config>),
    #[cfg(feature = "turso")]
    Turso(String),
    // #[cfg(feature = "libsql")]
    // Libsql(String),
}

#[allow(clippy::too_many_lines)]
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
        DatabaseType::Mssql => vec![
            // Use SQLite scripts for MSSQL in test
            include_str!("../tests/sqlite/test4/00_event.sql"),
            include_str!("../tests/sqlite/test4/02_golfer.sql"),
            include_str!("../tests/sqlite/test4/03_bettor.sql"),
            include_str!("../tests/sqlite/test4/04_event_user_player.sql"),
            include_str!("../tests/sqlite/test4/05_eup_statistic.sql"),
        ],
        #[cfg(feature = "libsql")]
        DatabaseType::Libsql => vec![
            // Use SQLite scripts for LibSQL in test (LibSQL is SQLite-compatible)
            include_str!("../tests/sqlite/test4/00_event.sql"),
            include_str!("../tests/sqlite/test4/02_golfer.sql"),
            include_str!("../tests/sqlite/test4/03_bettor.sql"),
            include_str!("../tests/sqlite/test4/04_event_user_player.sql"),
            include_str!("../tests/sqlite/test4/05_eup_statistic.sql"),
        ],
        #[cfg(feature = "turso")]
        DatabaseType::Turso => vec![
            include_str!("../tests/turso/test4/00_event.sql"),
            include_str!("../tests/turso/test4/02_golfer.sql"),
            include_str!("../tests/turso/test4/03_bettor.sql"),
        ],
    };

    #[cfg(feature = "turso")]
    if db_type == DatabaseType::Turso {
        for (idx, stmt) in ddl.iter().enumerate() {
            conn.execute_batch(stmt).await.map_err(|error| {
                let part = idx + 1;
                SqlMiddlewareDbError::ExecutionError(format!(
                    "Turso DDL failure on part {part}: {error}"
                ))
            })?;
        }
    } else {
        let ddl_query = ddl.join("\n");
        conn.execute_batch(&ddl_query).await?;
    }
    #[cfg(not(feature = "turso"))]
    {
        let ddl_query = ddl.join("\n");
        conn.execute_batch(&ddl_query).await?;
    }

    // Define the setup queries
    let setup_queries = match db_type {
        DatabaseType::Postgres | DatabaseType::Sqlite => include_str!("test4.sql"),
        #[cfg(feature = "libsql")]
        DatabaseType::Libsql => include_str!("test4.sql"),
        #[cfg(feature = "turso")]
        DatabaseType::Turso => include_str!("../tests/turso/test4/setup.sql"),
        DatabaseType::Mssql => include_str!("test4.sql"),
    };
    conn.execute_batch(setup_queries).await?;

    let test_tbl_query = "CREATE TABLE test (id bigint, name text);";
    conn.execute_batch(test_tbl_query).await?;

    let parameterized_query = match db_type {
        DatabaseType::Postgres => "INSERT INTO test (id, name) VALUES ($1, $2);",
        DatabaseType::Sqlite => "INSERT INTO test (id, name) VALUES (?1, ?2);",
        DatabaseType::Mssql => "INSERT INTO test (id, name) VALUES (@p1, @p2);",
        #[cfg(feature = "libsql")]
        DatabaseType::Libsql => "INSERT INTO test (id, name) VALUES (?1, ?2);",
        #[cfg(feature = "turso")]
        DatabaseType::Turso => "INSERT INTO test (id, name) VALUES (?1, ?2);",
    };

    // generate 100 params
    let params: Vec<Vec<RowValues>> = (0..100)
        .map(|i| vec![RowValues::Int(i), RowValues::Text(format!("name_{i}"))])
        .collect();
    // dbg!(&params);

    // conn.execute_dml(parameterized_query, &params[0]).await?;

    // lets first run this through 100 transactions, yikes
    for param in params {
        // println!("param: {:?}", param);
        conn.query(parameterized_query).params(&param).dml().await?;
    }

    let query = "select count(*) as cnt from test;";
    let result_set = conn.query(query).select().await?;
    assert_eq!(
        *result_set.results[0].get("cnt").unwrap().as_int().unwrap(),
        100
    );

    // generate 100 more params
    let params: Vec<Vec<RowValues>> = (100..200)
        .map(|i| vec![RowValues::Int(i), RowValues::Text(format!("name_{i}"))])
        .collect();

    // now let's be a little smarter and write our own loop to exec a 100 inserts

    match db_type {
        DatabaseType::Postgres => {
            if let MiddlewarePoolConnection::Postgres {
                client: pg_handle, ..
            } = conn
            {
                let tx = pg_handle.transaction().await?;
                for param in params {
                    let postgres_params = PostgresParams::convert(&param)?;
                    tx.execute(parameterized_query, postgres_params.as_refs())
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
                                let converted_params = convert_sql_params::<SqliteParams>(
                                    &param,
                                    ConversionMode::Execute,
                                )?;
                                let refs = converted_params.as_refs();
                                tx.execute(&parameterized_query, &refs[..])?;
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
        DatabaseType::Mssql => {
            // For this test, skip the MSSQL implementation
            // Simply insert the data using the middleware connection
            for param in params {
                conn.query(parameterized_query).params(&param).dml().await?;
            }
        }
        #[cfg(feature = "turso")]
        DatabaseType::Turso => {
            for param in params {
                conn.query(parameterized_query).params(&param).dml().await?;
            }
        }
        #[cfg(feature = "libsql")]
        DatabaseType::Libsql => {
            // LibSQL is SQLite-compatible, use middleware connection
            for param in params {
                conn.query(parameterized_query).params(&param).dml().await?;
            }
        }
    }

    let query = "select count(*) as cnt from test;";
    let result_set = conn.query(query).select().await?;
    assert_eq!(
        *result_set.results[0].get("cnt").unwrap().as_int().unwrap(),
        200
    );

    // generate 200 more params
    let params: Vec<Vec<RowValues>> = (0..200)
        .map(|i| vec![RowValues::Int(i), RowValues::Text(format!("name_{i}"))])
        .collect();

    // now let's be a little smarter and write our own loop to exec inserts
    // yes, this isn't as quick as bulk loading, which isn't implemented yet

    match db_type {
        DatabaseType::Postgres => {
            if let MiddlewarePoolConnection::Postgres {
                client: pg_handle, ..
            } = conn
            {
                let tx = pg_handle.transaction().await?;
                for param in params {
                    let postgres_params = PostgresParams::convert(&param)?;
                    tx.execute(parameterized_query, postgres_params.as_refs())
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
                                        let converted_params = convert_sql_params::<SqliteParams>(
                                            &param,
                                            ConversionMode::Execute,
                                        )?;
                                        let refs = converted_params.as_refs();
                                        tx.execute(&parameterized_query, &refs[..])?;
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
        DatabaseType::Mssql | DatabaseType::Turso | DatabaseType::Libsql => {
            // For MS SQL, insert data using the middleware connection
            for param in params {
                conn.query(parameterized_query).params(&param).dml().await?;
            }
        }
    }

    let query = "select count(*) as cnt from test;";
    let result_set = conn.query(query).select().await?;
    assert_eq!(
        *result_set.results[0].get("cnt").unwrap().as_int().unwrap(),
        400
    );

    // let's test a common pattern in rusty-golf
    // generate 1 more param
    let params: Vec<RowValues> = vec![RowValues::Int(990), RowValues::Text("name_990".to_string())];

    let query_and_params = QueryAndParams {
        query: parameterized_query.to_string(),
        params,
    };

    (match &mut *conn {
        MiddlewarePoolConnection::Postgres { client: xx, .. } => {
            let tx = xx.transaction().await?;
            let converted_params = PostgresParams::convert_for_batch(&query_and_params.params)?;

            let stmt = tx.prepare(&query_and_params.query).await?;
            postgres_build_result_set(&stmt, &converted_params, &tx).await?;

            let stmt = tx.prepare(&query_and_params.query).await?;
            postgres_build_result_set(&stmt, &converted_params, &tx).await?;

            tx.commit().await?;
            Ok::<_, SqlMiddlewareDbError>(result_set)
        }
        MiddlewarePoolConnection::Mssql { .. } => {
            // For MSSQL, just execute the query using the middleware
            conn.query(&query_and_params.query)
                .params(&query_and_params.params)
                .dml()
                .await?;
            Ok::<_, SqlMiddlewareDbError>(result_set)
        }
        MiddlewarePoolConnection::Sqlite { .. } => {
            Ok(conn
                .with_sqlite_connection(move |xxx| {
                    let tx = xxx.transaction()?;
                    {
                        let converted_params = convert_sql_params::<SqliteParams>(
                            &query_and_params.params,
                            ConversionMode::Query,
                        )?;
                        let mut stmt = tx.prepare(&query_and_params.query)?;
                        sqlite_build_result_set(&mut stmt, converted_params.as_values())?;
                    }
                    // should be able to read that val even tho we're not done w tx
                    {
                        let query_and_params_vec = QueryAndParams {
                            query: "select count(*) as cnt from test;".to_string(),
                            params: vec![],
                        };
                        let converted_params = convert_sql_params::<SqliteParams>(
                            &query_and_params_vec.params,
                            ConversionMode::Query,
                        )?;
                        let mut stmt = tx.prepare(&query_and_params_vec.query)?;
                        let result_set =
                            sqlite_build_result_set(&mut stmt, converted_params.as_values())?;
                        assert_eq!(result_set.results.len(), 1);
                        assert_eq!(
                            *result_set.results[0].get("cnt").unwrap().as_int().unwrap(),
                            401
                        );
                    }
                    {
                        let converted_params = convert_sql_params::<SqliteParams>(
                            &query_and_params.params,
                            ConversionMode::Query,
                        )?;
                        let mut stmt = tx.prepare(&query_and_params.query)?;
                        sqlite_build_result_set(&mut stmt, converted_params.as_values())?;
                    }
                    tx.commit()?;
                    Ok::<_, SqlMiddlewareDbError>(result_set)
                })
                .await?)
        }

        MiddlewarePoolConnection::Libsql { .. } | MiddlewarePoolConnection::Turso { .. } => {
            // For LibSQL, just execute the query using the middleware
            conn.query(&query_and_params.query)
                .params(&query_and_params.params)
                .dml()
                .await?;
            conn.query(&query_and_params.query)
                .params(&query_and_params.params)
                .dml()
                .await?;
            Ok::<_, SqlMiddlewareDbError>(result_set)
        }
    })?;

    // println!("dbdriver: {:?}, res: {:?}", db_type, res);

    // make sure to match the val from above
    let query = "select count(*) as cnt,name from test where id = 990 group by name;";
    let result_set = conn.query(query).select().await?;
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
    let result_set = conn.query(query).select().await?;
    assert_eq!(
        *result_set.results[0].get("cnt").unwrap().as_int().unwrap(),
        402
    );
    // assert_eq!(*result_set.results[0].get("name").unwrap().as_text().unwrap(), *"name_990");

    // assert_eq!(res.rows_affected, 1);

    Ok(())
}
