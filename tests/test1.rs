use chrono::NaiveDateTime;
use serde_json::json;
use sqlx_middleware::{
    middleware::{
        ConfigAndPool, MiddlewarePool, MiddlewarePoolConnection, QueryAndParams, RowValues,
    },
    SqlMiddlewareDbError,
};
// use sqlx_middleware::convenience_items::{create_tables, MissingDbObjects};
// use sqlx_middleware::db::{ConfigAndPool, DatabaseType, Db, QueryState};

// use sqlx_middleware::model::{CheckType, QueryAndParams, RowValues};
use std::vec;
use tokio::runtime::Runtime;

#[test]
fn sqlite_mutltiple_column_test() -> Result<(), Box<dyn std::error::Error>> {
    let rt = Runtime::new()?;
    Ok(rt.block_on(async {
        let x = "file::memory:?cache=shared".to_string();

        let config_and_pool = ConfigAndPool::new_sqlite(x).await?;
        let pool = config_and_pool.pool.get().await?;
        let sqlite_conn = MiddlewarePool::get_connection(pool).await?;
        let sconn = match &sqlite_conn {
            MiddlewarePoolConnection::Sqlite(sconn) => sconn,
            _ => panic!("Only sqlite is supported "),
        };

        let ddl = vec![
            "CREATE TABLE IF NOT EXISTS -- drop table test cascade
                test (
                recid INTEGER PRIMARY KEY AUTOINCREMENT
                , a int
                , b text
                , c datetime not null default current_timestamp
                , d real
                , e boolean
                , f blob
                , g json
                );",
        ];
        let query_and_params = QueryAndParams {
            query: ddl[0].to_string(),
            params: vec![],
            is_read_only: false,
        };

        {
            sconn
                .interact(move |xxx| {
                    let tx = xxx.transaction()?;
                    let result_set = {
                        let rs = tx.execute_batch(&query_and_params.query)?;
                        rs
                    };
                    tx.commit()?;
                    Ok::<_, SqlMiddlewareDbError>(result_set)
                })
                .await?
        }?;

        let setup_queries = include_str!("../tests/sqlite/test2_setup.sql");
        let query_and_params = QueryAndParams {
            query: setup_queries.to_string(),
            params: vec![],
            is_read_only: false,
        };

        {
            sconn
                .interact(move |xxx| {
                    let tx = xxx.transaction()?;
                    let result_set = {
                        let rs = tx.execute_batch(&query_and_params.query)?;
                        rs
                    };
                    tx.commit()?;
                    Ok::<_, SqlMiddlewareDbError>(result_set)
                })
                .await?
        }?;

        let qry = "SELECT * from test where recid in (?,?, ?);";
        // let param = [RowValues::Int(1), RowValues::Int(2), RowValues::Int(3)];
        let param = vec![RowValues::Int(1)];
        let query_and_params = QueryAndParams {
            query: qry.to_string(),
            params: param.to_vec(),
            is_read_only: true,
        };
        let res = {
            sconn
                .interact(move |xxx| {
                    let converted_params =
                        sqlx_middleware::sqlite_convert_params(&query_and_params.params)?;
                    let tx = xxx.transaction()?;

                    let result_set = {
                        let mut stmt = tx.prepare(&query_and_params.query)?;
                        let rs =
                            sqlx_middleware::sqlite_build_result_set(&mut stmt, &converted_params)?;
                        rs
                    };
                    tx.commit()?;
                    Ok::<_, SqlMiddlewareDbError>(result_set)
                })
                .await
        }??;

        // dbg!(&res);

        // we expect 3 rows
        assert_eq!(res.results.len(), 3);

        // dbg!(&res.return_result[0].results[0]);

        // row 1 should decode as: 1, 'Alpha', '2024-01-01 08:00:01', 10.5, 1, X'426C6F623132', '{"name": "Alice", "age": 30}'
        assert_eq!(*res.results[0].get("recid").unwrap().as_int().unwrap(), 1);
        assert_eq!(*res.results[0].get("a").unwrap().as_int().unwrap(), 1);
        assert_eq!(res.results[0].get("b").unwrap().as_text().unwrap(), "Alpha");
        assert_eq!(
            res.results[0]
                .get("c")
                .and_then(|v| v.as_timestamp())
                .unwrap(),
            NaiveDateTime::parse_from_str("2024-01-01 08:00:01", "%Y-%m-%d %H:%M:%S").unwrap()
        );
        assert_eq!(res.results[0].get("d").unwrap().as_float().unwrap(), 10.5);
        assert_eq!(*res.results[0].get("e").unwrap().as_bool().unwrap(), true);
        assert_eq!(
            res.results[0].get("f").unwrap().as_blob().unwrap(),
            b"Blob12"
        );
        // troubleshoot this around step 3 of db.rs
        assert_eq!(
            json!(res.results[0].get("g").unwrap().as_text().unwrap()),
            json!(r#"{"name": "Alice", "age": 30}"#)
        );
        Ok::<(), Box<dyn std::error::Error>>(())
    })?)
}
