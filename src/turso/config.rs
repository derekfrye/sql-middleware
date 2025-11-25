use crate::middleware::{ConfigAndPool, DatabaseType, MiddlewarePool, SqlMiddlewareDbError};

impl ConfigAndPool {
    /// Asynchronous initializer for `ConfigAndPool` with Turso (local/in-process).
    ///
    /// # Errors
    /// Returns `SqlMiddlewareDbError::ConnectionError` if database creation or connection test fails.
    pub async fn new_turso(db_path: String) -> Result<Self, SqlMiddlewareDbError> {
        Self::new_turso_with_translation(db_path, false).await
    }

    /// Asynchronous initializer for `ConfigAndPool` with Turso (local/in-process) and optional translation default.
    ///
    /// # Errors
    /// Returns `SqlMiddlewareDbError::ConnectionError` if database creation or connection test fails.
    ///
    /// Warning: translation skips placeholders inside quoted strings, comments, and dollar-quoted
    /// blocks via a lightweight state machine; it may miss edge cases in complex SQL. Prefer
    /// backend-specific SQL instead of relying on translation:
    /// ```rust
    /// # use sql_middleware::prelude::*;
    /// let query = match &conn {
    ///     MiddlewarePoolConnection::Postgres { .. } => r#"$function$
    /// BEGIN
    ///     RETURN ($1 ~ $q$[\t\r\n\v\\]$q$);
    /// END;
    /// $function$"#,
    ///     MiddlewarePoolConnection::Sqlite { .. } | MiddlewarePoolConnection::Turso { .. } => {
    ///         include_str!("../sql/functions/sqlite/03_sp_get_scores.sql")
    ///     }
    /// };
    /// ```
    pub async fn new_turso_with_translation(
        db_path: String,
        translate_placeholders: bool,
    ) -> Result<Self, SqlMiddlewareDbError> {
        let db = turso::Builder::new_local(&db_path)
            .build()
            .await
            .map_err(|e| {
                SqlMiddlewareDbError::ConnectionError(format!(
                    "Failed to create Turso database: {e}"
                ))
            })?;

        // Smoke-test a connection
        let conn = db.connect().map_err(|e| {
            SqlMiddlewareDbError::ConnectionError(format!("Failed to connect Turso database: {e}"))
        })?;

        // Best-effort pragmas for concurrency (ignore failure on in-memory/unsupported)
        let _ = conn.execute("PRAGMA journal_mode = WAL", ()).await;

        Ok(ConfigAndPool {
            pool: MiddlewarePool::Turso(db),
            db_type: DatabaseType::Turso,
            translate_placeholders,
        })
    }
}
