#![cfg(feature = "libsql")]

use sql_middleware::prelude::*;

#[test]
fn test5b_libsql_custom_tx_minimal() -> Result<(), Box<dyn std::error::Error>> {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async move {
        let cap = ConfigAndPool::new_libsql(":memory:".to_string()).await?;
        let pool = cap.pool.get().await?;
        let mut conn = MiddlewarePool::get_connection(&pool).await?;

        conn.execute_batch("CREATE TABLE IF NOT EXISTS t (id INTEGER, name TEXT);").await?;

        // Begin a transaction via libsql helper
        let lib = match &conn {
            MiddlewarePoolConnection::Libsql(obj) => obj,
            _ => panic!("Expected LibSQL connection"),
        };
        let tx = sql_middleware::libsql::begin_transaction(lib).await?;

        // Prepare + execute with RowValues directly
        let stmt = tx.prepare("INSERT INTO t (id, name) VALUES (?1, ?2)").await?;
        let _ = tx.execute_prepared(&stmt, &[RowValues::Int(1), RowValues::Text("alice".into())]).await?;
        tx.commit().await?;

        let rs = conn.execute_select("SELECT name FROM t WHERE id = ?1", &[RowValues::Int(1)]).await?;
        assert_eq!(rs.results[0].get("name").unwrap().as_text().unwrap(), "alice");
        Ok::<(), SqlMiddlewareDbError>(())
    })?;
    Ok(())
}

