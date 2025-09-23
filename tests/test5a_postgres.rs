#![cfg(feature = "test-utils")]

use sql_middleware::prelude::*;
use sql_middleware::{convert_sql_params, PostgresParams};
use sql_middleware::test_utils::testing_postgres::{setup_postgres_container, stop_postgres_container};

#[test]
fn test5a_postgres_custom_tx_minimal() -> Result<(), Box<dyn std::error::Error>> {
    let mut cfg = deadpool_postgres::Config::new();
    cfg.dbname = Some("test_db".to_string());
    cfg.host = Some("localhost".to_string());

    let pg = setup_postgres_container(&cfg)?;
    // Use the fully populated config returned by the embedded helper (has correct user/pass/port)
    let real_cfg = pg.config.clone();

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async move {
        let cap = ConfigAndPool::new_postgres(real_cfg).await?;
        let pool = cap.pool.get().await?;
        let mut conn = MiddlewarePool::get_connection(pool).await?;

        conn.execute_batch("CREATE TABLE IF NOT EXISTS t (id BIGINT, name TEXT);").await?;

        // Get Postgres-specific client and start a transaction
        let pg_obj = match &mut conn {
            MiddlewarePoolConnection::Postgres(pg) => pg,
            _ => panic!("Expected Postgres connection"),
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
        let rs = conn.execute_select("SELECT name FROM t WHERE id = $1", &[RowValues::Int(1)]).await?;
        assert_eq!(rs.results[0].get("name").unwrap().as_text().unwrap(), "alice");
        Ok::<(), SqlMiddlewareDbError>(())
    })?;

    stop_postgres_container(pg);
    Ok(())
}
