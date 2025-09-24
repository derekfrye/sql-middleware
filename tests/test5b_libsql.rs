#![cfg(feature = "libsql")]

use sql_middleware::prelude::*;

#[test]
fn test5b_libsql_custom_tx_minimal() -> Result<(), Box<dyn std::error::Error>> {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async move {
        let cap = ConfigAndPool::new_libsql(":memory:".to_string()).await?;
        let pool = cap.pool.get().await?;
        let mut conn = MiddlewarePool::get_connection(pool).await?;

        conn.execute_batch("CREATE TABLE IF NOT EXISTS t (id INTEGER, name TEXT);")
            .await?;

        // LibSQL vs 5a (Postgres): we don't want a "client" from a deadpool object here.
        // Instead, begin a transaction via the middleware helper so we can use our custom `prepare` API.
        // If we'd instead exposed a libsql-specific Transaction type like in test5a, there's no further
        // `prepare` API exposed by deadpool-libsql that we could use.
        let MiddlewarePoolConnection::Libsql(lib) = &conn else {
            panic!("Expected LibSQL connection");
        };
        let tx = sql_middleware::libsql::begin_transaction(lib).await?;

        // LibSQL differences vs 5a (postgres) example:
        // - SQLite-style placeholders (?1, ?2) instead of Postgres $1, $2
        // - No explicit convert_sql_params call req'd; libsql prepared helpers accept &[RowValues] directly
        let stmt = tx.prepare("INSERT INTO t (id, name) VALUES (?1, ?2)")?;
        let _ = tx
            .execute_prepared(&stmt, &[RowValues::Int(1), RowValues::Text("alice".into())])
            .await?;
        tx.commit().await?;

        // Verify
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
