#![cfg(feature = "postgres")]

use sql_middleware::convert_sql_params;
use sql_middleware::postgres::Params as PostgresParams;
use sql_middleware::prelude::*;
#[cfg(feature = "postgres")]
use sql_middleware::typed_postgres::{Idle as PgIdle, PgConnection, PgManager};
use std::env;

#[test]
fn test5a_postgres_custom_tx_minimal() -> Result<(), Box<dyn std::error::Error>> {
    let mut cfg = PgConfig::new();
    cfg.dbname = Some("testing".to_string());
    cfg.host = Some("10.3.0.201".to_string());
    cfg.port = Some(5432);
    cfg.user = Some("testuser".to_string());
    cfg.password = Some(env::var("TESTING_PG_PASSWORD").unwrap_or_default());

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async move {
        let cap = ConfigAndPool::new_postgres(PostgresOptions::new(cfg.clone())).await?;
        let mut conn = cap.get_connection().await?;

        conn.execute_batch("CREATE TABLE IF NOT EXISTS t (id BIGINT, name TEXT);")
            .await?;

        // Get Postgres-specific client and start a transaction
        let MiddlewarePoolConnection::Postgres { client: pg_obj, .. } = &mut conn else {
            panic!("Expected Postgres connection");
        };
        let tx = pg_obj.transaction().await?;

        // Prepare + convert + execute
        let insert = "INSERT INTO t (id, name) VALUES ($1, $2)";
        let stmt = tx.prepare(insert).await?;
        let params = vec![RowValues::Int(1), RowValues::Text("alice".into())];
        // - Explicit convert_sql_params call req'd; underlying tokio_postgres needs our RowValues converted
        let converted = convert_sql_params::<PostgresParams>(&params, ConversionMode::Execute)?;
        let _ = tx.execute(&stmt, converted.as_refs()).await?;
        tx.commit().await?;

        // Verify
        let rs = conn
            .query("SELECT name FROM t WHERE id = $1")
            .params(&[RowValues::Int(1)])
            .select()
            .await?;
        assert_eq!(
            rs.results[0].get("name").unwrap().as_text().unwrap(),
            "alice"
        );

        #[cfg(feature = "postgres")]
        {
            // Exercise the typestate API against the same backend using a separate table.
            let pg_cfg = cfg.to_tokio_config();

            let pool = PgManager::new(pg_cfg).build_pool().await?;
            let mut typed_conn: PgConnection<PgIdle> = PgConnection::from_pool(&pool).await?;

            typed_conn
                .execute_batch("CREATE TABLE IF NOT EXISTS t_typed (id BIGINT, name TEXT);")
                .await?;
            let _ = typed_conn
                .dml(
                    "INSERT INTO t_typed (id, name) VALUES ($1, $2)",
                    &[RowValues::Int(2), RowValues::Text("bob".into())],
                )
                .await?;
            let rs = typed_conn
                .select(
                    "SELECT name FROM t_typed WHERE id = $1",
                    &[RowValues::Int(2)],
                )
                .await?;
            assert_eq!(rs.results[0].get("name").unwrap().as_text().unwrap(), "bob");
        }
        Ok::<(), SqlMiddlewareDbError>(())
    })?;

    Ok(())
}
