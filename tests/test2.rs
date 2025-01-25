// use sqlx_middleware::convenience_items::{ create_tables, MissingDbObjects };
// use sqlx_middleware::db::{ QueryState, DatabaseType, Db, ConfigAndPool };
// use sqlx_middleware::model::{ CheckType, CustomDbRow, DatabaseResult, QueryAndParams, RowValues };
use chrono::NaiveDateTime;
// use sqlx::{ Connection, Executor };
use regex::Regex;
use sqlx_middleware::middleware::{
    ConfigAndPool, CustomDbRow, MiddlewarePool, MiddlewarePoolConnection, QueryAndParams, RowValues,
};
use sqlx_middleware::SqlMiddlewareDbError;
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
fn postgres_cr_and_del_tables() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Find a free TCP port (starting from start_port, increment if taken)
    let start_port = 9050;
    let port = find_available_port(start_port);
    // println!("Using port: {}", port);

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

    // 3. Wait 100ms to ensure Postgres inside the container is up; poll the DB until successful
    let mut success = false;
    let mut attempt = 0;

    let mut cfg = deadpool_postgres::Config::new();
    cfg.dbname = Some(db_name.to_string());
    cfg.host = Some("localhost".to_string());
    cfg.port = Some(port);
    cfg.user = Some(db_user.to_string());
    cfg.password = Some(db_pass.to_string());

    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let config_and_pool = ConfigAndPool::new_postgres(cfg.clone()).await?;
        let pool = config_and_pool.pool.get().await?;
        let conn = MiddlewarePool::get_connection(pool).await?;

        let pgconn = match conn {
            MiddlewarePoolConnection::Postgres(pgconn) => pgconn,
            MiddlewarePoolConnection::Sqlite(_) => {
                panic!("Only sqlite is supported");
            }
        };
        while !success && attempt < 10 {
            attempt += 1;
            // println!("Attempting to connect to Postgres. Attempt: {}", attempt);

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

            let res = pgconn.execute("SELECT 1", &[]).await?;
            if res == 1 {
                success = true;
                // println!("Successfully connected to Postgres!");
            } else {
                // println!("Failed to connect to Postgres. Retrying...");
                thread::sleep(Duration::from_millis(100));
            }
        }

        Ok::<(), Box<dyn std::error::Error>>(())
    })?;

    let rt = Runtime::new().unwrap();
    Ok(rt.block_on(async {
        // env::var("DB_USER") = Ok("postgres".to_string());

        let stmt = "CREATE TABLE IF NOT EXISTS -- drop table event cascade
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
        let conn = MiddlewarePool::get_connection(pool).await?;
        let mut pgconn = match conn {
            MiddlewarePoolConnection::Postgres(pgconn) => pgconn,
            MiddlewarePoolConnection::Sqlite(_) => {
                panic!("Only sqlite is supported");
            }
        };

        {
            let tx = pgconn.transaction().await?;
            let result_set = {
                let rs = tx.batch_execute(stmt).await?;
                rs
            };
            tx.commit().await?;
            Ok::<_, SqlMiddlewareDbError>(result_set)
        }?;

        let query = "DELETE FROM test;";
        {
            let tx = pgconn.transaction().await?;
            let result_set = {
                let rs = tx.batch_execute(query).await?;
                rs
            };
            tx.commit().await?;
            Ok::<_, SqlMiddlewareDbError>(result_set)
        }?;

        let query_and_params = QueryAndParams {
            query: "INSERT INTO test (espn_id, name, ins_ts) VALUES ($1, $2, $3)".to_string(),
            params: vec![
                RowValues::Int(123456),
                RowValues::Text("test name".to_string()),
                RowValues::Timestamp(
                    NaiveDateTime::parse_from_str("2021-08-06 16:00:00", "%Y-%m-%d %H:%M:%S")
                        .unwrap(),
                ),
            ],
            is_read_only: false,
        };

        {
            let converted_params =
                sqlx_middleware::PostgresParams::convert(&query_and_params.params)?;
            let tx = pgconn.transaction().await?;
            tx.prepare(query_and_params.query.as_str()).await?;
            let result_set = {
                let rs = tx
                    .execute(query_and_params.query.as_str(), &converted_params.as_refs())
                    .await?;

                rs
            };
            tx.commit().await?;
            Ok::<_, SqlMiddlewareDbError>(result_set)
        }?;

        let query = "select * FROM test;";
        let result = {
            let tx = pgconn.transaction().await?;
            let stmt = tx.prepare(query).await?;
            let result_set = {
                let rs = sqlx_middleware::postgres_build_result_set(&stmt, &[], &tx).await?;
                rs
            };
            tx.commit().await?;
            Ok::<_, SqlMiddlewareDbError>(result_set)
        }?;

        let expected_result = vec![CustomDbRow {
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
        }];

        let cols_to_actually_check = vec!["espn_id", "name", "ins_ts"];

        for (index, row) in result.results.iter().enumerate() {
            let left: Vec<RowValues> = row
                .column_names
                .iter()
                .zip(&row.rows) // Pair column names with corresponding row values
                .filter(|(col_name, _)| cols_to_actually_check.contains(&col_name.as_str()))
                .map(|(_, value)| value.clone()) // Collect the filtered row values
                .collect();

            // Get column names and row values from the expected result
            let right: Vec<RowValues> = expected_result[index]
                .column_names
                .iter()
                .zip(&expected_result[index].rows) // Pair column names with corresponding row values
                .filter(|(col_name, _)| cols_to_actually_check.contains(&col_name.as_str()))
                .map(|(_, value)| value.clone()) // Collect the filtered row values
                .collect();

            assert_eq!(left, right);
        }

        let query = "DROP TABLE test;
        DROP TABLE test_2;";
        {
            let tx = pgconn.transaction().await?;
            let result_set = {
                let rs = tx.batch_execute(query).await?;
                rs
            };
            tx.commit().await?;
            Ok::<_, SqlMiddlewareDbError>(result_set)
        }?;

        // stop the container
        let _stop_cmd = Command::new("podman")
            .args(&["stop", &container_id])
            .output()
            .expect("Failed to stop container");
        Ok::<(), Box<dyn std::error::Error>>(())
    })?)
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
