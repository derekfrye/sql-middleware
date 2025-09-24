#![cfg(feature = "turso")]

use sql_middleware::prelude::*;

#[test]
fn test5d_turso_custom_tx_minimal() -> Result<(), Box<dyn std::error::Error>> {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async move {
        // Turso isn't pooled in via deadpool yet; `get_connection` creates a fresh connection each time.
        let cap = ConfigAndPool::new_turso(":memory:".to_string()).await?;
        let pool = cap.pool.get().await?;
        let mut conn = MiddlewarePool::get_connection(&pool).await?;

        conn.execute_batch("CREATE TABLE IF NOT EXISTS t (id INTEGER, name TEXT);")
            .await?;

        // same pattern as LibSQL (test5b) for starting a transaction via helper
        let turso_conn = match &conn {
            MiddlewarePoolConnection::Turso(c) => c,
            _ => panic!("Expected Turso connection"),
        };
        // Begin a transaction via Turso helper (mirrors LibSQL's helper-based begin)
        let tx = sql_middleware::turso::begin_transaction(turso_conn).await?;

        // Turso vs 5b (LibSQL): prepared execution requires `&mut Prepared` since the
        // underlying turso::Statement has mutable query/execute methods.
        // Placeholders are SQLite-style (?1, ?2), same as LibSQL.
        let mut stmt = tx
            .prepare("INSERT INTO t (id, name) VALUES (?1, ?2)")
            .await?;
        let _ = tx
            .execute_prepared(
                &mut stmt,
                &[RowValues::Int(1), RowValues::Text("alice".into())],
            )
            .await?;
        tx.commit().await?;

        // Same SELECT placeholder style as LibSQL/SQLite
        let rs = conn
            .execute_select("SELECT name FROM t WHERE id = ?1", &[RowValues::Int(1)])
            .await?;
        assert_eq!(
            rs.results[0].get("name").unwrap().as_text().unwrap(),
            "alice"
        );
        Ok::<(), SqlMiddlewareDbError>(())
    })?;
    Ok(())
}
