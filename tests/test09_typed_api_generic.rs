#![cfg(all(feature = "typed-postgres", feature = "postgres", feature = "typed-turso", feature = "turso"))]

use sql_middleware::middleware::RowValues;
use sql_middleware::typed_api::{AnyIdle, AnyTx, BeginTx, Queryable, TxConn};
use sql_middleware::typed_postgres::{Idle as PgIdle, PgConnection, PgManager};
use sql_middleware::typed_turso::{Idle as TuIdle, TursoConnection, TursoManager};
use sql_middleware::translation::TranslationMode;
use sql_middleware::SqlMiddlewareDbError;

// Backend-agnostic helper: works with AnyIdle or AnyTx via Queryable.
async fn update_user(conn: &mut impl Queryable) -> Result<(), SqlMiddlewareDbError> {
    conn.query("UPDATE typed_api_users SET name = $1 WHERE id = $2")
        .params(&[RowValues::Text("New Name".into()), RowValues::Int(42)])
        .dml()
        .await?;
    Ok(())
}

async fn run_backend(mut conn: AnyIdle) -> Result<(), SqlMiddlewareDbError> {
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
        .translation(TranslationMode::ForceOn)
        .select()
        .await?;
    assert_eq!(rs.results.len(), 1);
    assert_eq!(rs.results[0].get("name").unwrap().as_text().unwrap(), "New Name");
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

        for mut backend in backends {
            // Create fresh table and seed a row for this backend.
            backend
                .query("DROP TABLE IF EXISTS typed_api_users")
                .translation(TranslationMode::ForceOn)
                .dml()
                .await?;
            backend
                .query("CREATE TABLE typed_api_users (id BIGINT PRIMARY KEY, name TEXT)")
                .translation(TranslationMode::ForceOn)
                .dml()
                .await?;
            backend
                .query("INSERT INTO typed_api_users (id, name) VALUES ($1, $2)")
                .translation(TranslationMode::ForceOn)
                .params(&[RowValues::Int(42), RowValues::Text("Old Name".into())])
                .dml()
                .await?;

            run_backend(backend).await?;
        }

        Ok(())
    })
}
