#![cfg(feature = "sqlite")]

use sql_middleware::prelude::*;
use sql_middleware::{SqliteParamsExecute, convert_sql_params};

#[test]
fn test5c_sqlite_custom_tx_minimal() -> Result<(), Box<dyn std::error::Error>> {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async move {
        let cap = ConfigAndPool::new_sqlite("file::memory:?cache=shared".to_string()).await?;
        let mut conn = cap.get_connection().await?;

        conn.execute_batch("CREATE TABLE IF NOT EXISTS t (id INTEGER, name TEXT);")
            .await?;

        match &mut conn {
            MiddlewarePoolConnection::Sqlite { .. } => {
                let params = vec![RowValues::Int(1), RowValues::Text("alice".into())];
                let converted =
                    convert_sql_params::<SqliteParamsExecute>(&params, ConversionMode::Execute)?;

                conn.with_sqlite_connection(move |raw| {
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
            }
            _ => panic!("Expected SQLite connection"),
        }

        let rs = conn
            .execute_select("SELECT name FROM t WHERE id = ?1", &[RowValues::Int(1)])
            .await?;
        assert_eq!(
            rs.results[0].get("name").unwrap().as_text().unwrap(),
            "alice"
        );

        // Exercise the prepared statement API to reuse the compiled select.
        let prepared = conn
            .prepare_sqlite_statement("SELECT name FROM t WHERE id = ?1")
            .await?;
        let result_set = prepared.query(&[RowValues::Int(1)]).await?;
        assert_eq!(
            result_set.results[0]
                .get("name")
                .unwrap()
                .as_text()
                .unwrap(),
            "alice"
        );
        Ok::<(), SqlMiddlewareDbError>(())
    })?;
    Ok(())
}
