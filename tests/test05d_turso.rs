#![cfg(feature = "turso")]

use sql_middleware::prelude::*;

#[test]
fn test5d_turso_custom_tx_minimal() -> Result<(), Box<dyn std::error::Error>> {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async move {
        let cap = ConfigAndPool::turso_builder(":memory:".to_string())
            .build()
            .await?;
        let mut conn = cap.get_connection().await?;

        conn.execute_batch("CREATE TABLE IF NOT EXISTS t (id INTEGER, name TEXT);")
            .await?;

        // Same pattern as SQLite for starting a transaction via helper
        let MiddlewarePoolConnection::Turso {
            conn: turso_conn, ..
        } = &mut conn
        else {
            panic!("Expected Turso connection");
        };
        // Begin a transaction via Turso helper
        let tx = sql_middleware::turso::begin_transaction(turso_conn).await?;

        // Prepared execution requires `&mut Prepared` since the
        // underlying turso::Statement has mutable query/execute methods.
        // Placeholders are SQLite-style (?1, ?2), same as SQLite.
        let mut stmt = tx
            .prepare("INSERT INTO t (id, name) VALUES (?1, ?2)")
            .await?;
        let _ = tx
            .execute(&mut stmt)
            .params(&[RowValues::Int(1), RowValues::Text("alice".into())])
            .run()
            .await?;
        let mut stmt = tx.prepare("SELECT name FROM t WHERE id = ?1").await?;
        let mapped_name = tx
            .select(&mut stmt)
            .params(&[RowValues::Int(1)])
            .map_one(|row| row.get::<String>(0).map_err(Into::into))
            .await?;
        assert_eq!(mapped_name, "alice");

        let mapped_missing = tx
            .select(&mut stmt)
            .params(&[RowValues::Int(2)])
            .map_optional(|row| row.get::<String>(0).map_err(Into::into))
            .await?;
        assert!(mapped_missing.is_none());
        tx.commit().await?;

        // Same SELECT placeholder style as SQLite
        let rs = conn
            .query("SELECT name FROM t WHERE id = ?1")
            .params(&[RowValues::Int(1)])
            .select()
            .await?;
        assert_eq!(
            rs.results[0].get("name").unwrap().as_text().unwrap(),
            "alice"
        );

        let prepared = conn
            .prepare_turso_statement("SELECT name FROM t WHERE id = ?1")
            .await?;
        let mut params_buf = TursoParamsBuf::with_capacity(1);
        params_buf.set_int(0, 1);
        let mapped_name = prepared
            .select()
            .params_buf(&params_buf)
            .map_one(|row| row.get::<String>(0).map_err(Into::into))
            .await?;
        assert_eq!(mapped_name, "alice");

        params_buf.set_int(0, 2);
        let mapped_missing = prepared
            .select()
            .params_buf(&params_buf)
            .map_optional(|row| row.get::<String>(0).map_err(Into::into))
            .await?;
        assert!(mapped_missing.is_none());

        Ok::<(), SqlMiddlewareDbError>(())
    })?;
    Ok(())
}
