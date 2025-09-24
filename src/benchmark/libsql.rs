use criterion::{BenchmarkId, Criterion};
use std::sync::{LazyLock, Mutex};
use std::{fs, path::Path};
use tokio::runtime::Runtime;

use crate::{
    SqlMiddlewareDbError,
    middleware::{ConfigAndPool, MiddlewarePool, MiddlewarePoolConnection},
};

use super::common::{generate_insert_statements, get_benchmark_rows};

static LIBSQL_INSTANCE: LazyLock<Mutex<Option<(ConfigAndPool, String)>>> =
    LazyLock::new(|| Mutex::new(None));

pub async fn get_libsql_instance() -> ConfigAndPool {
    println!("get_libsql_instance: Starting...");
    let mut instance_guard = LIBSQL_INSTANCE.lock().unwrap();
    println!("get_libsql_instance: Got mutex lock");

    if instance_guard.is_none() {
        println!("Setting up LibSQL instance (one-time setup)...");
        let db_path = ":memory:".to_string();

        println!("get_libsql_instance: About to call setup_libsql_db");
        let config_and_pool = setup_libsql_db(&db_path).await.unwrap();
        println!("get_libsql_instance: setup_libsql_db completed");

        *instance_guard = Some((config_and_pool, db_path));
        println!("LibSQL instance ready!");
    }

    let (config_and_pool, _) = instance_guard.as_ref().unwrap();
    println!("get_libsql_instance: Returning config_and_pool");
    config_and_pool.clone()
}

pub async fn clean_libsql_tables(
    config_and_pool: &ConfigAndPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let pool = config_and_pool.pool.get().await?;
    let conn = MiddlewarePool::get_connection(pool).await?;

    if let MiddlewarePoolConnection::Libsql(libsql_conn) = conn {
        // Drop and recreate the table
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
    println!("setup_libsql_db: Starting with path: {}", db_path);
    // Skip file removal for in-memory databases
    if db_path != ":memory:" && Path::new(db_path).exists() {
        fs::remove_file(db_path).unwrap();
    }

    println!("setup_libsql_db: About to call ConfigAndPool::new_libsql");
    let config_and_pool = ConfigAndPool::new_libsql(db_path.to_string()).await?;
    println!("setup_libsql_db: ConfigAndPool::new_libsql completed");

    let ddl = "CREATE TABLE IF NOT EXISTS test (
        recid INTEGER PRIMARY KEY AUTOINCREMENT,
        a int, b text, c datetime not null default current_timestamp,
        d real, e boolean, f blob, g json,
        h text, i text, j text, k text, l text, m text, n text, o text, p text
    )";

    let pool = config_and_pool.pool.get().await?;
    let libsql_conn = MiddlewarePool::get_connection(pool).await?;

    if let MiddlewarePoolConnection::Libsql(libsql_conn) = libsql_conn {
        libsql_conn.execute(ddl, ()).await.map_err(|e| {
            SqlMiddlewareDbError::ExecutionError(format!("Failed to execute DDL: {e}"))
        })?;
    } else {
        panic!("Expected LibSQL connection");
    }

    Ok(config_and_pool)
}

pub fn benchmark_libsql(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let num_rows = get_benchmark_rows();
    println!("Running LibSQL benchmark with {} rows", num_rows);
    let insert_statements = generate_insert_statements(num_rows);
    let mut group = c.benchmark_group("database_inserts");

    group.bench_function(
        BenchmarkId::new("libsql", format!("{}_rows", num_rows)),
        |b| {
            let statements = insert_statements.clone();
            b.to_async(&rt).iter_custom(|iters| {
                let statements = statements.clone();
                async move {
                    let config_and_pool = get_libsql_instance().await;
                    let mut total_duration = std::time::Duration::new(0, 0);

                    for _i in 0..iters {
                        clean_libsql_tables(&config_and_pool).await.unwrap();
                        let pool = config_and_pool.pool.get().await.unwrap();
                        let libsql_conn = MiddlewarePool::get_connection(pool).await.unwrap();

                        if let MiddlewarePoolConnection::Libsql(libsql_conn) = libsql_conn {
                            let start = std::time::Instant::now();

                            // Use the libsql executor function directly
                            crate::libsql::execute_batch(&libsql_conn, &statements)
                                .await
                                .unwrap();

                            let elapsed = start.elapsed();
                            total_duration += elapsed;
                        }
                    }

                    total_duration
                }
            });
        },
    );

    group.finish();
}

pub fn cleanup_libsql() {
    let mut libsql_guard = LIBSQL_INSTANCE.lock().unwrap();
    if let Some((_, db_path)) = libsql_guard.take() {
        // Skip cleanup for in-memory databases
        if db_path != ":memory:" {
            println!("Cleaning up LibSQL database file on exit...");
            if Path::new(&db_path).exists() {
                if let Err(e) = fs::remove_file(&db_path) {
                    println!("Warning: Failed to remove LibSQL file {}: {}", db_path, e);
                } else {
                    println!("LibSQL database file removed.");
                }
            }
        } else {
            println!("LibSQL in-memory database cleaned up.");
        }
    }
}
