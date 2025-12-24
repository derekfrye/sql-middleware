use criterion::{BenchmarkId, Criterion};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};
use tokio::runtime::Runtime;

use crate::middleware::{ConfigAndPool, MiddlewarePoolConnection, PgConfig, PostgresOptions};

use super::common::{generate_postgres_insert_statements, get_benchmark_rows};

static POSTGRES_INSTANCE: LazyLock<Mutex<Option<ConfigAndPool>>> =
    LazyLock::new(|| Mutex::new(None));

/// Acquire or initialise the shared `PostgreSQL` benchmark instance.
///
/// # Panics
/// Panics if the global mutex is poisoned or if the embedded `PostgreSQL` setup fails.
pub async fn get_postgres_instance() -> ConfigAndPool {
    if let Some(config_and_pool) = POSTGRES_INSTANCE.lock().unwrap().as_ref() {
        println!("get_postgres_instance: Reusing cached instance");
        return config_and_pool.clone();
    }

    println!("get_postgres_instance: Initialising PostgreSQL instance");
    let config_and_pool = setup_postgres_db()
        .await
        .expect("Failed to initialise PostgreSQL connection");

    let mut instance_guard = POSTGRES_INSTANCE.lock().unwrap();
    instance_guard.replace(config_and_pool.clone());
    println!("get_postgres_instance: PostgreSQL instance ready");

    config_and_pool
}

/// Reset the `PostgreSQL` benchmark table to an empty state.
///
/// # Errors
/// Returns any error encountered while acquiring a connection or executing the cleanup SQL.
pub async fn clean_postgres_tables(
    config_and_pool: &ConfigAndPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let conn = config_and_pool.get_connection().await?;

    if let MiddlewarePoolConnection::Postgres { client: pgconn, .. } = conn {
        pgconn.execute("DROP TABLE IF EXISTS test", &[]).await?;

        let create_sql = "CREATE TABLE IF NOT EXISTS test (
            recid SERIAL PRIMARY KEY,
            a int, b text, c timestamp not null default now(),
            d real, e boolean, f bytea, g jsonb,
            h text, i text, j text, k text, l text, m text, n text, o text, p text
        )";
        pgconn.execute(create_sql, &[]).await?;
    }
    Ok(())
}

/// Connect to PostgreSQL and return a ready-to-use pool.
///
/// # Errors
/// Propagates any error that occurs while connecting to the database or
/// executing the schema initialisation SQL.
async fn setup_postgres_db() -> Result<ConfigAndPool, Box<dyn std::error::Error>> {
    let mut cfg = PgConfig::new();
    cfg.dbname = Some("testing".to_string());
    cfg.host = Some("10.3.0.201".to_string());
    cfg.port = Some(5432);
    cfg.user = Some("testuser".to_string());
    cfg.password = Some(std::env::var("TESTING_PG_PASSWORD").unwrap_or_default());

    let config_and_pool = ConfigAndPool::new_postgres(PostgresOptions::new(cfg)).await?;
    clean_postgres_tables(&config_and_pool).await?;

    Ok(config_and_pool)
}

/// Benchmark insert throughput for the `PostgreSQL` backend.
///
/// # Panics
/// Panics if any benchmark iteration fails.
pub fn benchmark_postgres(c: &mut Criterion, runtime: &Runtime) {
    let num_rows = get_benchmark_rows();
    println!("Running PostgreSQL benchmark with {num_rows} rows");
    let postgres_insert_statements = generate_postgres_insert_statements(num_rows);
    let mut group = c.benchmark_group("database_inserts");

    group.bench_function(
        BenchmarkId::new("postgres", format!("{num_rows}_rows")),
        |b| {
            let statements = postgres_insert_statements.clone();
            b.to_async(runtime).iter_custom(|iters| {
                let statements = statements.clone();
                async move {
                    let config_and_pool = get_postgres_instance().await;
                    let mut total_duration = Duration::default();

                    for _ in 0..iters {
                        clean_postgres_tables(&config_and_pool)
                            .await
                            .expect("Failed to reset PostgreSQL tables");
                        let conn = config_and_pool
                            .get_connection()
                            .await
                            .expect("Failed to get conn");

                        if let MiddlewarePoolConnection::Postgres {
                            client: mut pgconn, ..
                        } = conn
                        {
                            let start = Instant::now();

                            let tx = pgconn.transaction().await.expect("Failed to open tx");
                            tx.batch_execute(&statements)
                                .await
                                .expect("Failed to run benchmark statements");
                            tx.commit().await.expect("Failed to commit benchmark tx");

                            total_duration += start.elapsed();
                        }
                    }

                    total_duration
                }
            });
        },
    );

    group.finish();
}

/// Tear down the shared `PostgreSQL` benchmark instance.
///
/// # Panics
/// Panics if the global mutex is poisoned or if the Tokio runtime cannot be created.
pub fn cleanup_postgres() {
    let mut postgres_guard = POSTGRES_INSTANCE.lock().unwrap();
    if postgres_guard.take().is_some() {
        println!("Cleaning up PostgreSQL benchmark instance on exit...");
    }
}
