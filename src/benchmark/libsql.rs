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
    let mut instance_guard = LIBSQL_INSTANCE.lock().unwrap();
    
    if instance_guard.is_none() {
        println!("Setting up LibSQL instance (one-time setup)...");
        let db_path = "benchmark_libsql_global.db".to_string();
        
        let config_and_pool = setup_libsql_db(&db_path).await.unwrap();
        
        *instance_guard = Some((config_and_pool, db_path));
        println!("LibSQL instance ready!");
    }
    
    let (config_and_pool, _) = instance_guard.as_ref().unwrap();
    config_and_pool.clone()
}

pub async fn clean_libsql_tables(config_and_pool: &ConfigAndPool) -> Result<(), Box<dyn std::error::Error>> {
    let pool = config_and_pool.pool.get().await?;
    let conn = MiddlewarePool::get_connection(&pool).await?;
    
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
    if Path::new(db_path).exists() {
        fs::remove_file(db_path).unwrap();
    }

    let config_and_pool = ConfigAndPool::new_libsql(db_path.to_string()).await?;

    let ddl = "CREATE TABLE IF NOT EXISTS test (
        recid INTEGER PRIMARY KEY AUTOINCREMENT,
        a int, b text, c datetime not null default current_timestamp,
        d real, e boolean, f blob, g json,
        h text, i text, j text, k text, l text, m text, n text, o text, p text
    )";

    let pool = config_and_pool.pool.get().await?;
    let libsql_conn = MiddlewarePool::get_connection(&pool).await?;

    if let MiddlewarePoolConnection::Libsql(libsql_conn) = libsql_conn {
        let tx = libsql_conn.transaction().await.map_err(|e| {
            SqlMiddlewareDbError::ExecutionError(format!("Failed to start transaction: {e}"))
        })?;
        
        tx.execute(ddl, ()).await.map_err(|e| {
            SqlMiddlewareDbError::ExecutionError(format!("Failed to execute DDL: {e}"))
        })?;
        
        tx.commit().await.map_err(|e| {
            SqlMiddlewareDbError::ExecutionError(format!("Failed to commit transaction: {e}"))
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

    group.bench_function(BenchmarkId::new("libsql", format!("{}_rows", num_rows)), |b| {
        let statements = insert_statements.clone();
        b.to_async(&rt).iter_custom(|iters| {
            let statements = statements.clone();
            async move {
                let config_and_pool = get_libsql_instance().await;
                let mut total_duration = std::time::Duration::new(0, 0);
                
                for _i in 0..iters {
                    clean_libsql_tables(&config_and_pool).await.unwrap();
                    let pool = config_and_pool.pool.get().await.unwrap();
                    let libsql_conn = MiddlewarePool::get_connection(&pool).await.unwrap();

                    if let MiddlewarePoolConnection::Libsql(libsql_conn) = libsql_conn {
                        let start = std::time::Instant::now();
                        
                        let tx = libsql_conn.transaction().await.unwrap();
                        tx.execute_batch(&statements).await.unwrap();
                        tx.commit().await.unwrap();
                        
                        let elapsed = start.elapsed();
                        total_duration += elapsed;
                    }
                }
                
                total_duration
            }
        });
    });

    group.finish();
}

pub fn cleanup_libsql() {
    let mut libsql_guard = LIBSQL_INSTANCE.lock().unwrap();
    if let Some((_, db_path)) = libsql_guard.take() {
        println!("Cleaning up LibSQL database file on exit...");
        if Path::new(&db_path).exists() {
            if let Err(e) = fs::remove_file(&db_path) {
                println!("Warning: Failed to remove LibSQL file {}: {}", db_path, e);
            } else {
                println!("LibSQL database file removed.");
            }
        }
    }
}