use criterion::{BenchmarkId, Criterion};
use std::sync::{LazyLock, Mutex};
use std::{fs, path::Path};
use tokio::runtime::Runtime;

use crate::{
    SqlMiddlewareDbError,
    middleware::{ConfigAndPool, MiddlewarePoolConnection},
};

use super::common::{generate_insert_statements, get_benchmark_rows};

static LIBSQL_INSTANCE: LazyLock<Mutex<Option<(ConfigAndPool, String)>>> =
    LazyLock::new(|| Mutex::new(None));

/// Acquire or initialise the shared `LibSQL` benchmark instance.
///
/// # Panics
/// Panics if the global mutex is poisoned or if `LibSQL` initialisation fails.
pub async fn get_libsql_instance() -> ConfigAndPool {
    if let Some((config_and_pool, _)) = LIBSQL_INSTANCE.lock().unwrap().as_ref() {
        println!("get_libsql_instance: Reusing cached instance");
        return config_and_pool.clone();
    }

    println!("get_libsql_instance: Initialising LibSQL instance");
    let db_path = ":memory:".to_string();
    let config_and_pool = setup_libsql_db(&db_path)
        .await
        .expect("Failed to set up LibSQL benchmark database");

    let mut instance_guard = LIBSQL_INSTANCE.lock().unwrap();
    instance_guard.replace((config_and_pool.clone(), db_path));
    println!("get_libsql_instance: LibSQL instance ready");

    config_and_pool
}

/// Reset the `LibSQL` benchmark table to an empty state.
///
/// # Errors
/// Returns any error encountered while acquiring a connection or executing the cleanup SQL.
pub async fn clean_libsql_tables(
    config_and_pool: &ConfigAndPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let conn = config_and_pool.get_connection().await?;

    if let MiddlewarePoolConnection::Libsql {
        conn: libsql_conn, ..
    } = conn
    {
        libsql_conn.execute("DROP TABLE IF EXISTS test", ()).await?;

        let create_sql = "CREATE TABLE IF NOT EXISTS test (
            recid INTEGER PRIMARY KEY AUTOINCREMENT,
            a int, b text, c datetime not null default current_timestamp,
            d real, e boolean, f blob, g json,
            h text, i text, j text, k text, l text, m text, n text, o text, p text
        )";
        libsql_conn.execute(create_sql, ()).await?;
    }
    Ok(())
}

async fn setup_libsql_db(db_path: &str) -> Result<ConfigAndPool, SqlMiddlewareDbError> {
    println!("setup_libsql_db: Starting with path: {db_path}");
    if db_path != ":memory:"
        && Path::new(db_path).exists()
        && let Err(err) = fs::remove_file(db_path)
    {
        println!("setup_libsql_db: Warning – failed to remove existing file {db_path}: {err}");
    }

    println!("setup_libsql_db: Creating new ConfigAndPool");
    let config_and_pool = ConfigAndPool::libsql_builder(db_path.to_string())
        .build()
        .await?;
    println!("setup_libsql_db: ConfigAndPool::new_libsql completed");

    let ddl = "CREATE TABLE IF NOT EXISTS test (
        recid INTEGER PRIMARY KEY AUTOINCREMENT,
        a int, b text, c datetime not null default current_timestamp,
        d real, e boolean, f blob, g json,
        h text, i text, j text, k text, l text, m text, n text, o text, p text
    )";

    let libsql_conn = config_and_pool.get_connection().await?;

    if let MiddlewarePoolConnection::Libsql {
        conn: libsql_conn, ..
    } = libsql_conn
    {
        libsql_conn.execute(ddl, ()).await.map_err(|error| {
            SqlMiddlewareDbError::ExecutionError(format!("Failed to execute DDL: {error}"))
        })?;
    } else {
        panic!("Expected LibSQL connection");
    }

    Ok(config_and_pool)
}

/// Benchmark insert throughput for the `LibSQL` backend.
///
/// # Panics
/// Panics if the Tokio runtime cannot be created or if any benchmark iteration fails.
pub fn benchmark_libsql(c: &mut Criterion) {
    let rt = Runtime::new().expect("Tokio runtime creation failed for LibSQL benchmarks");
    let num_rows = get_benchmark_rows();
    println!("Running LibSQL benchmark with {num_rows} rows");
    let insert_statements = generate_insert_statements(num_rows);
    let mut group = c.benchmark_group("database_inserts");

    group.bench_function(
        BenchmarkId::new("libsql", format!("{num_rows}_rows")),
        |b| {
            let statements = insert_statements.clone();
            b.to_async(&rt).iter_custom(|iters| {
                let statements = statements.clone();
                async move {
                    let config_and_pool = get_libsql_instance().await;
                    let mut total_duration = std::time::Duration::default();

                    for _ in 0..iters {
                        clean_libsql_tables(&config_and_pool)
                            .await
                            .expect("Failed to reset LibSQL tables");
                        let libsql_conn = config_and_pool
                            .get_connection()
                            .await
                            .expect("Failed to get conn");

                        if let MiddlewarePoolConnection::Libsql {
                            conn: libsql_conn, ..
                        } = libsql_conn
                        {
                            let start = std::time::Instant::now();

                            crate::libsql::execute_batch(&libsql_conn, &statements)
                                .await
                                .expect("LibSQL batch execution failed");

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

/// Tear down the shared `LibSQL` benchmark instance.
///
/// # Panics
/// Panics if the global instance mutex is poisoned.
pub fn cleanup_libsql() {
    let mut libsql_guard = LIBSQL_INSTANCE.lock().unwrap();
    if let Some((_, db_path)) = libsql_guard.take() {
        if db_path == ":memory:" {
            println!("LibSQL in-memory database cleaned up.");
        } else {
            println!("Cleaning up LibSQL database file on exit...");
            if Path::new(&db_path).exists() {
                if let Err(err) = fs::remove_file(&db_path) {
                    println!("Warning: Failed to remove LibSQL file {db_path}: {err}");
                } else {
                    println!("LibSQL database file removed.");
                }
            }
        }
    }
}
