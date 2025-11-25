#![cfg(feature = "turso")]

use sql_middleware::prelude::*;

#[test]
fn turso_url_literal_vs_placeholder() -> Result<(), Box<dyn std::error::Error>> {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async move {
        let cap = ConfigAndPool::new_turso(":memory:".to_string()).await?;
        let mut conn = cap.get_connection().await?;

        conn.execute_batch("CREATE TABLE tbl (val TEXT);").await?;
        conn.query("INSERT INTO tbl (val) VALUES (?1);")
            .params(&[RowValues::Text(
                "https://example.com/?1=param1Value&2=param2&token=$123abc".into(),
            )])
            .dml()
            .await?;

        let rs = conn
            .query("SELECT val FROM tbl WHERE val LIKE 'https://example.com/?1=' || ?1 || '%';")
            .params(&[RowValues::Text("param1Value".into())])
            .select()
            .await?;

        assert_eq!(rs.results.len(), 1);
        assert_eq!(
            rs.results[0].get("val").unwrap().as_text().unwrap(),
            "https://example.com/?1=param1Value&2=param2&token=$123abc"
        );
        Ok::<(), SqlMiddlewareDbError>(())
    })?;
    Ok(())
}
