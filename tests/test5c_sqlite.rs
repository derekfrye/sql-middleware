#![cfg(feature = "sqlite")]

use sql_middleware::prelude::*;
use sql_middleware::{convert_sql_params, SqliteParamsExecute};

#[test]
fn test5c_sqlite_custom_tx_minimal() -> Result<(), Box<dyn std::error::Error>> {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async move {
        let cap = ConfigAndPool::new_sqlite("file::memory:?cache=shared".to_string()).await?;
        let pool = cap.pool.get().await?;
        let mut conn = MiddlewarePool::get_connection(&pool).await?;

        conn.execute_batch("CREATE TABLE IF NOT EXISTS t (id INTEGER, name TEXT);").await?;

        if let MiddlewarePoolConnection::Sqlite(sqlite_obj) = &mut conn {
            let params = vec![RowValues::Int(1), RowValues::Text("alice".into())];
            let converted = convert_sql_params::<SqliteParamsExecute>(&params, ConversionMode::Execute)?;

            let _ = sqlite_obj
                .interact(move |raw| {
                    let tx = raw.transaction()?;
                    {
                        let mut stmt = tx.prepare("INSERT INTO t (id, name) VALUES (?1, ?2)")?;
                        // SqliteParamsExecute wraps a ParamsFromIter which can be passed directly
                        let _ = stmt.execute(converted.0)?;
                    }
                    tx.commit()?; // commit after stmt is dropped
                    Ok::<(), SqlMiddlewareDbError>(())
                })
                .await?;
        } else {
            panic!("Expected SQLite connection");
        }

        let rs = conn.execute_select("SELECT name FROM t WHERE id = ?1", &[RowValues::Int(1)]).await?;
        assert_eq!(rs.results[0].get("name").unwrap().as_text().unwrap(), "alice");
        Ok::<(), SqlMiddlewareDbError>(())
    })?;
    Ok(())
}
