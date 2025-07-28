#[cfg(all(test, feature = "test-utils"))]
mod threading_tests {
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