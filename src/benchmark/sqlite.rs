use criterion::{BenchmarkId, Criterion};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};
use std::{fs, path::Path};
use tokio::runtime::Runtime;

use crate::{
    SqlMiddlewareDbError,
    middleware::{ConfigAndPool, MiddlewarePool, MiddlewarePoolConnection},
};

use super::common::{generate_insert_statements, get_benchmark_rows};

static SQLITE_INSTANCE: LazyLock<Mutex<Option<(ConfigAndPool, String)>>> =
    LazyLock::new(|| Mutex::new(None));

/// Acquire or initialise the shared `SQLite` benchmark instance.
///
/// # Panics
/// Panics if the global mutex is poisoned or if `SQLite` initialisation fails.
pub async fn get_sqlite_instance() -> ConfigAndPool {
    if let Some((config_and_pool, _)) = SQLITE_INSTANCE.lock().unwrap().as_ref() {
        println!("get_sqlite_instance: Reusing cached instance");
        return config_and_pool.clone();
    }

    println!("get_sqlite_instance: Initialising SQLite instance");
    let db_path = String::from("benchmark_sqlite_global.db");
    let config_and_pool = setup_sqlite_db(&db_path)
        .await
        .expect("Failed to set up SQLite benchmark database");

    let mut instance_guard = SQLITE_INSTANCE.lock().unwrap();
    instance_guard.replace((config_and_pool.clone(), db_path));
    println!("get_sqlite_instance: SQLite instance ready");

    config_and_pool
}

/// Reset the `SQLite` benchmark table to an empty state.
///
/// # Errors
/// Returns any error encountered while acquiring a connection or executing the cleanup SQL.
pub async fn clean_sqlite_tables(
    config_and_pool: &ConfigAndPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let pool = config_and_pool.pool.get().await?;
    let mut conn = MiddlewarePool::get_connection(pool).await?;

    if matches!(&conn, MiddlewarePoolConnection::Sqlite(_)) {
        conn.with_sqlite_connection(move |connection| {
            connection.execute("DROP TABLE IF EXISTS test", [])?;

            let create_sql = "CREATE TABLE IF NOT EXISTS test (
            recid INTEGER PRIMARY KEY AUTOINCREMENT,
            a int, b text, c datetime not null default current_timestamp,
            d real, e boolean, f blob, g json,
            h text, i text, j text, k text, l text, m text, n text, o text, p text
        )";
            connection.execute(create_sql, [])?;

            Ok::<_, SqlMiddlewareDbError>(())
        })
        .await?;
    }
    Ok(())
}

/// Start a dedicated `SQLite` database file for benchmarking.
///
/// # Errors
/// Propagates any error that occurs while preparing the database file or executing the schema
/// initialisation SQL.
async fn setup_sqlite_db(db_path: &str) -> Result<ConfigAndPool, SqlMiddlewareDbError> {
    if Path::new(db_path).exists()
        && let Err(err) = fs::remove_file(db_path)
    {
        println!("setup_sqlite_db: Warning – failed to remove existing file {db_path}: {err}");
    }

    let config_and_pool = ConfigAndPool::new_sqlite(db_path.to_string()).await?;

    let ddl = "CREATE TABLE IF NOT EXISTS test (
        recid INTEGER PRIMARY KEY AUTOINCREMENT,
        a int, b text, c datetime not null default current_timestamp,
        d real, e boolean, f blob, g json,
        h text, i text, j text, k text, l text, m text, n text, o text, p text
    )";

    let pool = config_and_pool.pool.get().await?;
    let mut sqlite_conn = MiddlewarePool::get_connection(pool).await?;

    if matches!(&sqlite_conn, MiddlewarePoolConnection::Sqlite(_)) {
        sqlite_conn
            .with_sqlite_connection(move |connection| {
                let transaction = connection.transaction()?;
                transaction.execute_batch(ddl)?;
                transaction.commit()?;
                Ok::<_, SqlMiddlewareDbError>(())
            })
            .await?;
    } else {
        panic!("Expected SQLite connection");
    }

    Ok(config_and_pool)
}

/// Benchmark insert throughput for the `SQLite` backend.
///
/// # Panics
/// Panics if any benchmark iteration fails.
pub fn benchmark_sqlite(c: &mut Criterion, runtime: &Runtime) {
    let num_rows = get_benchmark_rows();
    println!("Running SQLite benchmark with {num_rows} rows");
    let insert_statements = generate_insert_statements(num_rows);
    let mut group = c.benchmark_group("database_inserts");

    group.bench_function(
        BenchmarkId::new("sqlite", format!("{num_rows}_rows")),
        |b| {
            let statements = insert_statements.clone();
            b.to_async(runtime).iter_custom(|iters| {
                let statements = statements.clone();
                async move {
                    let config_and_pool = get_sqlite_instance().await;
                    let mut total_duration = Duration::default();

                    for _ in 0..iters {
                        clean_sqlite_tables(&config_and_pool)
                            .await
                            .expect("Failed to reset SQLite tables");
                        let pool = config_and_pool
                            .pool
                            .get()
                            .await
                            .expect("Failed to get pool");
                        let mut sqlite_conn = MiddlewarePool::get_connection(pool)
                            .await
                            .expect("Failed to get conn");

                        if matches!(&sqlite_conn, MiddlewarePoolConnection::Sqlite(_)) {
                            let statements_clone = statements.clone();
                            let start = Instant::now();

                            sqlite_conn
                                .with_sqlite_connection(move |connection| {
                                    let transaction = connection.transaction()?;
                                    transaction.execute_batch(&statements_clone)?;
                                    transaction.commit()?;
                                    Ok::<_, SqlMiddlewareDbError>(())
                                })
                                .await
                                .expect("SQLite worker task failed");

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

/// Tear down the shared `SQLite` benchmark instance.
///
/// # Panics
/// Panics if the global mutex is poisoned.
pub fn cleanup_sqlite() {
    let mut sqlite_guard = SQLITE_INSTANCE.lock().unwrap();
    if let Some((_, db_path)) = sqlite_guard.take()
        && Path::new(&db_path).exists()
    {
        println!("Cleaning up SQLite database file on exit...");
        match fs::remove_file(&db_path) {
            Ok(()) => println!("SQLite database file removed."),
            Err(err) => println!("Warning: Failed to remove SQLite file {db_path}: {err}"),
        }
    }
}
