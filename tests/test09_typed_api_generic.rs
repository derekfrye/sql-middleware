#![cfg(all(feature = "postgres", feature = "turso"))]

use sql_middleware::SqlMiddlewareDbError;
use sql_middleware::middleware::RowValues;
use sql_middleware::translation::TranslationMode;
use sql_middleware::typed_api::{AnyIdle, AnyTx, BeginTx, TxConn, TypedConnOps};
use sql_middleware::typed_postgres::{Idle as PgIdle, PgConnection, PgManager};
use sql_middleware::typed_turso::{Idle as TuIdle, TursoConnection, TursoManager};

// Backend-agnostic helpers.
async fn insert_rows(
    conn: &mut impl TypedConnOps,
    start_id: i64,
    count: i64,
    label: &str,
) -> Result<(), SqlMiddlewareDbError> {
    for idx in 0..count {
        let id = start_id + idx;
        conn.query("INSERT INTO typed_api_users (id, name) VALUES ($1, $2)")
            .translation(TranslationMode::ForceOn)
            .params(&[RowValues::Int(id), RowValues::Text(format!("{label}_{id}"))])
            .dml()
            .await?;
    }
    Ok(())
}

async fn count_rows(conn: &mut impl TypedConnOps) -> Result<i64, SqlMiddlewareDbError> {
    let rs = conn
        .query("SELECT COUNT(*) AS cnt FROM typed_api_users")
        .translation(TranslationMode::ForceOn)
        .select()
        .await?;
    let val = rs.results[0]
        .get("cnt")
        .and_then(|v| v.as_int())
        .ok_or_else(|| SqlMiddlewareDbError::ExecutionError("missing count".into()))?;
    Ok(*val)
}

async fn run_backend(mut conn: AnyIdle) -> Result<(), SqlMiddlewareDbError> {
    // Create table with a batch (autocommit).
    conn.execute_batch(
        "DROP TABLE IF EXISTS typed_api_users; CREATE TABLE typed_api_users (id BIGINT PRIMARY KEY, name TEXT);",
    )
    .await?;

    // Insert 10 rows via auto-commit DML.
    insert_rows(&mut conn, 0, 10, "auto").await?;

    // Tx 1: insert 10 rows via DML then rollback.
    let mut tx = conn.begin().await?;
    insert_rows(&mut tx, 100, 10, "tx1").await?;
    conn = tx.rollback().await?;
    assert_eq!(count_rows(&mut conn).await?, 10);

    // Tx 2: insert 10 rows via batch then rollback.
    let mut tx = conn.begin().await?;
    let mut batch_sql = String::new();
    for idx in 0..10 {
        let id = 200 + idx;
        batch_sql.push_str(&format!(
            "INSERT INTO typed_api_users (id, name) VALUES ({id}, 'tx2_{id}');"
        ));
    }
    tx.execute_batch(&batch_sql).await?;
    conn = tx.rollback().await?;

    // Verify still 10 rows after rollbacks.
    assert_eq!(count_rows(&mut conn).await?, 10);

    // Tx 3: insert 10 rows via DML without commit; verify visibility inside tx.
    let mut tx = conn.begin().await?;
    insert_rows(&mut tx, 300, 10, "tx3").await?;
    assert_eq!(count_rows(&mut tx).await?, 20);
    conn = tx.rollback().await?;

    // Final check: back to 10 rows.
    assert_eq!(count_rows(&mut conn).await?, 10);
    Ok(())
}

fn postgres_cfg_for_test06() -> tokio_postgres::Config {
    let mut pg_cfg = tokio_postgres::Config::new();
    pg_cfg.dbname("testing");
    pg_cfg.host("10.3.0.201");
    pg_cfg.port(5432);
    pg_cfg.user("testuser");
    if let Ok(pw) = std::env::var("TESTING_PG_PASSWORD") {
        pg_cfg.password(pw);
    }
    pg_cfg
}

#[test]
fn typed_api_generic_helper_multiple_backends() -> Result<(), Box<dyn std::error::Error>> {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let mut backends: Vec<AnyIdle> = Vec::new();

        // Postgres branch.
        let pg_cfg = postgres_cfg_for_test06();
        let pg_pool = PgManager::new(pg_cfg).build_pool().await?;
        backends.push(AnyIdle::Postgres(
            PgConnection::<PgIdle>::from_pool(&pg_pool).await?,
        ));

        // Turso branch (in-memory).
        let db = turso::Builder::new_local(":memory:")
            .build()
            .await
            .map_err(|e| SqlMiddlewareDbError::ConnectionError(e.to_string()))?;
        let tu_pool = TursoManager::new(db).build_pool().await?;
        backends.push(AnyIdle::Turso(
            TursoConnection::<TuIdle>::from_pool(&tu_pool).await?,
        ));

        for backend in backends {
            run_backend(backend).await?;
        }

        Ok(())
    })
}
