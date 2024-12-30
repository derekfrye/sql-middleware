use chrono::NaiveDateTime;
use regex::Regex;
use sqlx::{Connection, Executor};
use sqlx_middleware::convenience_items::{create_tables, MissingDbObjects};
use sqlx_middleware::db::{DatabaseSetupState, DatabaseType, Db, DbConfigAndPool};
use sqlx_middleware::model::{CheckType, CustomDbRow, DatabaseResult, QueryAndParams, RowValues};
use std::net::TcpStream;
use std::vec;
use std::{
    net::TcpListener,
    process::{Command, Stdio},
    thread,
    time::Duration,
};
use tokio::runtime::Runtime;

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
        .args(&[
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
        ])
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
                db_user, db_pass, port, db_name
            );

            loop {
                let podman_logs = Command::new("podman")
                    .args(&["logs", &container_id])
                    .output()
                    .expect("Failed to get logs from container");
                let re = Regex::new(r"listening on IPv6 address [^,]+, port 5432").unwrap();
                let podman_logs_str = String::from_utf8_lossy(&podman_logs.stderr);
                if re.is_match(&podman_logs_str) {
                    break;
                }
                thread::sleep(Duration::from_millis(100));
            }

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

        let postgres_dbconfig: DbConfigAndPool =
            DbConfigAndPool::new(cfg, DatabaseType::Postgres).await;
        let postgres_db = Db::new(postgres_dbconfig).unwrap();

        let pg_objs = vec![
            MissingDbObjects {
                missing_object: "test".to_string(),
            },
            MissingDbObjects {
                missing_object: "test_2".to_string(),
            },
        ];

        // create two test tables
        let pg_create_result = create_tables(&postgres_db, pg_objs, CheckType::Table, TABLE_DDL)
            .await
            .unwrap();

        if pg_create_result.db_last_exec_state == DatabaseSetupState::QueryError {
            eprintln!("Error: {}", pg_create_result.error_message.unwrap());
        }

        assert_eq!(
            pg_create_result.db_last_exec_state,
            DatabaseSetupState::QueryReturnedSuccessfully
        );
        assert_eq!(pg_create_result.return_result, String::default());

        const TABLE_DDL_SYNTAX_ERR: &[(&str, &str, &str, &str)] = &[(
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
        )];

        let pg_objs = vec![MissingDbObjects {
            missing_object: "testa".to_string(),
        }];

        // create a test table
        let pg_create_result = create_tables(
            &postgres_db,
            pg_objs,
            CheckType::Table,
            TABLE_DDL_SYNTAX_ERR,
        )
        .await
        .unwrap();

        assert_eq!(
            pg_create_result.db_last_exec_state,
            DatabaseSetupState::QueryError
        );
        assert_eq!(pg_create_result.return_result, String::default());

        let query = "DELETE FROM test;";
        let params: Vec<RowValues> = vec![];
        let _ = postgres_db
            .exec_general_query(
                vec![QueryAndParams {
                    query: query.to_string(),
                    params: params,
                }],
                false,
            )
            .await
            .unwrap();

        let query = "INSERT INTO test (espn_id, name, ins_ts) VALUES ($1, $2, $3)";
        let params: Vec<RowValues> = vec![
            RowValues::Int(123456),
            RowValues::Text("test name".to_string()),
            RowValues::Timestamp(
                NaiveDateTime::parse_from_str("2021-08-06 16:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            ),
        ];
        // params.push("test name".to_string());
        let x = postgres_db
            .exec_general_query(
                vec![QueryAndParams {
                    query: query.to_string(),
                    params: params,
                }],
                false,
            )
            .await
            .unwrap();

        if x.db_last_exec_state == DatabaseSetupState::QueryError {
            eprintln!("Error: {}", x.error_message.unwrap());
        }

        assert_eq!(
            x.db_last_exec_state,
            DatabaseSetupState::QueryReturnedSuccessfully
        );

        let query = "SELECT * FROM test";
        let params: Vec<RowValues> = vec![];
        let result = postgres_db
            .exec_general_query(
                vec![QueryAndParams {
                    query: query.to_string(),
                    params: params,
                }],
                true,
            )
            .await
            .unwrap();

        let expected_result = DatabaseResult {
            db_last_exec_state: DatabaseSetupState::QueryReturnedSuccessfully,
            return_result: vec![CustomDbRow {
                column_names: vec![
                    "event_id".to_string(),
                    "espn_id".to_string(),
                    "name".to_string(),
                    "ins_ts".to_string(),
                ],
                rows: vec![
                    RowValues::Int(1),
                    RowValues::Int(123456),
                    RowValues::Text("test name".to_string()),
                    RowValues::Timestamp(
                        NaiveDateTime::parse_from_str("2021-08-06 16:00:00", "%Y-%m-%d %H:%M:%S")
                            .unwrap(),
                    ),
                ],
            }],
            error_message: None,
            db_object_name: "exec_general_query".to_string(),
        };

        assert_eq!(
            result.db_last_exec_state,
            expected_result.db_last_exec_state
        );
        assert_eq!(result.error_message, expected_result.error_message);

        let cols_to_actually_check = vec!["espn_id", "name", "ins_ts"];

        for (index, row) in result.return_result.iter().enumerate() {
            let left: Vec<RowValues> = row.results[0]
                .column_names
                .iter()
                .zip(&row.results[0].rows) // Pair column names with corresponding row values
                .filter(|(col_name, _)| cols_to_actually_check.contains(&col_name.as_str()))
                .map(|(_, value)| value.clone()) // Collect the filtered row values
                .collect();

            // Get column names and row values from the expected result
            let right: Vec<RowValues> = expected_result.return_result[index]
                .column_names
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
                false,
            )
            .await
            .unwrap();

        assert_eq!(
            result.db_last_exec_state,
            DatabaseSetupState::QueryReturnedSuccessfully
        );

        let query = "DROP TABLE test_2;";
        let params: Vec<RowValues> = vec![];
        let result = postgres_db
            .exec_general_query(
                vec![QueryAndParams {
                    query: query.to_string(),
                    params: params,
                }],
                false,
            )
            .await
            .unwrap();

        assert_eq!(
            result.db_last_exec_state,
            DatabaseSetupState::QueryReturnedSuccessfully
        );

        // stop the container
        let _stop_cmd = Command::new("podman")
            .args(&["stop", &container_id])
            .output()
            .expect("Failed to stop container");
    });
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
        Ok(_) => true,   // If the connection succeeds, the port is in use
        Err(_) => false, // If connection fails, the port is available
    }
}
