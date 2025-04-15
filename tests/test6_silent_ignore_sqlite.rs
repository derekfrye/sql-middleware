///! This test verifies that querying for a non-existent column correctly generates
///! an error and does not silently ignore the issue.
///!
///! A user-reported bug claimed that querying for non-existent columns might be silently
///! ignored, but our investigation shows that proper errors are generated at the prepare stage.

use sql_middleware::{
    convert_sql_params,
    middleware::{
        ConversionMode,
        MiddlewarePoolConnection::{self},
    },
    SqliteParamsQuery,
};

use sql_middleware::middleware::{ConfigAndPool, MiddlewarePool, QueryAndParams};
use sql_middleware::SqlMiddlewareDbError;
use tokio::runtime::Runtime;

#[test]
fn test_nonexistent_column_properly_errors() -> Result<(), Box<dyn std::error::Error>> {
    let rt = Runtime::new()?;
    rt.block_on(async {
        // Setup in-memory SQLite database
        let db_path = "file::memory:?cache=shared".to_string();
        let config_and_pool = ConfigAndPool::new_sqlite(db_path).await.unwrap();
        let pool = config_and_pool.pool.get().await?;
        let sqlite_conn = MiddlewarePool::get_connection(&pool).await?;
        let sconn = match &sqlite_conn {
            MiddlewarePoolConnection::Sqlite(sconn) => sconn,
            _ => panic!("Only sqlite is supported"),
        };

        // Create test table with columns a through g (but no column h)
        let ddl = "CREATE TABLE IF NOT EXISTS test (
            recid INTEGER PRIMARY KEY AUTOINCREMENT
            , a int
            , b text
            , c datetime not null default current_timestamp
            , d real
            , e boolean
            , f blob
            , g json
            );";

        let query_and_params = QueryAndParams {
            query: ddl.to_string(),
            params: vec![],
        };

        // Execute the table creation
        {
            sconn
                .interact(move |db_conn| {
                    let tx = db_conn.transaction()?;
                    let result = tx.execute_batch(&query_and_params.query)?;
                    tx.commit()?;
                    Ok::<_, SqlMiddlewareDbError>(result)
                })
                .await?
        }?;

        // Now try to query with a non-existent column 'h'
        let query_and_params = QueryAndParams {
            query: "SELECT a, h FROM test;".to_string(),
            params: vec![],
        };

        // Verify that attempting to query for a non-existent column results in an error
        let result = match &sqlite_conn {
            MiddlewarePoolConnection::Sqlite(sqlite_conn) => {
                sqlite_conn
                    .interact(move |db_conn| {
                        let converted_params = convert_sql_params::<SqliteParamsQuery>(
                            &query_and_params.params,
                            ConversionMode::Query,
                        )?;
                        let tx = db_conn.transaction()?;

                        // This is where we expect the error to be caught
                        let result_set = {
                            let mut stmt = tx.prepare(&query_and_params.query)?;
                            sql_middleware::sqlite_build_result_set(&mut stmt, &converted_params.0)?
                        };
                        
                        tx.commit()?;
                        Ok::<_, SqlMiddlewareDbError>(result_set)
                    })
                    .await
            }
            _ => Ok(Err(SqlMiddlewareDbError::Other(
                "Database type not supported for this operation".to_string(),
            ))),
        };
        
        // The query should have failed with an appropriate SqliteError
        match result {
            Ok(inner_result) => {
                match inner_result {
                    Ok(_) => {
                        panic!("Expected an error when querying for non-existent column, but got success");
                    }
                    Err(e) => {
                        // Verify we get the correct error type
                        match e {
                            SqlMiddlewareDbError::SqliteError(ref sqlite_err) => {
                                // Confirm error message contains reference to non-existent column
                                let err_msg = format!("{:?}", sqlite_err);
                                assert!(
                                    err_msg.contains("no such column"), 
                                    "Expected error about non-existent column, got: {}", err_msg
                                );
                            }
                            _ => {
                                panic!("Wrong error type: {:?}, expected SqliteError", e);
                            }
                        }
                    }
                }
            }
            Err(e) => {
                panic!("Unexpected outer error: {:?}", e);
            }
        }
        
        Ok::<(), Box<dyn std::error::Error>>(())
    })
}