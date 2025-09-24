use criterion::{BenchmarkId, Criterion};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};
use tokio::runtime::Runtime;

use crate::middleware::{ConfigAndPool, MiddlewarePool, MiddlewarePoolConnection};
#[cfg(feature = "test-utils")]
use crate::test_utils::postgres::EmbeddedPostgres;
#[cfg(feature = "test-utils")]
use postgresql_embedded::PostgreSQL;

use super::common::{generate_postgres_insert_statements, get_benchmark_rows};

static POSTGRES_INSTANCE: LazyLock<Mutex<Option<(ConfigAndPool, EmbeddedPostgres)>>> =
    LazyLock::new(|| Mutex::new(None));

/// Acquire or initialise the shared `PostgreSQL` benchmark instance.
///
/// # Panics
/// Panics if the global mutex is poisoned or if the embedded `PostgreSQL` setup fails.
pub async fn get_postgres_instance() -> ConfigAndPool {
    if let Some((config_and_pool, _)) = POSTGRES_INSTANCE.lock().unwrap().as_ref() {
        println!("get_postgres_instance: Reusing cached instance");
        return config_and_pool.clone();
    }

    println!("get_postgres_instance: Initialising PostgreSQL instance");
    let db_user = "test_user";
    let db_pass = "test_password123";
    let db_name = "test_db";

    let (config_and_pool, postgres_instance) = setup_postgres_db(db_user, db_pass, db_name)
        .await
        .expect("Failed to initialise embedded PostgreSQL");

    let mut instance_guard = POSTGRES_INSTANCE.lock().unwrap();
    instance_guard.replace((config_and_pool.clone(), postgres_instance));
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
    let pool = config_and_pool.pool.get().await?;
    let conn = MiddlewarePool::get_connection(pool).await?;

    if let MiddlewarePoolConnection::Postgres(pgconn) = conn {
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

/// Start an embedded `PostgreSQL` instance and return a ready-to-use pool.
///
/// # Errors
/// Propagates any error that occurs while preparing the embedded database, creating users,
/// or executing the schema initialisation SQL.
#[cfg(feature = "test-utils")]
async fn setup_postgres_db(
    db_user: &str,
    db_pass: &str,
    db_name: &str,
) -> Result<(ConfigAndPool, EmbeddedPostgres), Box<dyn std::error::Error>> {
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

    let (final_user, final_password) = if db_user == embedded_user {
        (embedded_user, embedded_password)
    } else {
        let mut admin_cfg = cfg.clone();
        admin_cfg.port = Some(port);
        admin_cfg.host = Some(host.clone());
        admin_cfg.user = Some(embedded_user.clone());
        admin_cfg.password = Some(embedded_password.clone());
        admin_cfg.dbname = Some(String::from("postgres"));

        let admin_pool = ConfigAndPool::new_postgres(admin_cfg).await?;
        let pool = admin_pool.pool.get().await?;
        let admin_conn = MiddlewarePool::get_connection(pool).await?;

        if let MiddlewarePoolConnection::Postgres(pgconn) = admin_conn {
            let create_user_sql =
                format!("CREATE USER \"{db_user}\" WITH PASSWORD '{db_pass}' CREATEDB SUPERUSER");
            pgconn
                .execute(&create_user_sql, &[])
                .await
                .map_err(|error| format!("Failed to create user {db_user}: {error}"))?;
        }

        (db_user.to_string(), db_pass.to_string())
    };

    let database_url = format!("postgres://{final_user}:{final_password}@{host}:{port}/{db_name}");

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
    let conn = MiddlewarePool::get_connection(pool).await?;

    if let MiddlewarePoolConnection::Postgres(mut pgconn) = conn {
        let tx = pgconn.transaction().await?;
        tx.batch_execute(ddl).await?;
        tx.commit().await?;
    } else {
        panic!("Expected PostgreSQL connection");
    }

    Ok((config_and_pool, postgres_instance))
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
                        let pool = config_and_pool
                            .pool
                            .get()
                            .await
                            .expect("Failed to get pool");
                        let conn = MiddlewarePool::get_connection(pool)
                            .await
                            .expect("Failed to get conn");

                        if let MiddlewarePoolConnection::Postgres(mut pgconn) = conn {
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
    if let Some((_, postgres_instance)) = postgres_guard.take() {
        println!("Cleaning up PostgreSQL instance on exit...");
        let runtime = Runtime::new().expect("Tokio runtime creation failed for cleanup");
        runtime.block_on(async {
            let _ = postgres_instance.postgresql.stop().await;
        });
        println!("PostgreSQL instance stopped.");
    }
}
