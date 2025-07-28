use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use std::sync::{LazyLock, Mutex};
use rand::Rng;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use serde_json::json;
#[cfg(feature = "test-utils")]
use sql_middleware::test_utils::postgres::EmbeddedPostgres;
use sql_middleware::{
    SqlMiddlewareDbError,
    middleware::{ConfigAndPool, MiddlewarePool, MiddlewarePoolConnection},
};
use std::{fs, path::Path};
use tokio::runtime::Runtime;

// Global PostgreSQL instance for benchmarks - set up once, torn down at process exit
static POSTGRES_INSTANCE: LazyLock<Mutex<Option<(ConfigAndPool, EmbeddedPostgres)>>> = 
    LazyLock::new(|| Mutex::new(None));

// Global SQLite instance for benchmarks - set up once, torn down at process exit
static SQLITE_INSTANCE: LazyLock<Mutex<Option<(ConfigAndPool, String)>>> = 
    LazyLock::new(|| Mutex::new(None));

// Helper to get or create the global PostgreSQL instance
async fn get_postgres_instance() -> ConfigAndPool {
    let mut instance_guard = POSTGRES_INSTANCE.lock().unwrap();
    
    if instance_guard.is_none() {
        println!("Setting up PostgreSQL instance (one-time setup)...");
        let db_user = "test_user";
        let db_pass = "test_password123";
        let db_name = "test_db";
        
        let (config_and_pool, postgres_instance) = 
            setup_postgres_db(db_user, db_pass, db_name).await.unwrap();
        
        *instance_guard = Some((config_and_pool, postgres_instance));
        println!("PostgreSQL instance ready!");
    }
    
    // Return a clone of the ConfigAndPool
    let (config_and_pool, _) = instance_guard.as_ref().unwrap();
    config_and_pool.clone()
}

// Helper to get or create the global SQLite instance
async fn get_sqlite_instance() -> ConfigAndPool {
    let mut instance_guard = SQLITE_INSTANCE.lock().unwrap();
    
    if instance_guard.is_none() {
        println!("Setting up SQLite instance (one-time setup)...");
        let db_path = "benchmark_sqlite_global.db".to_string();
        
        let config_and_pool = setup_sqlite_db(&db_path).await.unwrap();
        
        *instance_guard = Some((config_and_pool, db_path));
        println!("SQLite instance ready!");
    }
    
    // Return a clone of the ConfigAndPool
    let (config_and_pool, _) = instance_guard.as_ref().unwrap();
    config_and_pool.clone()
}

// Helper to clean the database between benchmark runs (but keep SQLite file)
async fn clean_sqlite_tables(config_and_pool: &ConfigAndPool) -> Result<(), Box<dyn std::error::Error>> {
    let pool = config_and_pool.pool.get().await?;
    let conn = MiddlewarePool::get_connection(&pool).await?;
    
    if let MiddlewarePoolConnection::Sqlite(sconn) = conn {
        sconn.interact(move |conn| {
            // Drop and recreate the test table to clean it
            conn.execute("DROP TABLE IF EXISTS test", [])?;
            
            let create_sql = "CREATE TABLE IF NOT EXISTS test (
                recid INTEGER PRIMARY KEY AUTOINCREMENT,
                a int,
                b text,
                c datetime not null default current_timestamp,
                d real,
                e boolean,
                f blob,
                g json,
                h text, i text, j text, k text, l text, m text, n text, o text, p text
            )";
            conn.execute(create_sql, [])?;
            
            Ok::<_, SqlMiddlewareDbError>(())
        }).await??;
    }
    Ok(())
}

// Helper to clean the database between benchmark runs (but keep PostgreSQL running)
async fn clean_postgres_tables(config_and_pool: &ConfigAndPool) -> Result<(), Box<dyn std::error::Error>> {
    let pool = config_and_pool.pool.get().await?;
    let conn = MiddlewarePool::get_connection(&pool).await?;
    
    if let MiddlewarePoolConnection::Postgres(pgconn) = conn {
        // Drop and recreate the test table to clean it
        let cleanup_sql = "DROP TABLE IF EXISTS test";
        pgconn.execute(cleanup_sql, &[]).await?;
        
        let create_sql = "CREATE TABLE IF NOT EXISTS test (
            recid SERIAL PRIMARY KEY,
            a int,
            b text,
            c timestamp not null default now(),
            d real,
            e boolean,
            f bytea,
            g jsonb,
            h text, i text, j text, k text, l text, m text, n text, o text, p text
        )";
        pgconn.execute(create_sql, &[]).await?;
    }
    Ok(())
}

// Reviewed; Function to generate a deterministic set of SQL insert statements
fn generate_insert_statements(num_rows: usize) -> String {
    // Create a deterministic RNG with fixed seed for reproducibility
    let mut rng = ChaCha8Rng::seed_from_u64(42);

    let mut statements = String::with_capacity(num_rows * 200); // Rough estimate of string capacity

    for i in 0..num_rows {
        // Generate random data for each row
        let a = rng.random_range(1..1000);
        let b = format!("text-{}", rng.random_range(1..1000));

        // Generate a timestamp between 2020-01-01 and 2025-12-31
        let timestamp_year = rng.random_range(2020..=2025);
        let timestamp_month = rng.random_range(1..=12);
        let timestamp_day = rng.random_range(1..=28); // Avoiding edge cases with days per month
        let timestamp_hour = rng.random_range(0..=23);
        let timestamp_minute = rng.random_range(0..=59);
        let timestamp_second = rng.random_range(0..=59);
        let timestamp = format!(
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
            timestamp_year,
            timestamp_month,
            timestamp_day,
            timestamp_hour,
            timestamp_minute,
            timestamp_second
        );

        let d = rng.random_range(0.0..1000.0);
        let e = rng.random_bool(0.5);

        // Generate a random blob of 10-20 bytes
        let blob_len = rng.random_range(10..=20);
        let blob: Vec<u8> = (0..blob_len).map(|_| rng.random_range(0..=255)).collect();
        let blob_hex = blob
            .iter()
            .map(|b| format!("{:02X}", b))
            .collect::<String>();

        // Generate a random JSON object
        let json_value = json!({
            "id": i,
            "value": rng.random_range(1..100),
            "tags": [
                format!("tag-{}", rng.random_range(1..10)),
                format!("tag-{}", rng.random_range(1..10))
            ]
        })
        .to_string();

        // Generate additional text fields
        let additional_texts: Vec<String> =
            (0..9).map(|j| format!("text-field-{}-{}", i, j)).collect();

        // Append the SQL statement
        let insert = format!(
            "INSERT INTO test (a, b, c, d, e, f, g, h, i, j, k, l, m, n, o, p) VALUES ({}, '{}', '{}', {}, {}, X'{}', '{}', '{}', '{}', '{}', '{}', '{}', '{}', '{}', '{}', '{}');\n",
            a,
            b,
            timestamp,
            d,
            if e { 1 } else { 0 },
            blob_hex,
            json_value,
            additional_texts[0],
            additional_texts[1],
            additional_texts[2],
            additional_texts[3],
            additional_texts[4],
            additional_texts[5],
            additional_texts[6],
            additional_texts[7],
            additional_texts[8]
        );

        statements.push_str(&insert);
    }

    statements
}

// PostgreSQL-specific version with proper data type handling
fn generate_postgres_insert_statements(num_rows: usize) -> String {
    // Create a deterministic RNG with fixed seed for reproducibility
    let mut rng = ChaCha8Rng::seed_from_u64(42);

    let mut statements = String::with_capacity(num_rows * 200); // Rough estimate of string capacity

    for i in 0..num_rows {
        // Generate random data for each row
        let a = rng.random_range(1..1000);
        let b = format!("text-{}", rng.random_range(1..1000));

        // Generate a timestamp between 2020-01-01 and 2025-12-31
        let timestamp_year = rng.random_range(2020..=2025);
        let timestamp_month = rng.random_range(1..=12);
        let timestamp_day = rng.random_range(1..=28); // Avoiding edge cases with days per month
        let timestamp_hour = rng.random_range(0..=23);
        let timestamp_minute = rng.random_range(0..=59);
        let timestamp_second = rng.random_range(0..=59);
        let timestamp = format!(
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
            timestamp_year,
            timestamp_month,
            timestamp_day,
            timestamp_hour,
            timestamp_minute,
            timestamp_second
        );

        let d = rng.random_range(0.0..1000.0);
        let e = rng.random_bool(0.5);

        // Generate a random blob of 10-20 bytes
        let blob_len = rng.random_range(10..=20);
        let blob: Vec<u8> = (0..blob_len).map(|_| rng.random_range(0..=255)).collect();
        let blob_hex = blob
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect::<String>();

        // Generate a random JSON object
        let json_value = json!({
            "id": i,
            "value": rng.random_range(1..100),
            "tags": [
                format!("tag-{}", rng.random_range(1..10)),
                format!("tag-{}", rng.random_range(1..10))
            ]
        })
        .to_string();

        // Generate additional text fields
        let additional_texts: Vec<String> =
            (0..9).map(|j| format!("text-field-{}-{}", i, j)).collect();

        // Append the SQL statement with PostgreSQL-specific syntax
        let insert = format!(
            "INSERT INTO test (a, b, c, d, e, f, g, h, i, j, k, l, m, n, o, p) VALUES ({}, '{}', '{}', {}, {}, E'\\\\x{}', '{}', '{}', '{}', '{}', '{}', '{}', '{}', '{}', '{}', '{}');\n",
            a,
            b,
            timestamp,
            d,
            if e { "true" } else { "false" }, // PostgreSQL boolean syntax
            blob_hex,
            json_value,
            additional_texts[0],
            additional_texts[1],
            additional_texts[2],
            additional_texts[3],
            additional_texts[4],
            additional_texts[5],
            additional_texts[6],
            additional_texts[7],
            additional_texts[8]
        );

        statements.push_str(&insert);
    }

    statements
}

// LibSQL setup function (similar to SQLite but uses ConfigAndPool::new_libsql)
#[allow(dead_code)] // Disabled due to threading issues, kept for future use
async fn setup_libsql_db() -> Result<ConfigAndPool, SqlMiddlewareDbError> {
    // Use in-memory database to minimize threading issues
    let config_and_pool = ConfigAndPool::new_libsql(":memory:".to_string()).await?;

    // Create the test table (same schema as SQLite)
    let ddl = "CREATE TABLE IF NOT EXISTS test (
        recid INTEGER PRIMARY KEY AUTOINCREMENT,
        a int,
        b text,
        c datetime not null default current_timestamp,
        d real,
        e boolean,
        f blob,
        g json,
        h text, i text, j text, k text, l text, m text, n text, o text, p text
    )";

    let pool = config_and_pool.pool.get().await?;
    let libsql_conn = MiddlewarePool::get_connection(&pool).await?;

    if let MiddlewarePoolConnection::Libsql(lconn) = libsql_conn {
        lconn.execute_batch(ddl).await?;
    } else {
        panic!("Expected LibSQL connection");
    }

    Ok(config_and_pool)
}

// reviewed
async fn setup_sqlite_db(db_path: &str) -> Result<ConfigAndPool, SqlMiddlewareDbError> {
    // Ensure the path doesn't exist
    if Path::new(db_path).exists() {
        fs::remove_file(db_path).unwrap();
    }

    let config_and_pool = ConfigAndPool::new_sqlite(db_path.to_string()).await?;

    // Create the test table
    let ddl = "CREATE TABLE IF NOT EXISTS test (
        recid INTEGER PRIMARY KEY AUTOINCREMENT,
        a int,
        b text,
        c datetime not null default current_timestamp,
        d real,
        e boolean,
        f blob,
        g json,
        h text, i text, j text, k text, l text, m text, n text, o text, p text
    )";

    let pool = config_and_pool.pool.get().await?;
    let sqlite_conn = MiddlewarePool::get_connection(&pool).await?;

    if let MiddlewarePoolConnection::Sqlite(sconn) = sqlite_conn {
        sconn
            .interact(move |conn| {
                let tx = conn.transaction()?;
                tx.execute_batch(ddl)?;
                tx.commit()?;
                Ok::<_, SqlMiddlewareDbError>(())
            })
            .await??;
    } else {
        panic!("Expected SQLite connection");
    }

    Ok(config_and_pool)
}

async fn setup_postgres_db(
    db_user: &str,
    db_pass: &str,
    db_name: &str,
) -> Result<(ConfigAndPool, EmbeddedPostgres), Box<dyn std::error::Error>> {
    use sql_middleware::test_utils::postgres::EmbeddedPostgres;
    #[cfg(feature = "test-utils")]
    use postgresql_embedded::PostgreSQL;
    
    let mut cfg = deadpool_postgres::Config::new();
    cfg.dbname = Some(db_name.to_string());
    cfg.user = Some(db_user.to_string());
    cfg.password = Some(db_pass.to_string());
    
    // Set up embedded PostgreSQL directly in async context to avoid runtime nesting
    let mut postgresql = PostgreSQL::default();
    postgresql.setup().await?;
    postgresql.start().await?;
    
    let port = postgresql.settings().port;
    let host = postgresql.settings().host.clone();
    let embedded_user = postgresql.settings().username.clone();
    let embedded_password = postgresql.settings().password.clone();
    
    // Create the test database
    postgresql.create_database(db_name).await?;
    
    // For backward compatibility, create a user with the credentials from the config
    let (final_user, final_password) = if db_user != &embedded_user {
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
                db_user, db_pass
            );
            pgconn.execute(&create_user_sql, &[]).await
                .map_err(|e| format!("Failed to create user {}: {}", db_user, e))?;
        }
        
        (db_user.to_string(), db_pass.to_string())
    } else {
        (embedded_user, embedded_password)
    };
    
    let database_url = format!("postgres://{}:{}@{}:{}/{}", 
        final_user, final_password, host, port, db_name);
    
    // Create final config with correct credentials
    let mut final_cfg = cfg.clone();
    final_cfg.port = Some(port);
    final_cfg.host = Some(host.clone());
    final_cfg.user = Some(final_user.clone());
    final_cfg.password = Some(final_password.clone());
    
    let postgres_instance = EmbeddedPostgres {
        postgresql,
        port,
        database_url,
        config: final_cfg.clone(),
    };
    
    let config_and_pool = ConfigAndPool::new_postgres(final_cfg).await?;

    // Create the test table
    let ddl = "CREATE TABLE IF NOT EXISTS test (
        recid SERIAL PRIMARY KEY,
        a int,
        b text,
        c timestamp not null default now(),
        d real,
        e boolean,
        f bytea,
        g jsonb,
        h text, i text, j text, k text, l text, m text, n text, o text, p text
    )";

    let pool = config_and_pool.pool.get().await?;
    let conn = MiddlewarePool::get_connection(&pool).await?;

    if let MiddlewarePoolConnection::Postgres(mut pgconn) = conn {
        let tx = pgconn.transaction().await?;
        tx.batch_execute(ddl).await?;
        tx.commit().await?;
    } else {
        panic!("Expected PostgreSQL connection");
    }

    Ok((config_and_pool, postgres_instance))
}

fn get_benchmark_rows() -> usize {
    // Use environment variable for configuration
    // Can be set via: BENCH_ROWS=10000 cargo bench
    std::env::var("BENCH_ROWS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(50_000)
}

fn benchmark_sqlite(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let num_rows = get_benchmark_rows();
    println!("Running SQLite benchmark with {} rows", num_rows);

    // Generate insert statements once (same for all benchmark runs)
    let insert_statements = generate_insert_statements(num_rows);

    let mut group = c.benchmark_group("database_inserts");

    group.bench_function(BenchmarkId::new("sqlite", format!("{}_rows", num_rows)), |b| {
        let statements = insert_statements.clone();
        b.to_async(&rt).iter_custom(|iters| {
            let statements = statements.clone();
            async move {
                // ONE-TIME SETUP: Get or create the global SQLite instance
                let config_and_pool = get_sqlite_instance().await;
                
                let mut total_duration = std::time::Duration::new(0, 0);
                
                for _i in 0..iters {
                    // SETUP PHASE (not timed): Clean the database and get connection
                    clean_sqlite_tables(&config_and_pool).await.unwrap();
                    let pool = config_and_pool.pool.get().await.unwrap();
                    let sqlite_conn = MiddlewarePool::get_connection(&pool).await.unwrap();

                    if let MiddlewarePoolConnection::Sqlite(sconn) = sqlite_conn {
                        let insert_statements_copy = statements.clone();

                        // START TIMING: The actual benchmarked part (inserts only)
                        let start = std::time::Instant::now();
                        
                        sconn
                            .interact(move |conn| {
                                let tx = conn.transaction()?;
                                tx.execute_batch(&insert_statements_copy)?;
                                tx.commit()?;
                                Ok::<_, SqlMiddlewareDbError>(())
                            })
                            .await
                            .unwrap()
                            .unwrap();
                        
                        // STOP TIMING
                        let elapsed = start.elapsed();
                        total_duration += elapsed;
                    }
                    
                    // No teardown needed - SQLite file stays
                }
                
                total_duration
            }
        });
    });

    group.finish();
}

#[allow(dead_code)] // Disabled due to threading issues, kept for future use
fn benchmark_libsql(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let num_rows = get_benchmark_rows();
    println!("Running LibSQL benchmark with {} rows", num_rows);

    // Generate insert statements once (same for all benchmark runs)
    // LibSQL uses SQLite-compatible syntax, so we can reuse the same statements
    let insert_statements = generate_insert_statements(num_rows);

    let mut group = c.benchmark_group("libsql_inserts");
    // Ensure LibSQL runs single-threaded to avoid threading conflicts
    group.sample_size(10); // Reduce sample size to speed up benchmarks

    group.bench_function(BenchmarkId::new("libsql", format!("{}_rows", num_rows)), |b| {
        b.to_async(&rt).iter(|| async {
            // Setup LibSQL database (in-memory)
            let config_and_pool = setup_libsql_db().await.unwrap();

            // Get connection from pool
            let pool = config_and_pool.pool.get().await.unwrap();
            let libsql_conn = MiddlewarePool::get_connection(&pool).await.unwrap();

            if let MiddlewarePoolConnection::Libsql(lconn) = libsql_conn {
                let insert_statements_copy = insert_statements.clone();

                // The actual benchmarked part
                lconn.execute_batch(&insert_statements_copy).await.unwrap();
            }

            // No cleanup needed for in-memory database
        });
    });

    group.finish();
}

fn benchmark_postgres(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let num_rows = get_benchmark_rows();
    println!("Running PostgreSQL benchmark with {} rows", num_rows);

    // Generate PostgreSQL-specific insert statements to handle data types properly
    let postgres_insert_statements = generate_postgres_insert_statements(num_rows);

    let mut group = c.benchmark_group("database_inserts");

    group.bench_function(BenchmarkId::new("postgres", format!("{}_rows", num_rows)), |b| {
        let statements = postgres_insert_statements.clone();
        b.to_async(&rt).iter_custom(|iters| {
            let statements = statements.clone();
            async move {
                // ONE-TIME SETUP: Get or create the global PostgreSQL instance
                let config_and_pool = get_postgres_instance().await;
                
                let mut total_duration = std::time::Duration::new(0, 0);
                
                for _i in 0..iters {
                    // SETUP PHASE (not timed): Clean the database and get connection
                    clean_postgres_tables(&config_and_pool).await.unwrap();
                    let pool = config_and_pool.pool.get().await.unwrap();
                    let conn = MiddlewarePool::get_connection(&pool).await.unwrap();

                    if let MiddlewarePoolConnection::Postgres(mut pgconn) = conn {
                        // START TIMING: The actual benchmarked part (inserts only)
                        let start = std::time::Instant::now();
                        
                        let tx = pgconn.transaction().await.unwrap();
                        tx.batch_execute(&statements).await.unwrap();
                        tx.commit().await.unwrap();
                        
                        // STOP TIMING
                        let elapsed = start.elapsed();
                        total_duration += elapsed;
                    }
                    
                    // No teardown needed - PostgreSQL stays running
                }
                
                total_duration
            }
        });
    });

    group.finish();
}

// Note: LibSQL benchmarks are commented out due to threading issues in benchmark environment
// To enable LibSQL benchmarks when threading issues are resolved, uncomment the lines below:
//
// criterion_group!(libsql_benches, benchmark_libsql);
// criterion_main!(sqlite_benches, libsql_benches, postgres_benches);
//
// Cleanup handler to stop databases on process exit
#[allow(dead_code)] // Used via static _CLEANUP, but compiler doesn't detect this pattern
struct DatabaseCleanup;

impl Drop for DatabaseCleanup {
    fn drop(&mut self) {
        // Clean up PostgreSQL instance
        let mut postgres_guard = POSTGRES_INSTANCE.lock().unwrap();
        if let Some((_, postgres_instance)) = postgres_guard.take() {
            println!("Cleaning up PostgreSQL instance on exit...");
            let rt = Runtime::new().unwrap();
            rt.block_on(async {
                let _ = postgres_instance.postgresql.stop().await;
            });
            println!("PostgreSQL instance stopped.");
        }
        
        // Clean up SQLite file
        let mut sqlite_guard = SQLITE_INSTANCE.lock().unwrap();
        if let Some((_, db_path)) = sqlite_guard.take() {
            println!("Cleaning up SQLite database file on exit...");
            if Path::new(&db_path).exists() {
                if let Err(e) = fs::remove_file(&db_path) {
                    println!("Warning: Failed to remove SQLite file {}: {}", db_path, e);
                } else {
                    println!("SQLite database file removed.");
                }
            }
        }
    }
}

// Static cleanup instance
static _CLEANUP: LazyLock<DatabaseCleanup> = LazyLock::new(|| DatabaseCleanup);

// Current working configuration:
criterion_group!(sqlite_benches, benchmark_sqlite);
criterion_group!(postgres_benches, benchmark_postgres);
criterion_main!(sqlite_benches, postgres_benches);
