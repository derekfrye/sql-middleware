use chrono::NaiveDateTime;
use serde_json::json;
use sqlx_middleware::convenience_items::{ create_tables3, MissingDbObjects};

use sqlx_middleware::db_model::{
    ConfigAndPool as ConfigAndPool2, DatabaseType as DatabaseType2, Db as Db2,
    QueryAndParams as QueryAndParams2, QueryState as QueryState2, RowValues as RowValues2,
};
use sqlx_middleware::model::CheckType;
use std::vec;
use tokio::runtime::Runtime;

#[test]
fn sqlite_mutltiple_column_test_db2() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let mut cfg = deadpool_postgres::Config::new();
        cfg.dbname = Some("file::memory:?cache=shared".to_string());
        // cfg.dbname = Some("xxx".to_string());
        let sqlite_configandpool = ConfigAndPool2::new_auto(&cfg, DatabaseType2::Sqlite).unwrap();
        let sql_db = Db2::new(sqlite_configandpool).unwrap();

        let tables = vec!["test"];
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

        // fixme, the conv item function shouldnt require a 4-len str array, that's silly
        let mut table_ddl = vec![];
        for (i, table) in tables.iter().enumerate() {
            table_ddl.push((table, ddl[i], "", ""));
        }

        let mut missing_objs: Vec<MissingDbObjects> = vec![];
        for table in table_ddl.iter() {
            missing_objs.push(MissingDbObjects {
                missing_object: table.0.to_string(),
            });
        }

        let create_result = create_tables3(
            &sql_db,
            missing_objs,
            CheckType::Table,
            &table_ddl
                .iter()
                .map(|(a, b, c, d)| (**a, *b, *c, *d))
                .collect::<Vec<_>>(),
        )
        .await
        .unwrap();

        // fixme, should just come back with an error rather than requiring caller to check results of last_exec_state
        if create_result.db_last_exec_state != QueryState2::QueryError {
            if let Some(error_message) = create_result.error_message.clone() {
                eprintln!("Error: {}", error_message);
            }
        }
        // dbg!("create_result: {:#?}", &create_result);
        assert_eq!(
            create_result.db_last_exec_state,
            QueryState2::QueryReturnedSuccessfully
        );
        assert_eq!(create_result.return_result, String::default());

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
            vec![RowValues2::Int(1)],
            vec![RowValues2::Int(2)],
            vec![RowValues2::Int(3)],
            vec![RowValues2::Int(4)],
            vec![RowValues2::Int(5)],
            vec![RowValues2::Int(6)],
            vec![RowValues2::Int(7)],
            vec![RowValues2::Int(8)],
            vec![RowValues2::Int(9)],
            vec![
                RowValues2::Int(10),
                RowValues2::Text("Juliet".to_string()),
                RowValues2::Float(100.75),
            ],
        ];
        let query_and_params_vec = setup_queries
            .iter()
            .zip(params.iter())
            .map(|(a, b)| QueryAndParams2 {
                query: a.to_string(),
                params: b.to_vec(),
                is_read_only: false,
            })
            .collect::<Vec<_>>();
        let res = sql_db
            .exec_general_query(query_and_params_vec, false)
            .await
            .unwrap();

        assert_eq!(
            res.db_last_exec_state,
            QueryState2::QueryReturnedSuccessfully
        );

        let qry = "SELECT * from test where recid in ( ?1, ?2, ?3);";
        let param = [RowValues2::Int(1), RowValues2::Int(2), RowValues2::Int(3)];
        // let param = [RowValues2::Int(1)];
        // let param = vec![RowValues::Int(1)];
        let query_and_params = QueryAndParams2 {
            query: qry.to_string(),
            params: param.to_vec(),
            is_read_only: true,
        };
        let res = sql_db
            .exec_general_query(vec![query_and_params], true)
            .await
            .unwrap();
        assert_eq!(
            res.db_last_exec_state,
            QueryState2::QueryReturnedSuccessfully
        );
        // we expect 1 result set
        assert_eq!(res.return_result.len(), 1);
        // we expect 3 rows
        assert_eq!(res.return_result[0].results.len(), 3);

        // dbg!(&res.return_result[0].results[0]);

        // row 1 should decode as: 1, 'Alpha', '2024-01-01 08:00:01', 10.5, 1, X'426C6F623132', '{"name": "Alice", "age": 30}'
        assert_eq!(
            *res.return_result[0].results[0]
                .get("recid")
                .unwrap()
                .as_int()
                .unwrap(),
            1
        );
        assert_eq!(
            *res.return_result[0].results[0]
                .get("a")
                .unwrap()
                .as_int()
                .unwrap(),
            1
        );
        assert_eq!(
            res.return_result[0].results[0]
                .get("b")
                .unwrap()
                .as_text()
                .unwrap(),
            "Alpha"
        );
        assert_eq!(
            res.return_result[0].results[0]
                .get("c")
                .unwrap()
                .as_timestamp()
                .unwrap(),
            NaiveDateTime::parse_from_str("2024-01-01 08:00:01", "%Y-%m-%d %H:%M:%S").unwrap()
        );
        assert_eq!(
            res.return_result[0].results[0]
                .get("d")
                .unwrap()
                .as_float()
                .unwrap(),
            10.5
        );
        assert_eq!(
            *res.return_result[0].results[0]
                .get("e")
                .unwrap()
                .as_bool()
                .unwrap(),
            true
        );
        assert_eq!(
            res.return_result[0].results[0]
                .get("f")
                .unwrap()
                .as_blob()
                .unwrap(),
            b"Blob12"
        );
        // troubleshoot this around step 3 of db.rs
        assert_eq!(
            json!(res.return_result[0].results[0]
                .get("g")
                .unwrap()
                .as_text()
                .unwrap()),
            json!(r#"{"name": "Alice", "age": 30}"#)
        );
    })
}
