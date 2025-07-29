use criterion::{BenchmarkId, Criterion};
use std::sync::{LazyLock, Mutex};
use std::{fs, path::Path};
use tokio::runtime::Runtime;

use crate::{
    SqlMiddlewareDbError,
    middleware::{ConfigAndPool, MiddlewarePool, MiddlewarePoolConnection},
};

use super::common::{generate_insert_statements, get_benchmark_rows};

static SQLITE_INSTANCE: LazyLock<Mutex<Option<(ConfigAndPool, String)>>> = 
    LazyLock::new(|| Mutex::new(None));

pub async fn get_sqlite_instance() -> ConfigAndPool {
    let mut instance_guard = SQLITE_INSTANCE.lock().unwrap();
    
    if instance_guard.is_none() {
        println!("Setting up SQLite instance (one-time setup)...");
        let db_path = "benchmark_sqlite_global.db".to_string();
        
        let config_and_pool = setup_sqlite_db(&db_path).await.unwrap();
        
        *instance_guard = Some((config_and_pool, db_path));
        println!("SQLite instance ready!");
    }
    
    let (config_and_pool, _) = instance_guard.as_ref().unwrap();
    config_and_pool.clone()
}

pub async fn clean_sqlite_tables(config_and_pool: &ConfigAndPool) -> Result<(), Box<dyn std::error::Error>> {
    let pool = config_and_pool.pool.get().await?;
    let conn = MiddlewarePool::get_connection(&pool).await?;
    
    if let MiddlewarePoolConnection::Sqlite(sconn) = conn {
        sconn.interact(move |conn| {
            conn.execute("DROP TABLE IF EXISTS test", [])?;
            
            let create_sql = "CREATE TABLE IF NOT EXISTS test (
                recid INTEGER PRIMARY KEY AUTOINCREMENT,
                a int, b text, c datetime not null default current_timestamp,
                d real, e boolean, f blob, g json,
                h text, i text, j text, k text, l text, m text, n text, o text, p text
            )";
            conn.execute(create_sql, [])?;
            
            Ok::<_, SqlMiddlewareDbError>(())
        }).await??;
    }
    Ok(())
}

async fn setup_sqlite_db(db_path: &str) -> Result<ConfigAndPool, SqlMiddlewareDbError> {
    if Path::new(db_path).exists() {
        fs::remove_file(db_path).unwrap();
    }

    let config_and_pool = ConfigAndPool::new_sqlite(db_path.to_string()).await?;

    let ddl = "CREATE TABLE IF NOT EXISTS test (
        recid INTEGER PRIMARY KEY AUTOINCREMENT,
        a int, b text, c datetime not null default current_timestamp,
        d real, e boolean, f blob, g json,
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

pub fn benchmark_sqlite(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let num_rows = get_benchmark_rows();
    println!("Running SQLite benchmark with {} rows", num_rows);
    let insert_statements = generate_insert_statements(num_rows);
    let mut group = c.benchmark_group("database_inserts");

    group.bench_function(BenchmarkId::new("sqlite", format!("{}_rows", num_rows)), |b| {
        let statements = insert_statements.clone();
        b.to_async(&rt).iter_custom(|iters| {
            let statements = statements.clone();
            async move {
                let config_and_pool = get_sqlite_instance().await;
                let mut total_duration = std::time::Duration::new(0, 0);
                
                for _i in 0..iters {
                    clean_sqlite_tables(&config_and_pool).await.unwrap();
                    let pool = config_and_pool.pool.get().await.unwrap();
                    let sqlite_conn = MiddlewarePool::get_connection(&pool).await.unwrap();

                    if let MiddlewarePoolConnection::Sqlite(sconn) = sqlite_conn {
                        let insert_statements_copy = statements.clone();
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

pub fn cleanup_sqlite() {
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