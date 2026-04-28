use bb8::Pool;
use sql_middleware::SqlMiddlewareDbError;
use sql_middleware::typed_postgres::{
    Idle as PgIdle, PgConnection, PgManager, set_skip_drop_rollback_for_tests as pg_skip_drop,
};
use tokio::task::yield_now;

use super::{count_rows, setup_table};

pub(super) async fn run() -> Result<(), SqlMiddlewareDbError> {
    let debug = std::env::var_os("SQL_MIDDLEWARE_PG_DEBUG").is_some();
    let pool = build_pool(debug).await?;
    run_legacy_drop(&pool, debug).await?;
    run_fixed_drop(&pool, debug).await?;
    {
        let mut conn = PgConnection::<PgIdle>::from_pool(&pool).await?;
        conn.execute_batch("DROP TABLE IF EXISTS bad_drop;").await?;
    }
    Ok(())
}

async fn build_pool(debug: bool) -> Result<Pool<PgManager>, SqlMiddlewareDbError> {
    let mut pg_cfg = tokio_postgres::Config::new();
    pg_cfg.dbname("testing");
    pg_cfg.host("10.3.0.201");
    pg_cfg.port(5432);
    pg_cfg.user("testuser");
    if let Ok(pw) = std::env::var("TESTING_PG_PASSWORD") {
        pg_cfg.password(pw);
    }
    if debug {
        eprintln!("[pg-bad-drop] building pool");
    }
    let pool = Pool::builder()
        .max_size(1)
        .build(PgManager::new(pg_cfg))
        .await
        .map_err(|e| SqlMiddlewareDbError::ConnectionError(format!("postgres pool error: {e}")))?;
    if debug {
        eprintln!("[pg-bad-drop] pool built");
    }
    Ok(pool)
}

async fn run_legacy_drop(pool: &Pool<PgManager>, debug: bool) -> Result<(), SqlMiddlewareDbError> {
    if debug {
        eprintln!("[pg-bad-drop] checking out conn for setup");
    }
    {
        let mut conn = PgConnection::<PgIdle>::from_pool(pool).await?;
        setup_table(&mut conn).await?;
    }
    pg_skip_drop(true);
    {
        if debug {
            eprintln!("[pg-bad-drop] checking out tx1");
        }
        let mut tx = PgConnection::<PgIdle>::from_pool(pool)
            .await?
            .begin()
            .await?;
        tx.execute_batch("INSERT INTO bad_drop (id) VALUES (1);")
            .await?;
    }
    pg_skip_drop(false);
    let mut conn = PgConnection::<PgIdle>::from_pool(pool).await?;
    assert_eq!(
        count_rows(&mut conn).await?,
        1,
        "postgres: legacy drop should leave row present"
    );
    conn.execute_batch("ROLLBACK").await?;
    Ok(())
}

async fn run_fixed_drop(pool: &Pool<PgManager>, debug: bool) -> Result<(), SqlMiddlewareDbError> {
    {
        if debug {
            eprintln!("[pg-bad-drop] checking out tx2");
        }
        let mut tx = PgConnection::<PgIdle>::from_pool(pool)
            .await?
            .begin()
            .await?;
        tx.execute_batch("INSERT INTO bad_drop (id) VALUES (2);")
            .await?;
    }
    yield_now().await;
    let mut conn = PgConnection::<PgIdle>::from_pool(pool).await?;
    assert_eq!(
        count_rows(&mut conn).await?,
        0,
        "postgres: fixed drop should rollback and leave table empty"
    );
    Ok(())
}
