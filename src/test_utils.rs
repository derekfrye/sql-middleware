use crate::middleware::{ConfigAndPool, MiddlewarePool, MiddlewarePoolConnection};
use regex::Regex;
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
    
    #[cfg(feature = "test-utils")]
    use pg_embed::postgres::{PgEmbed, PgSettings};
    #[cfg(feature = "test-utils")]
    use pg_embed::pg_enums::PgAuthMethod;
    #[cfg(feature = "test-utils")]
    use pg_embed::pg_fetch::PgFetchSettings;
    #[cfg(feature = "test-utils")]
    use std::path::PathBuf;

    /// Represents a running embedded PostgreSQL instance
    #[cfg(feature = "test-utils")]
    pub struct EmbeddedPostgres {
        pub pg_embed: PgEmbed,
        pub port: u16,
        pub database_url: String,
    }

    /// Set up an embedded PostgreSQL instance for testing or benchmarking
    #[cfg(feature = "test-utils")]
    pub fn setup_postgres_embedded(
        cfg: &deadpool_postgres::Config,
    ) -> Result<EmbeddedPostgres, Box<dyn std::error::Error>> {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let port = find_available_port(9050);
            
            let pg_settings = PgSettings {
                port,
                user: cfg.user.as_ref().unwrap().clone(),
                password: cfg.password.as_ref().unwrap().clone(),
                persistent: false,
                database_dir: PathBuf::from(&format!("/tmp/pg_embed_test_{}", port)),
                auth_method: PgAuthMethod::Plain,
                timeout: None,
                migration_dir: None,
            };

            let pg_fetch_settings = PgFetchSettings::default();
            let mut pg_embed = PgEmbed::new(pg_settings, pg_fetch_settings).await?;
            
            // Setup and start PostgreSQL
            pg_embed.setup().await?;
            pg_embed.start_db().await?;
            
            // Create the database
            pg_embed.create_database(cfg.dbname.as_ref().unwrap()).await?;

            let database_url = pg_embed.full_db_uri(cfg.dbname.as_ref().unwrap());
            
            // Wait for postgres to be ready by attempting connections
            let mut success = false;
            let mut attempt = 0;
            let max_attempts = 30;

            let mut test_cfg = cfg.clone();
            test_cfg.port = Some(port);
            test_cfg.host = Some("localhost".to_string());

            while !success && attempt < max_attempts {
                attempt += 1;
                
                match ConfigAndPool::new_postgres(test_cfg.clone()).await {
                    Ok(config_and_pool) => {
                        match config_and_pool.pool.get().await {
                            Ok(pool) => {
                                match MiddlewarePool::get_connection(&pool).await {
                                    Ok(conn) => {
                                        if let MiddlewarePoolConnection::Postgres(pgconn) = conn {
                                            match pgconn.execute("SELECT 1", &[]).await {
                                                Ok(rows) if rows == 1 => {
                                                    success = true;
                                                }
                                                _ => {
                                                    thread::sleep(Duration::from_millis(100));
                                                }
                                            }
                                        }
                                    }
                                    Err(_) => {
                                        thread::sleep(Duration::from_millis(100));
                                    }
                                }
                            }
                            Err(_) => {
                                thread::sleep(Duration::from_millis(100));
                            }
                        }
                    }
                    Err(_) => {
                        thread::sleep(Duration::from_millis(100));
                    }
                }
            }

            if !success {
                return Err("Failed to connect to embedded PostgreSQL after multiple attempts".into());
            }

            Ok(EmbeddedPostgres {
                pg_embed,
                port,
                database_url,
            })
        })
    }

    /// Stop a previously started embedded PostgreSQL instance
    #[cfg(feature = "test-utils")]
    pub fn stop_postgres_embedded(mut postgres: EmbeddedPostgres) {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let _ = postgres.pg_embed.stop_db().await;
        });
    }

    // Legacy function name for backward compatibility
    #[cfg(feature = "test-utils")]
    pub fn setup_postgres_container(
        cfg: &deadpool_postgres::Config,
    ) -> Result<EmbeddedPostgres, Box<dyn std::error::Error>> {
        setup_postgres_embedded(cfg)
    }

    // Legacy function name for backward compatibility
    #[cfg(feature = "test-utils")]
    pub fn stop_postgres_container(postgres: EmbeddedPostgres) {
        stop_postgres_embedded(postgres);
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
