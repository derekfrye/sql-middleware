#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bb8::Pool;

    use crate::middleware::SqlMiddlewareDbError;
    use crate::sqlite::connection::run_blocking;

    use super::super::config::SqliteManager;

    #[tokio::test]
    async fn worker_panic_marks_connection_broken() -> Result<(), Box<dyn std::error::Error>> {
        let pool = Pool::builder()
            .max_size(1)
            .build(SqliteManager::new("file::memory:?cache=shared".to_string()))
            .await?;

        let conn = pool.get_owned().await?;
        let handle = Arc::clone(&*conn);
        let err = run_blocking(handle, |_conn| -> Result<(), SqlMiddlewareDbError> {
            panic!("boom");
        })
        .await
        .expect_err("worker panic should surface as an error");
        assert!(
            err.to_string().contains("worker receive error"),
            "unexpected error for worker panic: {err}"
        );
        assert!(conn.is_broken(), "connection should be marked broken");

        drop(conn);

        let conn = pool.get_owned().await?;
        let handle = Arc::clone(&*conn);
        run_blocking(handle, |c| {
            c.query_row("SELECT 1", rusqlite::params![], |_row| Ok(()))
                .map_err(SqlMiddlewareDbError::SqliteError)
        })
        .await?;
        assert!(
            !conn.is_broken(),
            "replacement connection should be healthy"
        );
        Ok(())
    }
}
