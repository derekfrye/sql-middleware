#![cfg(any(feature = "postgres", feature = "sqlite", feature = "turso"))]

use sql_middleware::middleware::{RowValues, SqlMiddlewareDbError};
use sql_middleware::translation::TranslationMode;
use sql_middleware::typed_api::{AnyIdle, BeginTx, Queryable, TxConn, TypedConnOps};
#[cfg(feature = "postgres")]
use sql_middleware::typed_postgres::{Idle as PgIdle, PgConnection, PgManager};
#[cfg(feature = "sqlite")]
use sql_middleware::typed_sqlite::{Idle as SqIdle, SqliteTypedConnection};
#[cfg(feature = "turso")]
use sql_middleware::typed_turso::{Idle as TuIdle, TursoConnection, TursoManager};

#[cfg(feature = "sqlite")]
use sql_middleware::sqlite::config::SqliteManager;

#[derive(Clone, Copy, Debug)]
enum Backend {
    #[cfg(feature = "postgres")]
    Postgres,
    #[cfg(feature = "turso")]
    Turso,
    #[cfg(feature = "sqlite")]
    Sqlite,
}

/// Minimal end-to-end smoke test exercising AnyIdle/AnyTx + typed connections for issue #2 API shape.
#[tokio::test]
async fn test_issue_2_anyidle_flow() -> Result<(), SqlMiddlewareDbError> {
    for backend in enabled_backends() {
        run_backend_flow(backend).await?;
    }
    Ok(())
}

fn enabled_backends() -> Vec<Backend> {
    vec![
        #[cfg(feature = "postgres")]
        Backend::Postgres,
        #[cfg(feature = "turso")]
        Backend::Turso,
        #[cfg(feature = "sqlite")]
        Backend::Sqlite,
    ]
}

async fn run_backend_flow(backend: Backend) -> Result<(), SqlMiddlewareDbError> {
    let mut conn = build_connection(backend).await?;

    // Reset the table so each backend run is independent.
    conn.execute_batch(
        "DROP TABLE IF EXISTS test11_users; \
         CREATE TABLE test11_users (id INT PRIMARY KEY, name TEXT);",
    )
    .await?;

    // Auto-commit insert on the idle connection.
    conn.query("INSERT INTO test11_users (id, name) VALUES ($2, $1)")
        .translation(TranslationMode::ForceOn)
        .params(&[RowValues::Text("Alice".into()), RowValues::Int(42)])
        .dml()
        .await?;

    // Verify initial value.
    assert_eq!(fetch_name(&mut conn).await?, "Alice");

    // Run shared auto-commit + transaction flow.
    let mut restored = more_work(conn).await?;

    // Verify final value after updates.
    assert_eq!(fetch_name(&mut restored).await?, "Bob");
    Ok(())
}

async fn build_connection(backend: Backend) -> Result<AnyIdle, SqlMiddlewareDbError> {
    match backend {
        #[cfg(feature = "postgres")]
        Backend::Postgres => {
            let mut cfg = tokio_postgres::Config::new();
            cfg.host("10.3.0.201")
                .port(5432)
                .dbname("testing")
                .user("testuser");
            if let Ok(pw) = std::env::var("TESTING_PG_PASSWORD") {
                cfg.password(pw);
            }
            let pool = PgManager::new(cfg).build_pool().await?;
            Ok(AnyIdle::Postgres(
                PgConnection::<PgIdle>::from_pool(&pool).await?,
            ))
        }
        #[cfg(feature = "turso")]
        Backend::Turso => {
            let db = turso::Builder::new_local(":memory:")
                .build()
                .await
                .map_err(|e| SqlMiddlewareDbError::ConnectionError(e.to_string()))?;
            let pool = TursoManager::new(db).build_pool().await?;
            Ok(AnyIdle::Turso(
                TursoConnection::<TuIdle>::from_pool(&pool).await?,
            ))
        }
        #[cfg(feature = "sqlite")]
        Backend::Sqlite => {
            let pool = SqliteManager::new("file::memory:?cache=shared".to_string())
                .build_pool()
                .await?;
            Ok(AnyIdle::Sqlite(
                SqliteTypedConnection::<SqIdle>::from_pool(&pool).await?,
            ))
        }
    }
}

async fn more_work(mut conn: AnyIdle) -> Result<AnyIdle, SqlMiddlewareDbError> {
    // Auto-commit path
    update_user(&mut conn).await?;

    // Explicit transaction path
    let mut tx_conn = conn.begin().await?;
    update_user(&mut tx_conn).await?;
    conn = tx_conn.commit().await?;
    Ok(conn)
}

// Works for typed backends via AnyIdle/AnyTx.
async fn update_user(conn: &mut impl TypedConnOps) -> Result<(), SqlMiddlewareDbError> {
    conn.query("UPDATE test11_users SET name = $1 WHERE id = $2")
        .translation(TranslationMode::ForceOn)
        .params(&[RowValues::Text("Bob".into()), RowValues::Int(42)])
        .dml()
        .await?;
    Ok(())
}

async fn fetch_name(conn: &mut impl TypedConnOps) -> Result<String, SqlMiddlewareDbError> {
    let rs = conn
        .query("SELECT name FROM test11_users WHERE id = $1")
        .translation(TranslationMode::ForceOn)
        .params(&[RowValues::Int(42)])
        .select()
        .await?;
    let val = rs
        .results
        .first()
        .and_then(|row| row.get("name"))
        .and_then(|v| v.as_text())
        .ok_or_else(|| SqlMiddlewareDbError::ExecutionError("missing name".into()))?;
    Ok(val.to_string())
}
