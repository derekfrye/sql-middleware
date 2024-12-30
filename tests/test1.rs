use chrono::{NaiveDateTime, Utc};
use serde_json::json;
use sqlx_middleware::convenience_items::{create_tables, MissingDbObjects};
use sqlx_middleware::db::{DatabaseSetupState, DatabaseType, Db, DbConfigAndPool};
use sqlx_middleware::model::{CheckType, QueryAndParams, RowValues};
use std::vec;
use tokio::runtime::Runtime;

#[test]
fn sqlite_rusty_golf_test() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let mut cfg = deadpool_postgres::Config::new();
        cfg.dbname = Some(":memory:".to_string());

        let sqlite_configandpool = DbConfigAndPool::new(cfg, DatabaseType::Sqlite).await;
        let sql_db = Db::new(sqlite_configandpool.clone()).unwrap();

        let tables = vec![
            "event",
            "golfstatistic",
            "player",
            "golfuser",
            "event_user_player",
            "eup_statistic"
        ];
        let ddl = vec![
            include_str!("../tests/sqlite/00_event.sql"),
            include_str!("../tests/sqlite/01_golfstatistic.sql"),
            include_str!("../tests/sqlite/02_player.sql"),
            include_str!("../tests/sqlite/03_golfuser.sql"),
            include_str!("../tests/sqlite/04_event_user_player.sql"),
            include_str!("../tests/sqlite/05_eup_statistic.sql")
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

        let create_result = create_tables(
            &sql_db,
            missing_objs,
            CheckType::Table,
            &table_ddl
                .iter()
                .map(|(a, b, c, d)| (**a, *b, *c, *d))
                .collect::<Vec<_>>()
        ).await.unwrap();

        if create_result.db_last_exec_state == DatabaseSetupState::QueryError {
            eprintln!("Error: {}", create_result.error_message.unwrap());
        }
        assert_eq!(create_result.db_last_exec_state, DatabaseSetupState::QueryReturnedSuccessfully);
        assert_eq!(create_result.return_result, String::default());

        let setup_queries = include_str!("../tests/sqlite/test1_setup.sql");
        let query_and_params = QueryAndParams {
            query: setup_queries.to_string(),
            params: vec![],
        };
        let res = sql_db.exec_general_query(vec![query_and_params], false).await.unwrap();

        assert_eq!(res.db_last_exec_state, DatabaseSetupState::QueryReturnedSuccessfully);

        let qry = "SELECT DATE(?)||'asdf' as dt;";
        let param = "now";
        let query_and_params = QueryAndParams {
            query: qry.to_string(),
            params: vec![RowValues::Text(param.to_string())],
        };
        let res = sql_db.exec_general_query(vec![query_and_params], true).await.unwrap();
        assert_eq!(res.db_last_exec_state, DatabaseSetupState::QueryReturnedSuccessfully);
        assert_eq!(res.return_result.len(), 1);

        let todays_date_computed = Utc::now().date_naive().format("%Y-%m-%d").to_string();
        let todays_date_plus_a_str = todays_date_computed.to_string() + "asdf";
        assert_eq!(
            res.return_result[0].results[0].get("dt").unwrap().as_text().unwrap(),
            todays_date_plus_a_str
        );

        // now ck event_user_player has three entries for player1
        // lets use a param
        let qry =
            "SELECT count(*) as cnt FROM event_user_player WHERE user_id = (select user_id from golfuser where name = ?);";
        // let params = vec![1];
        let param = "Player1";
        let query_and_params = QueryAndParams {
            query: qry.to_string(),
            params: vec![RowValues::Text(param.to_string())],
        };
        let res1 = sql_db.exec_general_query(vec![query_and_params], true).await;

        if res1.is_err() {
            eprintln!("Test Error: {}", &res1.err().unwrap());

            assert!(false);
        } else {
            let res = res1.unwrap();
            if res.db_last_exec_state != DatabaseSetupState::QueryReturnedSuccessfully {
                eprintln!("Error: {}", res.error_message.unwrap());
            }
            assert_eq!(res.db_last_exec_state, DatabaseSetupState::QueryReturnedSuccessfully);

            let count = res.return_result[0].results[0].get("cnt").unwrap();

            // print the type of the var
            println!("count: {:?}", count);

            let count1 = count.as_int().unwrap();
            // let cnt = count.
            assert_eq!(*count1, 3);
        }
    })
}

#[test]
fn sqlite_mutltiple_column_test() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let mut cfg = deadpool_postgres::Config::new();
        cfg.dbname = Some(":memory:".to_string());

        let sqlite_configandpool = DbConfigAndPool::new(cfg, DatabaseType::Sqlite).await;
        let sql_db = Db::new(sqlite_configandpool.clone()).unwrap();

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

        let create_result = create_tables(
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
        if create_result.db_last_exec_state == DatabaseSetupState::QueryError {
            eprintln!("Error: {}", create_result.error_message.unwrap());
        }
        assert_eq!(
            create_result.db_last_exec_state,
            DatabaseSetupState::QueryReturnedSuccessfully
        );
        assert_eq!(create_result.return_result, String::default());

        let setup_queries = include_str!("../tests/sqlite/test2_setup.sql");
        let query_and_params = QueryAndParams {
            query: setup_queries.to_string(),
            params: vec![],
        };
        let res = sql_db
            .exec_general_query(vec![query_and_params], false)
            .await
            .unwrap();

        assert_eq!(
            res.db_last_exec_state,
            DatabaseSetupState::QueryReturnedSuccessfully
        );

        let qry = "SELECT * from test where recid in (?,?, ?);";
        // let param = [RowValues::Int(1), RowValues::Int(2), RowValues::Int(3)];
        let param = vec![RowValues::Int(1)];
        let query_and_params = QueryAndParams {
            query: qry.to_string(),
            params: param.to_vec(),
        };
        let res = sql_db
            .exec_general_query(vec![query_and_params], true)
            .await
            .unwrap();
        assert_eq!(
            res.db_last_exec_state,
            DatabaseSetupState::QueryReturnedSuccessfully
        );
        // we expect 1 result set
        assert_eq!(res.return_result.len(), 1);
        // we expect 1 row
        assert_eq!(res.return_result[0].results.len(), 1);

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
            *res.return_result[0].results[0]
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
