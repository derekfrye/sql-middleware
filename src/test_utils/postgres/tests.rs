#[cfg(all(test, feature = "test-utils"))]
mod threading_tests {
    use crate::middleware::PgConfig;
    use crate::test_utils::SHARED_RUNTIME;
    use crate::test_utils::postgres::embedded::*;
    use std::thread;
    use std::time::Instant;

    #[cfg(feature = "test-utils")]
    use postgresql_embedded::PostgreSQL;

    #[test]
    fn test_concurrent_postgresql_embedded_initialization() {
        println!("=== Starting concurrent postgresql-embedded test ===");

        let handles: Vec<_> = (0..8)
            .map(|i| {
                thread::spawn(move || {
                    println!("Thread {i} starting postgresql-embedded...");
                    let start_time = Instant::now();

                    let result = SHARED_RUNTIME.block_on(setup_postgresql_embedded_instance(i));

                    let duration = start_time.elapsed();
                    let succeeded = result.is_ok();
                    println!("Thread {i} completed in {duration:?} with result: {succeeded:?}");

                    if let Err(ref e) = result {
                        println!("Thread {i} error: {e}");
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
                        println!("Thread {i} succeeded");
                        results.push(Ok(()));
                    } else {
                        results.push(result);
                    }
                }
                Err(_e) => {
                    println!("Thread {i} join error");
                    results.push(Err("Join error".into()));
                }
            }
        }

        println!("=== postgresql-embedded test completed ===");
        let success_count = results.iter().filter(|r| r.is_ok()).count();
        let failure_count = results.len() - success_count;
        println!("Results: {success_count} successes, {failure_count} failures");
    }

    #[test]
    fn test_embedded_postgres_setup_functions() {
        println!("=== Testing embedded postgres setup functions ===");

        let db_user = "postgres";
        let db_pass = "password";
        let db_name = "test_db";

        let mut cfg = PgConfig::new();
        cfg.dbname = Some(db_name.to_string());
        cfg.user = Some(db_user.to_string());
        cfg.password = Some(db_pass.to_string());

        let postgres_instance =
            setup_postgres_embedded(&cfg).expect("Failed to setup embedded PostgreSQL instance");

        println!(
            "✓ Successfully started embedded PostgreSQL on port {port}",
            port = postgres_instance.port
        );
        println!(
            "✓ Database URL: {database_url}",
            database_url = postgres_instance.database_url
        );

        stop_postgres_embedded(postgres_instance);
        println!("✓ Successfully stopped embedded PostgreSQL instance");
    }

    #[test]
    fn test_concurrent_embedded_postgres_setup_functions() {
        println!("=== Testing concurrent embedded postgres setup functions ===");

        let handles: Vec<_> = (0..8)
            .map(|i| {
                thread::spawn(move || {
                    println!("Thread {i} starting embedded postgres setup functions...");
                    let start_time = Instant::now();

                    let db_user = "postgres";
                    let db_pass = "password";
                    let db_name = format!("test_db_{i}");

                    let mut cfg = PgConfig::new();
                    cfg.dbname = Some(db_name);
                    cfg.user = Some(db_user.to_string());
                    cfg.password = Some(db_pass.to_string());

                    let result = setup_postgres_embedded(&cfg);

                    let duration = start_time.elapsed();
                    let succeeded = result.is_ok();
                    println!("Thread {i} completed in {duration:?} with result: {succeeded:?}");

                    if let Ok(postgres_instance) = result {
                        println!(
                            "Thread {i}: Started on port {port}",
                            port = postgres_instance.port
                        );
                        stop_postgres_embedded(postgres_instance);
                        println!("Thread {i}: Stopped successfully");
                        Ok(())
                    } else if let Err(ref e) = result {
                        println!("Thread {i} error: {e}");
                        Err(format!("Setup failed: {e}"))
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
                        println!("Thread {i} succeeded");
                        results.push(Ok(()));
                    } else {
                        results.push(result);
                    }
                }
                Err(_e) => {
                    println!("Thread {i} join error");
                    results.push(Err("Join error".to_string()));
                }
            }
        }

        println!("=== concurrent embedded postgres setup test completed ===");
        let success_count = results.iter().filter(|r| r.is_ok()).count();
        let failure_count = results.len() - success_count;
        println!("Results: {success_count} successes, {failure_count} failures");

        // Assert that all 8 threads succeeded
        assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 8);
    }

    async fn setup_postgresql_embedded_instance(
        thread_id: usize,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        println!("Thread {thread_id}: Starting postgresql-embedded setup");

        let mut postgresql = PostgreSQL::default();

        println!("Thread {thread_id}: Calling setup()");
        postgresql.setup().await.map_err(|e| {
            println!("Thread {thread_id}: setup() failed: {e}");
            format!("Thread {thread_id}: setup() failed: {e}")
        })?;

        println!("Thread {thread_id}: setup() completed, calling start()");
        postgresql.start().await.map_err(|e| {
            println!("Thread {thread_id}: start() failed: {e}");
            format!("Thread {thread_id}: start() failed: {e}")
        })?;

        println!("Thread {thread_id}: start() completed, testing database operations");

        let db_name = format!("test_db_{thread_id}");
        postgresql.create_database(&db_name).await.map_err(|e| {
            println!("Thread {thread_id}: create_database failed: {e}");
            format!("Thread {thread_id}: create_database failed: {e}")
        })?;

        let exists = postgresql.database_exists(&db_name).await.map_err(|e| {
            println!("Thread {thread_id}: database_exists failed: {e}");
            format!("Thread {thread_id}: database_exists failed: {e}")
        })?;

        if !exists {
            return Err(format!("Thread {thread_id}: Database {db_name} was not created").into());
        }

        postgresql.drop_database(&db_name).await.map_err(|e| {
            println!("Thread {thread_id}: drop_database failed: {e}");
            format!("Thread {thread_id}: drop_database failed: {e}")
        })?;

        println!("Thread {thread_id}: stopping postgresql");
        postgresql.stop().await.map_err(|e| {
            println!("Thread {thread_id}: stop() failed: {e}");
            format!("Thread {thread_id}: stop() failed: {e}")
        })?;

        println!("Thread {thread_id}: PostgreSQL instance stopped successfully");
        Ok(())
    }
}
