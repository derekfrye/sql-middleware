#![cfg(any(feature = "sqlite", feature = "postgres", feature = "turso"))]
#[path = "custom_logic_between_txn/backend_tx.rs"]
mod backend_tx;

use std::env;

use sql_middleware::middleware::{
    ConfigAndPool, MiddlewarePoolConnection, PgConfig, RowValues, SqlMiddlewareDbError,
};
use tokio::runtime::Runtime;

#[cfg(feature = "postgres")]
use sql_middleware::postgres::PostgresOptions;
#[cfg(feature = "postgres")]
use sql_middleware::typed_postgres::{Idle as PgIdle, PgConnection, PgManager};

#[cfg(feature = "postgres")]
fn postgres_config() -> PgConfig {
    let mut cfg = PgConfig::new();
    cfg.dbname = Some("testing".to_string());
    cfg.host = Some("10.3.0.201".to_string());
    cfg.port = Some(5432);
    cfg.user = Some("testuser".to_string());
    // Trust auth in CI; allow override when a password is required.
    cfg.password = Some(env::var("TESTING_PG_PASSWORD").unwrap_or_default());
    cfg
}

async fn run_roundtrip(conn: &mut MiddlewarePoolConnection) -> Result<(), SqlMiddlewareDbError> {
    // Shared query authored once; translated for SQLite-family backends.
    let insert_query = "INSERT INTO custom_logic_txn (id, note) VALUES ($1, $2)";

    // Success path should commit.
    {
        backend_tx::execute_with_finalize(
            conn,
            insert_query,
            vec![RowValues::Int(1), RowValues::Text("ok".into())],
        )
        .await?;
    }

    // Duplicate insert should roll back and propagate the error.
    {
        let res = backend_tx::execute_with_finalize(
            conn,
            insert_query,
            vec![RowValues::Int(1), RowValues::Text("dup".into())],
        )
        .await;
        assert!(res.is_err(), "expected duplicate key to fail");
    }

    // Verify only the committed row exists.
    let rs = conn
        .query("SELECT COUNT(*) AS cnt FROM custom_logic_txn")
        .select()
        .await?;
    let count = *rs.results[0].get("cnt").unwrap().as_int().unwrap();
    assert_eq!(count, 1);
    Ok(())
}

#[cfg(all(feature = "postgres", feature = "postgres"))]
async fn run_typed_pg_roundtrip(
    mut conn: PgConnection<PgIdle>,
) -> Result<(), SqlMiddlewareDbError> {
    conn.execute_batch(
        "DROP TABLE IF EXISTS custom_logic_txn;
         CREATE TABLE custom_logic_txn (id BIGINT PRIMARY KEY, note TEXT);",
    )
    .await?;

    // Success path should commit.
    {
        let mut tx = conn.begin().await?;
        let rows = tx
            .dml(
                "INSERT INTO custom_logic_txn (id, note) VALUES ($1, $2)",
                &[RowValues::Int(1), RowValues::Text("ok".into())],
            )
            .await?;
        assert_eq!(rows, 1);
        conn = tx.commit().await?;
    }

    // Duplicate insert should roll back and propagate the error.
    {
        let mut tx = conn.begin().await?;
        let res = tx
            .dml(
                "INSERT INTO custom_logic_txn (id, note) VALUES ($1, $2)",
                &[RowValues::Int(1), RowValues::Text("dup".into())],
            )
            .await;
        assert!(res.is_err(), "expected duplicate key to fail");
        conn = tx.rollback().await?;
    }

    // Verify only the committed row exists.
    let rs = conn
        .select("SELECT COUNT(*) AS cnt FROM custom_logic_txn", &[])
        .await?;
    let count = *rs.results[0].get("cnt").unwrap().as_int().unwrap();
    assert_eq!(count, 1);

    conn.execute_batch("DROP TABLE IF EXISTS custom_logic_txn;")
        .await?;
    Ok(())
}

#[test]
fn custom_logic_between_transactions_across_backends() -> Result<(), Box<dyn std::error::Error>> {
    let rt = Runtime::new()?;
    rt.block_on(async {
        // SQLite (always available in default feature set)
        #[cfg(feature = "sqlite")]
        {
            let cap = ConfigAndPool::sqlite_builder("file::memory:?cache=shared".to_string())
                .build()
                .await?;
            let mut conn = cap.get_connection().await?;
            conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS custom_logic_txn (id INTEGER PRIMARY KEY, note TEXT);",
            )
            .await?;
            run_roundtrip(&mut conn).await?;
            println!("sqlite backend run successful");
        }

        // Postgres using the same auth flow as test06_*.
        #[cfg(feature = "postgres")]
        {
            let cfg = postgres_config();
            let cap = ConfigAndPool::new_postgres(PostgresOptions::new(cfg)).await?;
            let mut conn = cap.get_connection().await?;
            conn.execute_batch(
                "DROP TABLE IF EXISTS custom_logic_txn;
                 CREATE TABLE custom_logic_txn (id BIGINT PRIMARY KEY, note TEXT);",
            )
            .await?;
            run_roundtrip(&mut conn).await?;
            conn.execute_batch("DROP TABLE IF EXISTS custom_logic_txn;")
                .await?;
            println!("postgres backend run successful");
        }

        // Typed Postgres mirror of the same flow.
        #[cfg(all(feature = "postgres", feature = "postgres"))]
        {
            let cfg = postgres_config();
            let pool = PgManager::new(cfg.to_tokio_config()).build_pool().await?;
            let typed_conn: PgConnection<PgIdle> = PgConnection::from_pool(&pool).await?;
            run_typed_pg_roundtrip(typed_conn).await?;
            println!("typed-postgres backend run successful");
        }

        // Turso (optional feature)
        #[cfg(feature = "turso")]
        {
            let cap = ConfigAndPool::turso_builder(":memory:".to_string())
                .build()
                .await?;
            let mut conn = cap.get_connection().await?;
            conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS custom_logic_txn (id INTEGER PRIMARY KEY, note TEXT);",
            )
            .await?;
            run_roundtrip(&mut conn).await?;
            println!("turso backend run successful");
        }

        Ok::<(), SqlMiddlewareDbError>(())
    })?;

    Ok(())
}
