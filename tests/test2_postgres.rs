// use sqlx_middleware::convenience_items::{ create_tables, MissingDbObjects };
// use sqlx_middleware::db::{ QueryState, DatabaseType, Db, ConfigAndPool };
// use sqlx_middleware::model::{ CheckType, CustomDbRow, DatabaseResult, QueryAndParams, RowValues };
use chrono::NaiveDateTime;
// use sqlx::{ Connection, Executor };

use sql_middleware::middleware::{
    ConfigAndPool,
    ConversionMode,
    MiddlewarePool,
    MiddlewarePoolConnection,
    QueryAndParams,
    RowValues,
};
use sql_middleware::{
    convert_sql_params,
    postgres_build_result_set,
    PostgresParams,
    SqlMiddlewareDbError,
};
#[cfg(feature = "test-utils")]
use sql_middleware::test_utils::testing_postgres::{setup_postgres_container, stop_postgres_container};

use std::vec;
use tokio::runtime::Runtime;

#[test]
fn test2_postgres_cr_and_del_tbls() -> Result<(), Box<dyn std::error::Error>> {
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

    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        // env::var("DB_USER") = Ok("postgres".to_string());

        let stmt =
            "CREATE TABLE IF NOT EXISTS -- drop table event cascade
                    test (
                    event_id BIGSERIAL NOT NULL PRIMARY KEY,
                    espn_id BIGINT NOT NULL,
                    name TEXT NOT NULL,
                    ins_ts TIMESTAMP NOT NULL DEFAULT now()
                    );
                CREATE TABLE IF NOT EXISTS -- drop table event cascade
                    test_2 (
                    event_id BIGSERIAL NOT NULL PRIMARY KEY,
                    espn_id BIGINT NOT NULL,
                    name TEXT NOT NULL,
                    ins_ts TIMESTAMP NOT NULL DEFAULT now()
                    );";

        let config_and_pool = ConfigAndPool::new_postgres(cfg).await?;
        let pool = config_and_pool.pool.get().await?;
        let conn = MiddlewarePool::get_connection(&pool).await?;
        let mut pgconn = match conn {
            MiddlewarePoolConnection::Postgres(pgconn) => pgconn,
            MiddlewarePoolConnection::Sqlite(_) => {
                panic!("Only sqlite is supported");
            }
        };

        ({
            let tx = pgconn.transaction().await?;
            let result_set = {
                let rs = tx.batch_execute(stmt).await?;
                rs
            };
            tx.commit().await?;
            Ok::<_, SqlMiddlewareDbError>(result_set)
        })?;

        let query = "DELETE FROM test;";
        ({
            let tx = pgconn.transaction().await?;
            let result_set = {
                let rs = tx.batch_execute(query).await?;
                rs
            };
            tx.commit().await?;
            Ok::<_, SqlMiddlewareDbError>(result_set)
        })?;

        let query_and_params = QueryAndParams {
            query: "INSERT INTO test (espn_id, name, ins_ts) VALUES ($1, $2, $3)".to_string(),
            params: vec![
                RowValues::Int(123456),
                RowValues::Text("test name".to_string()),
                RowValues::Timestamp(
                    NaiveDateTime::parse_from_str("2021-08-06 16:00:00", "%Y-%m-%d %H:%M:%S")?
                )
            ],
        };

        let converted_params = convert_sql_params::<PostgresParams>(
            &query_and_params.params,
            ConversionMode::Execute
        )?;

        let tx = pgconn.transaction().await?;
        tx.prepare(query_and_params.query.as_str()).await?;
        tx.execute(query_and_params.query.as_str(), &converted_params.as_refs()).await?;
        tx.commit().await?;

        let query = "select * FROM test;";
        let result = ({
            let tx = pgconn.transaction().await?;
            let stmt = tx.prepare(query).await?;
            let result_set = {
                let rs = postgres_build_result_set(&stmt, &[], &tx).await?;
                rs
            };
            tx.commit().await?;
            Ok::<_, SqlMiddlewareDbError>(result_set)
        })?;

        let expected_result = vec![sql_middleware::test_helpers::create_test_row(
            vec![
                "event_id".to_string(),
                "espn_id".to_string(),
                "name".to_string(),
                "ins_ts".to_string()
            ],
            vec![
                RowValues::Int(1),
                RowValues::Int(123456),
                RowValues::Text("test name".to_string()),
                RowValues::Timestamp(
                    NaiveDateTime::parse_from_str(
                        "2021-08-06 16:00:00",
                        "%Y-%m-%d %H:%M:%S"
                    ).unwrap()
                )
            ]
        )];

        let cols_to_actually_check = vec!["espn_id", "name", "ins_ts"];

        for (index, row) in result.results.iter().enumerate() {
            let left: Vec<RowValues> = row.column_names
                .iter()
                .zip(&row.rows) // Pair column names with corresponding row values
                .filter(|(col_name, _)| cols_to_actually_check.contains(&col_name.as_str()))
                .map(|(_, value)| value.clone()) // Collect the filtered row values
                .collect();

            // Get column names and row values from the expected result
            let right: Vec<RowValues> = expected_result[index].column_names
                .iter()
                .zip(&expected_result[index].rows) // Pair column names with corresponding row values
                .filter(|(col_name, _)| cols_to_actually_check.contains(&col_name.as_str()))
                .map(|(_, value)| value.clone()) // Collect the filtered row values
                .collect();

            assert_eq!(left, right);
        }

        let query = "DROP TABLE test;
        DROP TABLE test_2;";
        ({
            let tx = pgconn.transaction().await?;
            let result_set = {
                let rs = tx.batch_execute(query).await?;
                rs
            };
            tx.commit().await?;
            Ok::<_, SqlMiddlewareDbError>(result_set)
        })?;

        // stop the container

        Ok::<(), Box<dyn std::error::Error>>(())
    })?;

    stop_postgres_container(postgres_stuff);

    Ok(())
}
