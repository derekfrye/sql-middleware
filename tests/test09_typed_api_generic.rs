#![cfg(all(feature = "typed-postgres", feature = "postgres"))]

use sql_middleware::middleware::RowValues;
use sql_middleware::typed_api::{AnyIdle, AnyTx, BeginTx, Queryable, TxConn};
use sql_middleware::typed_postgres::{Idle as PgIdle, PgConnection, PgManager};
use sql_middleware::SqlMiddlewareDbError;

fn build_pg_config() -> tokio_postgres::Config {
    let mut pg_cfg = tokio_postgres::Config::new();
    pg_cfg.user("testuser");
    pg_cfg.host("localhost");
    pg_cfg.dbname("test_db");
    // tests rely on embedded postgres from test-utils; password/port will be set there.
    pg_cfg
}

// Backend-agnostic helper: works with AnyIdle or AnyTx via Queryable.
async fn update_user(conn: &mut impl Queryable) -> Result<(), SqlMiddlewareDbError> {
    conn.query("UPDATE typed_api_users SET name = $1 WHERE id = $2")
        .params(&[RowValues::Text("New Name".into()), RowValues::Int(42)])
        .dml()
        .await?;
    Ok(())
}

#[tokio::test]
async fn typed_api_generic_helper_postgres() -> Result<(), Box<dyn std::error::Error>> {
    // Stand up embedded Postgres via test-utils and translate into tokio_postgres config.
    let mut cfg = deadpool_postgres::Config::new();
    cfg.dbname = Some("test_db".to_string());
    cfg.host = Some("localhost".to_string());
    let pg = sql_middleware::test_utils::testing_postgres::setup_postgres_container(&cfg)?;
    let mut real_cfg = build_pg_config();
    if let Some(port) = pg.config.port {
        real_cfg.port(port);
    }
    if let Some(user) = pg.config.user.as_deref() {
        real_cfg.user(user);
    }
    if let Some(password) = pg.config.password.as_deref() {
        real_cfg.password(password);
    }

    let pool = PgManager::new(real_cfg).build_pool().await?;
    let mut conn: AnyIdle = AnyIdle::Postgres(PgConnection::<PgIdle>::from_pool(&pool).await?);

    // Setup table once.
    conn.query(
        "DROP TABLE IF EXISTS typed_api_users; CREATE TABLE typed_api_users (id BIGINT PRIMARY KEY, name TEXT);",
    )
    .dml()
    .await?;
    conn.query("INSERT INTO typed_api_users (id, name) VALUES ($1, $2)")
        .params(&[RowValues::Int(42), RowValues::Text("Old Name".into())])
        .dml()
        .await?;

    // Case 1: auto-commit
    update_user(&mut conn).await?;

    // Case 2: explicit transaction
    let mut tx: AnyTx = conn.begin().await?;
    update_user(&mut tx).await?;
    conn = tx.commit().await?;

    // Verify
    let rs = conn
        .query("SELECT name FROM typed_api_users WHERE id = $1")
        .params(&[RowValues::Int(42)])
        .select()
        .await?;
    assert_eq!(rs.results.len(), 1);
    assert_eq!(rs.results[0].get("name").unwrap().as_text().unwrap(), "New Name");

    sql_middleware::test_utils::testing_postgres::stop_postgres_container(pg);
    Ok(())
}
