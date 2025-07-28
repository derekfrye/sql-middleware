use crate::middleware::{ConfigAndPool, MiddlewarePool, MiddlewarePoolConnection};
use tokio::runtime::Runtime;
use std::sync::LazyLock;

/// Shared tokio runtime for test utilities to avoid creating multiple runtimes
static SHARED_RUNTIME: LazyLock<Runtime> = LazyLock::new(|| {
    Runtime::new().expect("Failed to create tokio runtime for test utilities")
});

/// Test utilities for PostgreSQL testing and benchmarking
pub mod testing_postgres {
    use super::*;

    #[cfg(feature = "test-utils")]
    use postgresql_embedded::PostgreSQL;

    /// Represents a running embedded PostgreSQL instance
    #[cfg(feature = "test-utils")]
    pub struct EmbeddedPostgres {
        pub postgresql: PostgreSQL,
        pub port: u16,
        pub database_url: String,
        /// The actual working configuration with correct credentials
        pub config: deadpool_postgres::Config,
    }


    /// Set up an embedded PostgreSQL instance for testing or benchmarking
    #[cfg(feature = "test-utils")]
    pub fn setup_postgres_embedded(
        cfg: &deadpool_postgres::Config,
    ) -> Result<EmbeddedPostgres, Box<dyn std::error::Error>> {
        SHARED_RUNTIME.block_on(async {
            let mut postgresql = PostgreSQL::default();
            
            // Setup PostgreSQL binaries (bundled, so no download conflicts)
            postgresql.setup().await?;
            
            // Start the PostgreSQL instance
            postgresql.start().await?;
            
            let port = postgresql.settings().port;
            let host = postgresql.settings().host.clone();
            let embedded_user = postgresql.settings().username.clone();
            let embedded_password = postgresql.settings().password.clone();
            
            // Create the test database
            let db_name = cfg.dbname.as_ref().unwrap();
            postgresql.create_database(db_name).await?;
            
            // For backward compatibility, create a user with the credentials from the config
            // if they're different from the embedded defaults
            let (final_user, final_password) = if let (Some(desired_user), Some(desired_password)) = 
                (cfg.user.as_ref(), cfg.password.as_ref()) {
                
                if desired_user != &embedded_user {
                    // Connect as the embedded user to create the desired user
                    let mut admin_cfg = cfg.clone();
                    admin_cfg.port = Some(port);
                    admin_cfg.host = Some(host.clone());
                    admin_cfg.user = Some(embedded_user.clone());
                    admin_cfg.password = Some(embedded_password.clone());
                    admin_cfg.dbname = Some("postgres".to_string());
                    
                    let admin_pool = ConfigAndPool::new_postgres(admin_cfg).await?;
                    let pool = admin_pool.pool.get().await?;
                    let admin_conn = MiddlewarePool::get_connection(&pool).await?;
                    
                    if let MiddlewarePoolConnection::Postgres(pgconn) = admin_conn {
                        // Create the desired user with the desired password
                        let create_user_sql = format!(
                            "CREATE USER \"{}\" WITH PASSWORD '{}' CREATEDB SUPERUSER",
                            desired_user, desired_password
                        );
                        pgconn.execute(&create_user_sql, &[]).await
                            .map_err(|e| format!("Failed to create user {}: {}", desired_user, e))?;
                        
                        println!("Created user {} with desired credentials", desired_user);
                    }
                    
                    (desired_user.clone(), desired_password.clone())
                } else {
                    (embedded_user, embedded_password)
                }
            } else {
                (embedded_user, embedded_password)
            };
            
            let database_url = format!("postgres://{}:{}@{}:{}/{}", 
                final_user, final_password, host, port, db_name);
            
            println!("PostgreSQL started on port {}", port);
            println!("Database URL: {}", database_url);
            
            // Create final config with correct credentials
            let mut final_cfg = cfg.clone();
            final_cfg.port = Some(port);
            final_cfg.host = Some(host);
            final_cfg.user = Some(final_user);
            final_cfg.password = Some(final_password);
            
            // Quick connection test
            let config_and_pool = ConfigAndPool::new_postgres(final_cfg.clone()).await?;
            let pool = config_and_pool.pool.get().await?;
            let conn = MiddlewarePool::get_connection(&pool).await?;
            
            if let MiddlewarePoolConnection::Postgres(pgconn) = conn {
                // Test with a simple query
                pgconn.execute("SELECT 1", &[]).await?;
                println!("Successfully connected to embedded PostgreSQL database!");
            }
            
            Ok(EmbeddedPostgres {
                postgresql,
                port,
                database_url,
                config: final_cfg,
            })
        })
    }

    /// Stop a previously started embedded PostgreSQL instance
    #[cfg(feature = "test-utils")]
    pub fn stop_postgres_embedded(postgres: EmbeddedPostgres) {
        SHARED_RUNTIME.block_on(async {
            let _ = postgres.postgresql.stop().await;
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


    #[cfg(all(test, feature = "test-utils"))]
    mod threading_tests {
        use super::*;
        use std::thread;
        use std::time::Instant;
        use postgresql_embedded::PostgreSQL;

        #[test]
        fn test_concurrent_postgresql_embedded_initialization() {
            println!("=== Starting concurrent postgresql-embedded test ===");
            
            let handles: Vec<_> = (0..8)
                .map(|i| {
                    thread::spawn(move || {
                        println!("Thread {} starting postgresql-embedded...", i);
                        let start_time = Instant::now();
                        
                        let result = SHARED_RUNTIME.block_on(setup_postgresql_embedded_instance(i));
                        
                        let duration = start_time.elapsed();
                        println!("Thread {} completed in {:?} with result: {:?}", 
                               i, duration, result.is_ok());
                        
                        if let Err(ref e) = result {
                            println!("Thread {} error: {}", i, e);
                        }
                        
                        result
                    })
                })
                .collect();

            let mut results = Vec::new();
            for (i, handle) in handles.into_iter().enumerate() {
                match handle.join() {
                    Ok(result) => {
                        if result.is_ok() {
                            println!("Thread {} succeeded", i);
                            results.push(Ok(()));
                        } else {
                            results.push(result.map(|_| ()));
                        }
                    },
                    Err(_e) => {
                        println!("Thread {} join error", i);
                        results.push(Err("Join error".into()));
                    }
                }
            }

            println!("=== postgresql-embedded test completed ===");
            println!("Results: {} successes, {} failures", 
                   results.iter().filter(|r| r.is_ok()).count(),
                   results.iter().filter(|r| r.is_err()).count());
        }

        #[test]
        fn test_embedded_postgres_setup_functions() {
            println!("=== Testing embedded postgres setup functions ===");
            
            let db_user = "postgres";
            let db_pass = "password";
            let db_name = "test_db";

            let mut cfg = deadpool_postgres::Config::new();
            cfg.dbname = Some(db_name.to_string());
            cfg.user = Some(db_user.to_string());
            cfg.password = Some(db_pass.to_string());

            let postgres_instance = setup_postgres_embedded(&cfg)
                .expect("Failed to setup embedded PostgreSQL instance");

            println!("✓ Successfully started embedded PostgreSQL on port {}", postgres_instance.port);
            println!("✓ Database URL: {}", postgres_instance.database_url);

            stop_postgres_embedded(postgres_instance);
            println!("✓ Successfully stopped embedded PostgreSQL instance");
        }

        #[test]
        fn test_concurrent_embedded_postgres_setup_functions() {
            println!("=== Testing concurrent embedded postgres setup functions ===");
            
            let handles: Vec<_> = (0..8)
                .map(|i| {
                    thread::spawn(move || {
                        println!("Thread {} starting embedded postgres setup functions...", i);
                        let start_time = Instant::now();
                        
                        let db_user = "postgres";
                        let db_pass = "password";
                        let db_name = format!("test_db_{}", i);

                        let mut cfg = deadpool_postgres::Config::new();
                        cfg.dbname = Some(db_name);
                        cfg.user = Some(db_user.to_string());
                        cfg.password = Some(db_pass.to_string());

                        let result = setup_postgres_embedded(&cfg);
                        
                        let duration = start_time.elapsed();
                        println!("Thread {} completed in {:?} with result: {:?}", 
                               i, duration, result.is_ok());
                        
                        if let Ok(postgres_instance) = result {
                            println!("Thread {}: Started on port {}", i, postgres_instance.port);
                            stop_postgres_embedded(postgres_instance);
                            println!("Thread {}: Stopped successfully", i);
                            Ok(())
                        } else if let Err(ref e) = result {
                            println!("Thread {} error: {}", i, e);
                            Err(format!("Setup failed: {}", e))
                        } else {
                            unreachable!()
                        }
                    })
                })
                .collect();

            let mut results = Vec::new();
            for (i, handle) in handles.into_iter().enumerate() {
                match handle.join() {
                    Ok(result) => {
                        if result.is_ok() {
                            println!("Thread {} succeeded", i);
                            results.push(Ok(()));
                        } else {
                            results.push(result);
                        }
                    },
                    Err(_e) => {
                        println!("Thread {} join error", i);
                        results.push(Err("Join error".to_string()));
                    }
                }
            }

            println!("=== concurrent embedded postgres setup test completed ===");
            println!("Results: {} successes, {} failures", 
                   results.iter().filter(|r| r.is_ok()).count(),
                   results.iter().filter(|r| r.is_err()).count());
                   
            // Assert that all 8 threads succeeded
            assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 8);
        }

        async fn setup_postgresql_embedded_instance(thread_id: usize) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
            println!("Thread {}: Starting postgresql-embedded setup", thread_id);
            
            let mut postgresql = PostgreSQL::default();
            
            println!("Thread {}: Calling setup()", thread_id);
            postgresql.setup().await.map_err(|e| {
                println!("Thread {}: setup() failed: {}", thread_id, e);
                format!("Thread {}: setup() failed: {}", thread_id, e)
            })?;
            
            println!("Thread {}: setup() completed, calling start()", thread_id);
            postgresql.start().await.map_err(|e| {
                println!("Thread {}: start() failed: {}", thread_id, e);
                format!("Thread {}: start() failed: {}", thread_id, e)
            })?;
            
            println!("Thread {}: start() completed, testing database operations", thread_id);
            
            let db_name = format!("test_db_{}", thread_id);
            postgresql.create_database(&db_name).await.map_err(|e| {
                println!("Thread {}: create_database failed: {}", thread_id, e);
                format!("Thread {}: create_database failed: {}", thread_id, e)
            })?;
            
            let exists = postgresql.database_exists(&db_name).await.map_err(|e| {
                println!("Thread {}: database_exists failed: {}", thread_id, e);
                format!("Thread {}: database_exists failed: {}", thread_id, e)
            })?;
            
            if !exists {
                return Err(format!("Thread {}: Database {} was not created", thread_id, db_name).into());
            }
            
            postgresql.drop_database(&db_name).await.map_err(|e| {
                println!("Thread {}: drop_database failed: {}", thread_id, e);
                format!("Thread {}: drop_database failed: {}", thread_id, e)
            })?;
            
            println!("Thread {}: stopping postgresql", thread_id);
            postgresql.stop().await.map_err(|e| {
                println!("Thread {}: stop() failed: {}", thread_id, e);
                format!("Thread {}: stop() failed: {}", thread_id, e)
            })?;
            
            println!("Thread {}: PostgreSQL instance stopped successfully", thread_id);
            Ok(())
        }
    }
}



