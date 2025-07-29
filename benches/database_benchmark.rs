use criterion::{criterion_group, criterion_main};
use std::sync::LazyLock;

mod common;

// SQLite module
mod sqlite {
    use criterion::{BenchmarkId, Criterion};
    use std::sync::{LazyLock, Mutex};
    use std::{fs, path::Path};
    use tokio::runtime::Runtime;

    use sql_middleware::{
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
}

// Postgres module
mod postgres {
    use criterion::{BenchmarkId, Criterion};
    use std::sync::{LazyLock, Mutex};
    use tokio::runtime::Runtime;

    #[cfg(feature = "test-utils")]
    use sql_middleware::test_utils::postgres::EmbeddedPostgres;
    use sql_middleware::middleware::{ConfigAndPool, MiddlewarePool, MiddlewarePoolConnection};

    use super::common::{generate_postgres_insert_statements, get_benchmark_rows};

    static POSTGRES_INSTANCE: LazyLock<Mutex<Option<(ConfigAndPool, EmbeddedPostgres)>>> = 
        LazyLock::new(|| Mutex::new(None));

    pub async fn get_postgres_instance() -> ConfigAndPool {
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
        
        let (config_and_pool, _) = instance_guard.as_ref().unwrap();
        config_and_pool.clone()
    }

    pub async fn clean_postgres_tables(config_and_pool: &ConfigAndPool) -> Result<(), Box<dyn std::error::Error>> {
        let pool = config_and_pool.pool.get().await?;
        let conn = MiddlewarePool::get_connection(&pool).await?;
        
        if let MiddlewarePoolConnection::Postgres(pgconn) = conn {
            let cleanup_sql = "DROP TABLE IF EXISTS test";
            pgconn.execute(cleanup_sql, &[]).await?;
            
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
        
        let mut postgresql = PostgreSQL::default();
        postgresql.setup().await?;
        postgresql.start().await?;
        
        let port = postgresql.settings().port;
        let host = postgresql.settings().host.clone();
        let embedded_user = postgresql.settings().username.clone();
        let embedded_password = postgresql.settings().password.clone();
        
        postgresql.create_database(db_name).await?;
        
        let (final_user, final_password) = if db_user != &embedded_user {
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

        let ddl = "CREATE TABLE IF NOT EXISTS test (
            recid SERIAL PRIMARY KEY,
            a int, b text, c timestamp not null default now(),
            d real, e boolean, f bytea, g jsonb,
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

    pub fn benchmark_postgres(c: &mut Criterion) {
        let rt = Runtime::new().unwrap();
        let num_rows = get_benchmark_rows();
        println!("Running PostgreSQL benchmark with {} rows", num_rows);
        let postgres_insert_statements = generate_postgres_insert_statements(num_rows);
        let mut group = c.benchmark_group("database_inserts");

        group.bench_function(BenchmarkId::new("postgres", format!("{}_rows", num_rows)), |b| {
            let statements = postgres_insert_statements.clone();
            b.to_async(&rt).iter_custom(|iters| {
                let statements = statements.clone();
                async move {
                    let config_and_pool = get_postgres_instance().await;
                    let mut total_duration = std::time::Duration::new(0, 0);
                    
                    for _i in 0..iters {
                        clean_postgres_tables(&config_and_pool).await.unwrap();
                        let pool = config_and_pool.pool.get().await.unwrap();
                        let conn = MiddlewarePool::get_connection(&pool).await.unwrap();

                        if let MiddlewarePoolConnection::Postgres(mut pgconn) = conn {
                            let start = std::time::Instant::now();
                            
                            let tx = pgconn.transaction().await.unwrap();
                            tx.batch_execute(&statements).await.unwrap();
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

    pub fn cleanup_postgres() {
        let mut postgres_guard = POSTGRES_INSTANCE.lock().unwrap();
        if let Some((_, postgres_instance)) = postgres_guard.take() {
            println!("Cleaning up PostgreSQL instance on exit...");
            let rt = Runtime::new().unwrap();
            rt.block_on(async {
                let _ = postgres_instance.postgresql.stop().await;
            });
            println!("PostgreSQL instance stopped.");
        }
    }
}

use sqlite::{benchmark_sqlite, cleanup_sqlite};
use postgres::{benchmark_postgres, cleanup_postgres};

#[allow(dead_code)]
struct DatabaseCleanup;

impl Drop for DatabaseCleanup {
    fn drop(&mut self) {
        cleanup_postgres();
        cleanup_sqlite();
    }
}

static _CLEANUP: LazyLock<DatabaseCleanup> = LazyLock::new(|| DatabaseCleanup);

criterion_group!(sqlite_benches, benchmark_sqlite);
criterion_group!(postgres_benches, benchmark_postgres);
criterion_main!(sqlite_benches, postgres_benches);