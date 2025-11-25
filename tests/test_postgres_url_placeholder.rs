#![cfg(feature = "postgres")]

use sql_middleware::prelude::*;
use std::env;

#[test]
fn postgres_url_literal_vs_placeholder() -> Result<(), Box<dyn std::error::Error>> {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async move {
        let mut cfg = deadpool_postgres::Config::new();
        cfg.dbname = Some("testing".to_string());
        cfg.host = Some("10.3.0.201".to_string());
        cfg.port = Some(5432);
        cfg.user = Some("testuser".to_string()); // default user
        // Deadpool requires a password field; default to empty for trust auth, allow env override.
        cfg.password = Some(env::var("TESTING_PG_PASSWORD").unwrap_or_default());

        let cap = ConfigAndPool::new_postgres(cfg).await?;
        let mut conn = cap.get_connection().await?;

        conn.execute_batch("CREATE TABLE IF NOT EXISTS tbl (val TEXT);")
            .await?;
        conn.execute_dml(
            "INSERT INTO tbl (val) VALUES ($1);",
            &[RowValues::Text(
                "https://example.com/?1=param1Value&2=param2&token=$123abc".into(),
            )],
        )
        .await?;

        let rs = conn
            .execute_select(
                "SELECT val FROM tbl WHERE val LIKE $tag$https://example.com/?1=$tag$ || $1 || '%';",
                &[RowValues::Text("param1Value".into())],
            )
            .await?;

        assert_eq!(rs.results.len(), 1);
        assert_eq!(
            rs.results[0].get("val").unwrap().as_text().unwrap(),
            "https://example.com/?1=param1Value&2=param2&token=$123abc"
        );

        conn.execute_batch("DROP TABLE IF EXISTS tbl;").await?;
        Ok::<(), SqlMiddlewareDbError>(())
    })?;
    Ok(())
}
