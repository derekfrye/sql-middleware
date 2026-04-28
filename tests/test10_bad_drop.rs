#![cfg(all(feature = "postgres", feature = "turso", feature = "sqlite"))]
#[path = "bad_drop/postgres.rs"]
mod postgres;

use bb8::Pool;
use sql_middleware::SqlMiddlewareDbError;
use sql_middleware::translation::TranslationMode;
use sql_middleware::typed_api::TypedConnOps;
use sql_middleware::typed_sqlite::{
    Idle as SqIdle, SqliteTypedConnection, set_skip_drop_rollback_for_tests as sqlite_skip_drop,
};
use sql_middleware::typed_turso::{
    Idle as TuIdle, TursoConnection, TursoManager,
    set_skip_drop_rollback_for_tests as turso_skip_drop,
};
use tokio::task::yield_now;

async fn count_rows(conn: &mut impl TypedConnOps) -> Result<i64, SqlMiddlewareDbError> {
    let rs = conn
        .query("SELECT COUNT(*) AS cnt FROM bad_drop")
        .translation(TranslationMode::ForceOn)
        .select()
        .await?;
    let val = rs.results[0]
        .get("cnt")
        .and_then(|v| v.as_int())
        .ok_or_else(|| SqlMiddlewareDbError::ExecutionError("missing count".into()))?;
    Ok(*val)
}

async fn setup_table(conn: &mut impl TypedConnOps) -> Result<(), SqlMiddlewareDbError> {
    conn.execute_batch(
        "DROP TABLE IF EXISTS bad_drop; CREATE TABLE bad_drop (id INTEGER PRIMARY KEY);",
    )
    .await
}

async fn run_turso_bad_drop() -> Result<(), SqlMiddlewareDbError> {
    let db = turso::Builder::new_local(":memory:")
        .build()
        .await
        .map_err(|e| SqlMiddlewareDbError::ConnectionError(e.to_string()))?;
    let pool = Pool::builder()
        .max_size(1)
        .build(TursoManager::new(db))
        .await
        .map_err(|e| SqlMiddlewareDbError::ConnectionError(format!("turso pool error: {e}")))?;

    {
        let mut conn = TursoConnection::<TuIdle>::from_pool(&pool).await?;
        setup_table(&mut conn).await?;
    }

    turso_skip_drop(true);
    {
        let mut tx = TursoConnection::<TuIdle>::from_pool(&pool)
            .await?
            .begin()
            .await?;
        tx.execute_batch("INSERT INTO bad_drop (id) VALUES (1);")
            .await?;
        // drop without commit/rollback (simulate legacy leak)
    }
    turso_skip_drop(false);

    let mut conn = TursoConnection::<TuIdle>::from_pool(&pool).await?;
    assert_eq!(
        count_rows(&mut conn).await?,
        1,
        "turso: legacy drop should leave row present"
    );
    drop(conn);
    {
        let raw = pool.get_owned().await.map_err(|e| {
            SqlMiddlewareDbError::ConnectionError(format!("turso cleanup checkout error: {e}"))
        })?;
        raw.execute_batch("ROLLBACK").await.map_err(|e| {
            SqlMiddlewareDbError::ExecutionError(format!("turso cleanup rollback error: {e}"))
        })?;
    }

    {
        let mut tx = TursoConnection::<TuIdle>::from_pool(&pool)
            .await?
            .begin()
            .await?;
        tx.execute_batch("INSERT INTO bad_drop (id) VALUES (2);")
            .await?;
        // drop without commit/rollback; fixed path should rollback
    }
    yield_now().await;

    let mut conn = TursoConnection::<TuIdle>::from_pool(&pool).await?;
    assert_eq!(
        count_rows(&mut conn).await?,
        0,
        "turso: fixed drop should rollback and leave table empty"
    );
    Ok(())
}

async fn run_sqlite_bad_drop() -> Result<(), SqlMiddlewareDbError> {
    use sql_middleware::sqlite::config::SqliteManager;

    let pool = Pool::builder()
        .max_size(1)
        .build(SqliteManager::new("file::memory:?cache=shared".to_string()))
        .await
        .map_err(|e| SqlMiddlewareDbError::ConnectionError(format!("sqlite pool error: {e}")))?;

    {
        let mut conn = SqliteTypedConnection::<SqIdle>::from_pool(&pool).await?;
        setup_table(&mut conn).await?;
    }

    sqlite_skip_drop(true);
    {
        let mut tx = SqliteTypedConnection::<SqIdle>::from_pool(&pool)
            .await?
            .begin()
            .await?;
        tx.execute_batch("INSERT INTO bad_drop (id) VALUES (1);")
            .await?;
        // drop without commit/rollback (should rollback in Drop)
    }
    sqlite_skip_drop(false);

    let mut conn = SqliteTypedConnection::<SqIdle>::from_pool(&pool).await?;
    assert_eq!(
        count_rows(&mut conn).await?,
        1,
        "sqlite: legacy drop behavior (skip rollback) should leave row present"
    );
    drop(conn);
    sql_middleware::sqlite::config::rollback_for_tests(&pool).await?;

    // Now verify default (fixed) behavior rolls back.
    {
        let mut tx = SqliteTypedConnection::<SqIdle>::from_pool(&pool)
            .await?
            .begin()
            .await?;
        tx.execute_batch("INSERT INTO bad_drop (id) VALUES (2);")
            .await?;
        // drop without commit/rollback; should rollback by default
    }
    yield_now().await;
    let mut conn = SqliteTypedConnection::<SqIdle>::from_pool(&pool).await?;
    assert_eq!(
        count_rows(&mut conn).await?,
        0,
        "sqlite: fixed drop behavior should rollback and leave table empty"
    );
    Ok(())
}

#[test]
fn typed_bad_drop_is_rolled_back() -> Result<(), Box<dyn std::error::Error>> {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        postgres::run().await?;
        run_turso_bad_drop().await?;
        run_sqlite_bad_drop().await?;
        Ok(())
    })
}
