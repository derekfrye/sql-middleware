use regex::Regex;
use crate::middleware::{ConfigAndPool, MiddlewarePool, MiddlewarePoolConnection};
use std::net::TcpStream;
use std::{
    net::TcpListener,
    process::{Command, Stdio},
    thread,
    time::Duration,
};
use tokio::runtime::Runtime;

/// Test utilities for PostgreSQL testing and benchmarking
pub mod testing_postgres {
    use super::*;

    /// Represents a running PostgreSQL container
    pub struct PostgresContainer {
        pub container_id: String,
        pub port: u16,
    }

    /// Set up a PostgreSQL container for testing or benchmarking
    pub fn setup_postgres_container(
        cfg: &deadpool_postgres::Config,
    ) -> Result<PostgresContainer, Box<dyn std::error::Error>> {
        let mut cfg = cfg.clone();
        // 1. Find a free TCP port (starting from start_port, increment if taken)
        let start_port = 9050;
        let port = find_available_port(start_port);
        cfg.port = Some(port);
        // println!("Using port: {}", port);

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
                &format!("POSTGRES_USER={}", &cfg.user.as_ref().unwrap()),
                "-e",
                &format!("POSTGRES_PASSWORD={}", cfg.password.as_ref().unwrap()),
                "-e",
                &format!("POSTGRES_DB={}", cfg.dbname.as_ref().unwrap()),
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

        // let mut cfg = deadpool_postgres::Config::new();
        // cfg.dbname = Some(db_name.to_string());
        // cfg.host = Some("localhost".to_string());
        // cfg.port = Some(port);
        // cfg.user = Some(db_user.to_string());
        // cfg.password = Some(db_pass.to_string());

        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            // let cfg = cfg.clone();
            let config_and_pool = ConfigAndPool::new_postgres(cfg.clone()).await?;
            let pool = config_and_pool.pool.get().await?;
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

                let conn = MiddlewarePool::get_connection(&pool).await;
                if conn.is_ok() {
                    let pgconn = match conn? {
                        MiddlewarePoolConnection::Postgres(pgconn) => pgconn,
                        MiddlewarePoolConnection::Sqlite(_) => {
                            panic!("Only postgres is supported for this test");
                        },
                        MiddlewarePoolConnection::Mssql(_) => {
                            panic!("Only postgres is supported for this test");
                        }
                    };

                    let res = pgconn.execute("SELECT 1", &[]).await;
                    if res.is_ok() && res? == 1 {
                        success = true;
                        // println!("Successfully connected to Postgres!");
                    }
                } else {
                    // println!("Failed to connect to Postgres. Retrying...");
                    thread::sleep(Duration::from_millis(100));
                }
            }

            Ok::<(), Box<dyn std::error::Error>>(())
        })?;

        Ok(PostgresContainer { container_id, port })
    }

    /// Stop a previously started PostgreSQL container
    pub fn stop_postgres_container(container: PostgresContainer) {
        let output = Command::new("podman")
            .args(&["stop", &container.container_id])
            .output()
            .expect("Failed to stop Podman Postgres container");

        // Ensure Podman stopped successfully
        assert!(
            output.status.success(),
            "Failed to stop podman container: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
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