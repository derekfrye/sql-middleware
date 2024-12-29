use crate::db::{ DatabaseSetupState, Db };
use crate::model::{ CheckType, DatabaseItem, DatabaseResult, QueryAndParams, RowValues };
use function_name::named;
use serde::Deserialize;
// use sqlx::query;

/// we need this to deserialize the json, even though it seems trivial, it's needed for data validation
#[derive(Deserialize, Debug, Clone)]
pub struct MissingDbObjects {
    pub missing_object: String,
}

/// Check if tables or constraints are setup. Expects a particular query result format.
/// This format, from rusty-golf 0x_tables_exist.sql, expects this result format from they query:
///```text
///             tbl        | exists
///     -------------------+--------
///      eup_statistic     | f
///      event             | t
///      event_user_player | t
///      golfstatistic     | t
///      golf_user         | t
///      player            | t
///```
pub async fn test_is_db_setup(
    db: &Db,
    check_type: &CheckType,
    query: &str,
    ddl: &[DatabaseItem]
) -> Result<Vec<DatabaseResult<String>>, Box<dyn std::error::Error>> {
    let mut dbresults = vec![];

    // let query = include_str!("../admin/model/sql/schema/0x_tables_exist.sql");
    let query_and_params = QueryAndParams {
        query: query.to_string(),
        params: vec![],
    };
    let result = db.exec_general_query(vec![query_and_params], true).await;

    let missing_tables = match result {
        Ok(r) => {
            if r.db_last_exec_state == DatabaseSetupState::QueryReturnedSuccessfully {
                r.return_result[0].results.clone()
            } else {
                let mut dbresult: DatabaseResult<String> = DatabaseResult::<String>::default();
                dbresult.db_last_exec_state = r.db_last_exec_state;
                dbresult.error_message = r.error_message;
                return Ok(vec![dbresult]);
            }
        }
        Err(e) => {
            let emessage = format!("Failed in {}, {}: {}", std::file!(), std::line!(), e);
            let mut dbresult: DatabaseResult<String> = DatabaseResult::<String>::default();
            dbresult.error_message = Some(emessage);
            dbresults.push(dbresult);
            return Ok(dbresults);
        }
    };

    // may have to declare as Vec<String>
    let zz: Vec<_> = missing_tables
        .iter()
        .filter_map(|row| {
            let exists_index = row.column_names.iter().position(|col| col == "exists")?;
            let tbl_index = row.column_names.iter().position(|col| col == "tbl")?;

            // Check if the "exists" column value is `Value::Bool(true)` or `Value::Text("t")`
            match &row.rows[exists_index] {
                RowValues::Bool(true) =>
                    match &row.rows[tbl_index] {
                        RowValues::Text(tbl_name) => Some(tbl_name.clone()),
                        _ => None,
                    }
                RowValues::Text(value) if value == "t" =>
                    match &row.rows[tbl_index] {
                        RowValues::Text(tbl_name) => Some(tbl_name.clone()),
                        _ => None,
                    }
                _ => None,
            }
        })
        .collect();

    fn local_fn_get_iter<'a>(
        ddl: &'a [DatabaseItem],
        check_type: &'a CheckType
    ) -> impl Iterator<Item = &'a str> {
        ddl.iter().filter_map(move |item| {
            match (check_type, item) {
                (CheckType::Table, DatabaseItem::Table(table)) => Some(table.table_name.as_str()),
                (CheckType::Constraint, DatabaseItem::Constraint(constraint)) => {
                    Some(constraint.constraint_name.as_str())
                }
                _ => None,
            }
        })
    }

    for table in local_fn_get_iter(ddl, check_type) {
        let mut dbresult: DatabaseResult<String> = DatabaseResult::<String>::default();
        dbresult.db_object_name = table.to_string();

        if zz.iter().any(|x| x == table) {
            dbresult.db_last_exec_state = DatabaseSetupState::QueryReturnedSuccessfully;
        } else {
            dbresult.db_last_exec_state = DatabaseSetupState::MissingRelations;
        }

        dbresults.push(dbresult);
    }

    Ok(dbresults)
}

#[named]
pub async fn create_tables(
    db: &Db,
    tables: Vec<MissingDbObjects>,
    check_type: CheckType,
    ddl_for_validation: &[(&str, &str, &str, &str)]
) -> Result<DatabaseResult<String>, Box<dyn std::error::Error>> {
    let mut return_result: DatabaseResult<String> = DatabaseResult::<String>::default();
    return_result.db_object_name = function_name!().to_string();

    let entire_create_stms = if check_type == CheckType::Table {
        ddl_for_validation
            .iter()
            .filter(|x| tables.iter().any(|y| y.missing_object == x.0))
            .map(|af| af.1)
            // .into_iter()
            .collect::<Vec<&str>>()
        // .join("")
        // .flatten()
    } else {
        ddl_for_validation
            .iter()
            .filter(|x| tables.iter().any(|y| y.missing_object == x.2))
            .map(|af| af.3)
            // .collect::<Vec<&str>>()
            // .flatten()
            .collect::<Vec<&str>>()
        // .join("")
    };

    let result = db.exec_general_query(
        entire_create_stms
            .iter()
            .map(|x| QueryAndParams {
                query: x.to_string(),
                params: vec![],
            })
            .collect(),
        false
    ).await;

    // let query_and_params = QueryAndParams {
    //     query: entire_create_stms,
    //     params: vec![],
    // };
    // let result = self.exec_general_query(vec![query_and_params], false).await;

    let mut dbresult: DatabaseResult<String> = DatabaseResult::<String>::default();

    match result {
        Ok(r) => {
            dbresult.db_last_exec_state = r.db_last_exec_state;
            dbresult.error_message = r.error_message;
            // r.return_result
        }
        Err(e) => {
            let emessage = format!("Failed in {}, {}: {}", std::file!(), std::line!(), e);
            dbresult.error_message = Some(emessage);
        }
    }
    Ok(dbresult)
}

#[cfg(test)]
mod tests {
    use crate::db::{ DatabaseType, DbConfigAndPool };
    use crate::model::CustomDbRow;
    use chrono::{ NaiveDateTime, Utc };
    use function_name::named;
    use sqlx::{ Connection, Executor };
    use std::net::TcpStream;
    use std::vec;
    // use sqlx::query;
    use std::{ net::TcpListener, process::{ Command, Stdio }, thread, time::Duration };
    use tokio::runtime::Runtime;

    use super::*;

    #[named]
    async fn create_tables(
        sself: &Db,
        tables: Vec<MissingDbObjects>,
        check_type: CheckType,
        ddl_for_validation: &[(&str, &str, &str, &str)]
    ) -> Result<DatabaseResult<String>, Box<dyn std::error::Error>> {
        let mut return_result: DatabaseResult<String> = DatabaseResult::<String>::default();
        return_result.db_object_name = function_name!().to_string();

        let entire_create_stms = if check_type == CheckType::Table {
            ddl_for_validation
                .iter()
                .filter(|x| tables.iter().any(|y| y.missing_object == x.0))
                .map(|af| af.1)
                // .into_iter()
                .collect::<Vec<&str>>()
            // .join("")
            // .flatten()
        } else {
            ddl_for_validation
                .iter()
                .filter(|x| tables.iter().any(|y| y.missing_object == x.2))
                .map(|af| af.3)
                // .collect::<Vec<&str>>()
                // .flatten()
                .collect::<Vec<&str>>()
            // .join("")
        };

        let result = sself.exec_general_query(
            entire_create_stms
                .iter()
                .map(|x| QueryAndParams {
                    query: x.to_string(),
                    params: vec![],
                })
                .collect(),
            false
        ).await;

        // let query_and_params = QueryAndParams {
        //     query: entire_create_stms,
        //     params: vec![],
        // };
        // let result = self.exec_general_query(vec![query_and_params], false).await;

        let mut dbresult: DatabaseResult<String> = DatabaseResult::<String>::default();

        match result {
            Ok(r) => {
                dbresult.db_last_exec_state = r.db_last_exec_state;
                dbresult.error_message = r.error_message;
                // r.return_result
            }
            Err(e) => {
                let emessage = format!("Failed in {}, {}: {}", std::file!(), std::line!(), e);
                dbresult.error_message = Some(emessage);
            }
        }
        Ok(dbresult)
    }

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
            assert_eq!(
                create_result.db_last_exec_state,
                DatabaseSetupState::QueryReturnedSuccessfully
            );
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
                );"
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

            // fixme, should just come back with an error rather than requiring caller to check results of last_exec_state
            if create_result.db_last_exec_state == DatabaseSetupState::QueryError {
                eprintln!("Error: {}", create_result.error_message.unwrap());
            }
            assert_eq!(
                create_result.db_last_exec_state,
                DatabaseSetupState::QueryReturnedSuccessfully
            );
            assert_eq!(create_result.return_result, String::default());

            // here
            let setup_queries = include_str!("../tests/sqlite/test2_setup.sql");
            let query_and_params = QueryAndParams {
                query: setup_queries.to_string(),
                params: vec![],
            };
            let res = sql_db.exec_general_query(vec![query_and_params], false).await.unwrap();

            assert_eq!(res.db_last_exec_state, DatabaseSetupState::QueryReturnedSuccessfully);

            let qry = "SELECT * from test where recid in (?,?, ?);";
            let param = [RowValues::Int(1), RowValues::Int(2), RowValues::Int(3)];
            let query_and_params = QueryAndParams {
                query: qry.to_string(),
                params: param.to_vec(),
            };
            let res = sql_db.exec_general_query(vec![query_and_params], true).await.unwrap();
            assert_eq!(res.db_last_exec_state, DatabaseSetupState::QueryReturnedSuccessfully);
            // we expect 1 result set
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

    // A small helper function to find an available port by trying to bind
    // starting from `start_port`, then incrementing until a bind succeeds.
    fn find_available_port(start_port: u16) -> u16 {
        let mut port = start_port;
        loop {
            if TcpListener::bind(("127.0.0.1", port)).is_ok() && !is_port_in_use(port) {
                return port;
            }
            port += 1;
        }
    }

    fn is_port_in_use(port: u16) -> bool {
        match TcpStream::connect(("127.0.0.1", port)) {
            Ok(_) => true, // If the connection succeeds, the port is in use
            Err(_) => false, // If connection fails, the port is available
        }
    }

    #[test]
    fn postgres_cr_and_del_tables() {
        // 1. Find a free TCP port (starting from 5432, increment if taken)
        let start_port = 9050;
        let port = find_available_port(start_port);
        println!("Using port: {}", port);

        let db_user = "test_user";
        // don't use @ or # in here, it fails
        // https://github.com/launchbadge/sqlx/issues/1624
        let db_pass = "test_passwordx(!323341";
        let db_name = "test_db";

        // 2. Start the Podman container
        //    In this example, we're running in detached mode (-d) and removing
        //    automatically when the container stops (--rm).
        let output = Command::new("podman")
            .args(
                &[
                    "run",
                    "--rm",
                    "-d",
                    "-p",
                    &format!("{}:5432", port),
                    "-e",
                    &format!("POSTGRES_USER={}", db_user),
                    "-e",
                    &format!("POSTGRES_PASSWORD={}", db_pass),
                    "-e",
                    &format!("POSTGRES_DB={}", db_name),
                    "postgres:latest",
                ]
            )
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .expect("Failed to start Podman Postgres container");

        // Ensure Podman started successfully
        assert!(
            output.status.success(),
            "Failed to run podman command: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        // Grab the container ID from stdout
        let container_id = String::from_utf8_lossy(&output.stdout).trim().to_string();
        // println!("Started container ID: {}", container_id);

        // 3. Wait a few seconds to ensure Postgres inside the container is up
        //   poll the DB in a loop until successful)
        let mut success = false;
        let mut attempt = 0;
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            while !success && attempt < 10 {
                attempt += 1;
                // println!("Attempting to connect to Postgres. Attempt: {}", attempt);
                let conn_str = format!(
                    "postgres://{}:{}@localhost:{}/{}",
                    db_user,
                    db_pass,
                    port,
                    db_name
                );
                thread::sleep(Duration::from_secs(4));
                let mut conn = sqlx::PgConnection::connect(&conn_str).await.unwrap();
                let res = conn.execute("SELECT 1").await.unwrap();
                if res.rows_affected() == 1 {
                    success = true;
                    // println!("Successfully connected to Postgres!");
                } else {
                    // println!("Failed to connect to Postgres. Retrying...");
                    thread::sleep(Duration::from_secs(1));
                }
            }
        });

        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            // env::var("DB_USER") = Ok("postgres".to_string());

            const TABLE_DDL: &[(&str, &str, &str, &str)] = &[
                (
                    "test",
                    "CREATE TABLE IF NOT EXISTS -- drop table event cascade
                    test (
                    event_id BIGSERIAL NOT NULL PRIMARY KEY,
                    espn_id BIGINT NOT NULL,
                    name TEXT NOT NULL,
                    ins_ts TIMESTAMP NOT NULL DEFAULT now()
                    );",
                    "",
                    "",
                ),
                (
                    "test_2",
                    "CREATE TABLE IF NOT EXISTS -- drop table event cascade
                    test_2 (
                    event_id BIGSERIAL NOT NULL PRIMARY KEY,
                    espn_id BIGINT NOT NULL,
                    name TEXT NOT NULL,
                    ins_ts TIMESTAMP NOT NULL DEFAULT now()
                    );",
                    "",
                    "",
                ),
            ];

            let mut cfg = deadpool_postgres::Config::new();
            cfg.dbname = Some(db_name.to_string());
            cfg.host = Some("localhost".to_string());
            cfg.port = Some(port);
            cfg.user = Some(db_user.to_string());
            cfg.password = Some(db_pass.to_string());

            let postgres_dbconfig: DbConfigAndPool = DbConfigAndPool::new(
                cfg,
                DatabaseType::Postgres
            ).await;
            let postgres_db = Db::new(postgres_dbconfig).unwrap();

            let pg_objs = vec![
                MissingDbObjects {
                    missing_object: "test".to_string(),
                },
                MissingDbObjects {
                    missing_object: "test_2".to_string(),
                }
            ];

            // create two test tables
            let pg_create_result = create_tables(
                &postgres_db,
                pg_objs,
                CheckType::Table,
                TABLE_DDL
            ).await.unwrap();

            if pg_create_result.db_last_exec_state == DatabaseSetupState::QueryError {
                eprintln!("Error: {}", pg_create_result.error_message.unwrap());
            }

            assert_eq!(
                pg_create_result.db_last_exec_state,
                DatabaseSetupState::QueryReturnedSuccessfully
            );
            assert_eq!(pg_create_result.return_result, String::default());

            const TABLE_DDL_SYNTAX_ERR: &[(&str, &str, &str, &str)] = &[
                (
                    "testa",
                    "CREATE TABLEXXXXXXXX IF NOT EXISTS -- drop table event cascade
                    test (
                    event_id BIGSERIAL NOT NULL PRIMARY KEY,
                    espn_id BIGINT NOT NULL,
                    name TEXT NOT NULL,
                    ins_ts TIMESTAMP NOT NULL DEFAULT now()
                    );",
                    "",
                    "",
                ),
            ];

            let pg_objs = vec![MissingDbObjects {
                missing_object: "testa".to_string(),
            }];

            // create a test table
            let pg_create_result = create_tables(
                &postgres_db,
                pg_objs,
                CheckType::Table,
                TABLE_DDL_SYNTAX_ERR
            ).await.unwrap();

            assert_eq!(pg_create_result.db_last_exec_state, DatabaseSetupState::QueryError);
            assert_eq!(pg_create_result.return_result, String::default());

            let query = "DELETE FROM test;";
            let params: Vec<RowValues> = vec![];
            let _ = postgres_db
                .exec_general_query(
                    vec![QueryAndParams {
                        query: query.to_string(),
                        params: params,
                    }],
                    false
                ).await
                .unwrap();

            let query = "INSERT INTO test (espn_id, name, ins_ts) VALUES ($1, $2, $3)";
            let params: Vec<RowValues> = vec![
                RowValues::Int(123456),
                RowValues::Text("test name".to_string()),
                RowValues::Timestamp(
                    NaiveDateTime::parse_from_str(
                        "2021-08-06 16:00:00",
                        "%Y-%m-%d %H:%M:%S"
                    ).unwrap()
                )
            ];
            // params.push("test name".to_string());
            let x = postgres_db
                .exec_general_query(
                    vec![QueryAndParams {
                        query: query.to_string(),
                        params: params,
                    }],
                    false
                ).await
                .unwrap();

            if x.db_last_exec_state == DatabaseSetupState::QueryError {
                eprintln!("Error: {}", x.error_message.unwrap());
            }

            assert_eq!(x.db_last_exec_state, DatabaseSetupState::QueryReturnedSuccessfully);

            let query = "SELECT * FROM test";
            let params: Vec<RowValues> = vec![];
            let result = postgres_db
                .exec_general_query(
                    vec![QueryAndParams {
                        query: query.to_string(),
                        params: params,
                    }],
                    true
                ).await
                .unwrap();

            let expected_result = DatabaseResult {
                db_last_exec_state: DatabaseSetupState::QueryReturnedSuccessfully,
                return_result: vec![CustomDbRow {
                    column_names: vec![
                        "event_id".to_string(),
                        "espn_id".to_string(),
                        "name".to_string(),
                        "ins_ts".to_string()
                    ],
                    rows: vec![
                        RowValues::Int(1),
                        RowValues::Int(123456),
                        RowValues::Text("test name".to_string()),
                        RowValues::Timestamp(
                            NaiveDateTime::parse_from_str(
                                "2021-08-06 16:00:00",
                                "%Y-%m-%d %H:%M:%S"
                            ).unwrap()
                        )
                    ],
                }],
                error_message: None,
                db_object_name: "exec_general_query".to_string(),
            };

            assert_eq!(result.db_last_exec_state, expected_result.db_last_exec_state);
            assert_eq!(result.error_message, expected_result.error_message);

            let cols_to_actually_check = vec!["espn_id", "name", "ins_ts"];

            for (index, row) in result.return_result.iter().enumerate() {
                let left: Vec<RowValues> = row.results[0].column_names
                    .iter()
                    .zip(&row.results[0].rows) // Pair column names with corresponding row values
                    .filter(|(col_name, _)| cols_to_actually_check.contains(&col_name.as_str()))
                    .map(|(_, value)| value.clone()) // Collect the filtered row values
                    .collect();

                // Get column names and row values from the expected result
                let right: Vec<RowValues> = expected_result.return_result[index].column_names
                    .iter()
                    .zip(&expected_result.return_result[index].rows) // Pair column names with corresponding row values
                    .filter(|(col_name, _)| cols_to_actually_check.contains(&col_name.as_str()))
                    .map(|(_, value)| value.clone()) // Collect the filtered row values
                    .collect();

                assert_eq!(left, right);
            }

            let query = "DROP TABLE test;";
            let params: Vec<RowValues> = vec![];
            let result = postgres_db
                .exec_general_query(
                    vec![QueryAndParams {
                        query: query.to_string(),
                        params: params,
                    }],
                    false
                ).await
                .unwrap();

            assert_eq!(result.db_last_exec_state, DatabaseSetupState::QueryReturnedSuccessfully);

            let query = "DROP TABLE test_2;";
            let params: Vec<RowValues> = vec![];
            let result = postgres_db
                .exec_general_query(
                    vec![QueryAndParams {
                        query: query.to_string(),
                        params: params,
                    }],
                    false
                ).await
                .unwrap();

            assert_eq!(result.db_last_exec_state, DatabaseSetupState::QueryReturnedSuccessfully);

            // stop the container
            let _stop_cmd = Command::new("podman")
                .args(&["stop", &container_id])
                .output()
                .expect("Failed to stop container");
        });
    }
}
