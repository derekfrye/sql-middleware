use crate::middleware::{ConfigAndPool, MiddlewarePool, MiddlewarePoolConnection};
use std::net::TcpStream;
use std::{
    net::TcpListener,
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
                timeout: Some(std::time::Duration::from_secs(30)),
                migration_dir: None,
            };

            let pg_fetch_settings = PgFetchSettings::default();
            
            let mut pg_embed = PgEmbed::new(pg_settings, pg_fetch_settings).await
                .map_err(|e| format!("Failed to initialize embedded PostgreSQL: {}. This might be due to network connectivity or platform compatibility issues. Consider installing PostgreSQL locally for testing.", e))?;
            
            // Setup and start PostgreSQL
            pg_embed.setup().await?;
            pg_embed.start_db().await?;
            
            // Give PostgreSQL time to start up
            thread::sleep(Duration::from_millis(3000));

            // Get the base URI and test database URI
            let pg_base_uri = &pg_embed.db_uri;
            let database_url = pg_embed.full_db_uri(cfg.dbname.as_ref().unwrap());
            println!("PostgreSQL started on port {}", port);
            println!("Base URI: {}", pg_base_uri);
            println!("Database URL: {}", database_url);
            
            // Wait for postgres to be ready by attempting connections
            let mut success = false;
            let mut attempt = 0;
            let max_attempts = 30;

            // First connect to postgres database to create our test database
            let mut postgres_cfg = cfg.clone();
            postgres_cfg.port = Some(port);
            postgres_cfg.host = Some("localhost".to_string());
            postgres_cfg.dbname = Some("postgres".to_string());
            
            println!("Connecting to postgres database to create test database...");
            match ConfigAndPool::new_postgres(postgres_cfg.clone()).await {
                Ok(config_and_pool) => {
                    match config_and_pool.pool.get().await {
                        Ok(pool) => {
                            match MiddlewarePool::get_connection(&pool).await {
                                Ok(conn) => {
                                    if let MiddlewarePoolConnection::Postgres(pgconn) = conn {
                                        let create_db_sql = format!("CREATE DATABASE \"{}\"", cfg.dbname.as_ref().unwrap());
                                        match pgconn.execute(&create_db_sql, &[]).await {
                                            Ok(_) => println!("Successfully created database {}", cfg.dbname.as_ref().unwrap()),
                                            Err(e) => println!("Database creation failed (might already exist): {}", e),
                                        }
                                    }
                                }
                                Err(e) => println!("Failed to get connection to postgres db: {}", e),
                            }
                        }
                        Err(e) => println!("Failed to get pool for postgres db: {}", e),
                    }
                }
                Err(e) => println!("Failed to create config for postgres db: {}", e),
            }

            // Now try to connect to our test database
            let mut test_cfg = cfg.clone();
            test_cfg.port = Some(port);
            test_cfg.host = Some("localhost".to_string());

            while !success && attempt < max_attempts {
                attempt += 1;
                println!("Attempt {} to connect to test database...", attempt);
                
                match ConfigAndPool::new_postgres(test_cfg.clone()).await {
                    Ok(config_and_pool) => {
                        match config_and_pool.pool.get().await {
                            Ok(pool) => {
                                match MiddlewarePool::get_connection(&pool).await {
                                    Ok(conn) => {
                                        if let MiddlewarePoolConnection::Postgres(pgconn) = conn {
                                            match pgconn.execute("SELECT 1", &[]).await {
                                                Ok(1) => {
                                                    println!("Successfully connected to test database!");
                                                    success = true;
                                                }
                                                Ok(rows) => {
                                                    println!("Query returned {} rows instead of 1", rows);
                                                    thread::sleep(Duration::from_millis(100));
                                                }
                                                Err(e) => {
                                                    println!("Query failed: {}", e);
                                                    thread::sleep(Duration::from_millis(100));
                                                }
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        println!("Connection failed: {}", e);
                                        thread::sleep(Duration::from_millis(100));
                                    }
                                }
                            }
                            Err(e) => {
                                println!("Pool get failed: {}", e);
                                thread::sleep(Duration::from_millis(100));
                            }
                        }
                    }
                    Err(e) => {
                        println!("Config creation failed: {}", e);
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
