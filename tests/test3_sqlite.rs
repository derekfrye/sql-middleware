use chrono::NaiveDateTime;
use serde_json::json;
// use sqlx_middleware::convenience_items::{create_tables3, MissingDbObjects};
use sql_middleware::{
    convert_sql_params,
    middleware::{
        ConversionMode,
        MiddlewarePoolConnection::{self},
        RowValues,
    },
    sqlite_build_result_set, SqliteParamsExecute, SqliteParamsQuery,
};

use sql_middleware::middleware::{ConfigAndPool, MiddlewarePool, QueryAndParams};
// use sqlx_middleware::model::CheckType;
use sql_middleware::SqlMiddlewareDbError;
use std::vec;
use tokio::runtime::Runtime;

#[test]
fn sqlite_multiple_column_test_db2() -> Result<(), Box<dyn std::error::Error>> {
    let rt = Runtime::new()?;
    rt.block_on(async {
        let x = "file::memory:?cache=shared".to_string();
        // let x = "xxx".to_string();
        // cfg.dbname = Some("xxx".to_string());
        let config_and_pool = ConfigAndPool::new_sqlite(x).await.unwrap();
        let pool = config_and_pool.pool.get().await?;
        let sqlite_conn = MiddlewarePool::get_connection(&pool).await?;
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

        let setup_queries = vec![
            include_str!("../tests/sqlite/test3/test3_01_setup.sql"),
            include_str!("../tests/sqlite/test3/test3_02_setup.sql"),
            include_str!("../tests/sqlite/test3/test3_03_setup.sql"),
            include_str!("../tests/sqlite/test3/test3_04_setup.sql"),
            include_str!("../tests/sqlite/test3/test3_05_setup.sql"),
            include_str!("../tests/sqlite/test3/test3_06_setup.sql"),
            include_str!("../tests/sqlite/test3/test3_07_setup.sql"),
            include_str!("../tests/sqlite/test3/test3_08_setup.sql"),
            include_str!("../tests/sqlite/test3/test3_09_setup.sql"),
            include_str!("../tests/sqlite/test3/test3_10_setup.sql"),
        ];
        let params = vec![
            vec![RowValues::Int(1)],
            vec![RowValues::Int(2)],
            vec![RowValues::Int(3)],
            vec![RowValues::Int(4)],
            vec![RowValues::Int(5)],
            vec![RowValues::Int(6)],
            vec![RowValues::Int(7)],
            vec![RowValues::Int(8)],
            vec![RowValues::Int(9)],
            vec![
                RowValues::Int(100),
                RowValues::Text("Juliet".to_string()),
                RowValues::Float(100.75),
            ],
        ];
        let query_and_params_vec = setup_queries
            .iter()
            .zip(params.iter())
            .map(|(a, b)| QueryAndParams {
                query: a.to_string(),
                params: b.to_vec(),
            })
            .collect::<Vec<_>>();

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

        let qry = "SELECT * from test where recid in ( ?1, ?2, ?3, ?4);";
        let param = [
            RowValues::Int(1),
            RowValues::Int(2),
            RowValues::Int(3),
            RowValues::Int(10),
        ];
        // let param = [RowValues2::Int(1)];
        // let param = vec![RowValues::Int(1)];
        let query_and_params = QueryAndParams {
            query: qry.to_string(),
            params: param.to_vec(),
        };

        let res = {
            sconn
                .interact(move |conn| {
                    // Start a transaction
                    let converted_params = convert_sql_params::<SqliteParamsQuery>(
                        &query_and_params.params,
                        ConversionMode::Query,
                    )?;

                    let tx = conn.transaction()?;
                    let result_set = {
                        let mut stmt = tx.prepare(&query_and_params.query)?;
                        let rs = sqlite_build_result_set(&mut stmt, &converted_params.0)?;
                        rs
                    };
                    tx.commit()?;

                    Ok::<_, SqlMiddlewareDbError>(result_set)
                })
                .await
                .map_err(|e| format!("Error executing query: {:?}", e))
        }??;

        // we expect 4 rows
        assert_eq!(res.results.len(), 4);

        // assert_eq!(res.return_result[0].results.len(), 3);

        // dbg!(&res.return_result[0].results[0]);

        // row 1 should decode as: 1, 'Alpha', '2024-01-01 08:00:01', 10.5, 1, X'426C6F623132', '{"name": "Alice", "age": 30}'
        assert_eq!(*res.results[0].get("recid").unwrap().as_int().unwrap(), 1);
        assert_eq!(*res.results[0].get("a").unwrap().as_int().unwrap(), 1);
        assert_eq!(res.results[0].get("b").unwrap().as_text().unwrap(), "Alpha");
        assert_eq!(
            res.results[0].get("c").unwrap().as_timestamp().unwrap(),
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

        // dbg!(&res.results[2]);

        assert_eq!(*res.results[2].get("recid").unwrap().as_int().unwrap(), 3);
        assert_eq!(*res.results[2].get("a").unwrap().as_int().unwrap(), 3);
        assert_eq!(
            res.results[2].get("b").unwrap().as_text().unwrap(),
            "Charlie"
        );
        assert_eq!(
            res.results[2].get("c").unwrap().as_timestamp().unwrap(),
            NaiveDateTime::parse_from_str("2024-01-03 10:30:00", "%Y-%m-%d %H:%M:%S").unwrap()
        );
        assert_eq!(res.results[2].get("d").unwrap().as_float().unwrap(), 30.25);
        assert_eq!(*res.results[2].get("e").unwrap().as_bool().unwrap(), true);

        // this was a param above
        assert_eq!(*res.results[3].get("a").unwrap().as_int().unwrap(), 100);
        // this was a param above too
        assert_eq!(res.results[3].get("d").unwrap().as_float().unwrap(), 100.75);
        // dbg!(&res.results[3]);
        // this was hard-coded
        assert_eq!(
            res.results[3].get("f").unwrap().as_blob().unwrap(),
            b"Blob11"
        );
        Ok::<(), Box<dyn std::error::Error>>(())
    })
}
