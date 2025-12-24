use crate::error::SqlMiddlewareDbError;
use crate::sqlite::config::SqliteManager;
use crate::sqlite::{SqliteConnection, SqlitePreparedStatement};

use super::MiddlewarePoolConnection;

#[cfg(feature = "sqlite")]
pub(super) async fn get_connection(
    pool: &bb8::Pool<SqliteManager>,
    translate_placeholders: bool,
) -> Result<MiddlewarePoolConnection, SqlMiddlewareDbError> {
    let conn = pool.get_owned().await.map_err(|e| {
        SqlMiddlewareDbError::ConnectionError(format!("sqlite checkout error: {e}"))
    })?;
    let worker_conn = SqliteConnection::new(conn);
    Ok(MiddlewarePoolConnection::Sqlite {
        conn: Some(worker_conn),
        translate_placeholders,
    })
}

#[cfg(feature = "sqlite")]
impl MiddlewarePoolConnection {
    /// Run synchronous `SQLite` work on the underlying worker-owned connection.
    ///
    /// Use this when you need to batch multiple statements in one worker hop, reuse `rusqlite`
    /// features we don't expose (savepoints, pragmas that return rows, custom hooks), or avoid
    /// re-preparing statements in hot loops. It keeps blocking work off the async runtime while
    /// letting you drive the raw `rusqlite::Connection`.
    ///
    /// The closure runs on a worker thread and must **not** capture non-`Send` state across an
    /// `await`. Do all work inside the closure and return promptly instead of holding onto the
    /// connection handle.
    ///
    /// # Errors
    /// Returns [`SqlMiddlewareDbError::Unimplemented`] when the connection is not `SQLite`.
    ///
    /// # Examples
    /// ```rust,no_run
    /// use sql_middleware::prelude::*;
    ///
    /// # async fn demo() -> Result<(), SqlMiddlewareDbError> {
    /// let cap = ConfigAndPool::new_sqlite("file::memory:?cache=shared".into()).await?;
    /// let mut conn = cap.get_connection().await?;
    /// conn.with_blocking_sqlite(|raw| {
    ///     raw.execute_batch("CREATE TABLE t (id INTEGER, name TEXT);")?;
    ///     Ok::<_, SqlMiddlewareDbError>(())
    /// })
    /// .await?;
    /// # Ok(()) }
    /// ```
    pub async fn with_blocking_sqlite<F, R>(&mut self, func: F) -> Result<R, SqlMiddlewareDbError>
    where
        F: FnOnce(&mut rusqlite::Connection) -> Result<R, SqlMiddlewareDbError> + Send + 'static,
        R: Send + 'static,
    {
        let conn = self.sqlite_conn_mut()?;
        conn.with_connection(func).await
    }

    /// Prepare a `SQLite` statement and obtain a reusable handle backed by the worker thread.
    ///
    /// # Errors
    /// Returns [`SqlMiddlewareDbError::Unimplemented`] when the underlying connection is not
    /// `SQLite`, or propagates any preparation error reported by the worker thread.
    ///
    /// The returned handle borrows the worker-owned connection. Use it within the async scope that
    /// created it; do not move it across tasks or hold it across long `await` chains.
    ///
    /// # Examples
    /// ```rust,no_run
    /// use sql_middleware::prelude::*;
    ///
    /// # async fn demo() -> Result<(), SqlMiddlewareDbError> {
    /// let cap = ConfigAndPool::new_sqlite("file::memory:?cache=shared".into()).await?;
    /// let mut conn = cap.get_connection().await?;
    /// conn.execute_batch("CREATE TABLE t (id INTEGER, name TEXT)").await?;
    ///
    /// let prepared = conn
    ///     .prepare_sqlite_statement("INSERT INTO t (id, name) VALUES (?1, ?2)")
    ///     .await?;
    /// prepared
    ///     .execute(&[RowValues::Int(1), RowValues::Text("alice".into())])
    ///     .await?;
    /// # Ok(()) }
    /// ```
    pub async fn prepare_sqlite_statement(
        &mut self,
        query: &str,
    ) -> Result<SqlitePreparedStatement<'_>, SqlMiddlewareDbError> {
        let conn = self.sqlite_conn_mut()?;
        conn.prepare_statement(query).await
    }

    pub(crate) fn sqlite_conn_mut(
        &mut self,
    ) -> Result<&mut SqliteConnection, SqlMiddlewareDbError> {
        match self {
            MiddlewarePoolConnection::Sqlite { conn, .. } => conn.as_mut().ok_or_else(|| {
                SqlMiddlewareDbError::ExecutionError(
                    "SQLite connection already taken from pool wrapper".into(),
                )
            }),
            #[cfg(any(feature = "postgres", feature = "mssql", feature = "turso"))]
            _ => Err(SqlMiddlewareDbError::Unimplemented(
                "SQLite helper called on non-sqlite connection".into(),
            )),
        }
    }

    /// Extract the `SQLite` connection, returning the translation flag alongside it.
    ///
    /// # Errors
    /// Returns `SqlMiddlewareDbError` if the connection is already taken or the enum is not `SQLite`.
    pub fn into_sqlite(self) -> Result<(SqliteConnection, bool), SqlMiddlewareDbError> {
        match self {
            MiddlewarePoolConnection::Sqlite {
                mut conn,
                translate_placeholders,
            } => conn
                .take()
                .map(|conn| (conn, translate_placeholders))
                .ok_or_else(|| {
                    SqlMiddlewareDbError::ExecutionError(
                        "SQLite connection already taken from pool wrapper".into(),
                    )
                }),
            #[cfg(any(feature = "postgres", feature = "mssql", feature = "turso"))]
            _ => Err(SqlMiddlewareDbError::Unimplemented(
                "into_sqlite is only available for SQLite connections".to_string(),
            )),
        }
    }

    /// Rewrap a `SQLite` connection back into the enum with the original translation flag.
    #[must_use]
    pub fn from_sqlite_parts(
        conn: SqliteConnection,
        translate_placeholders: bool,
    ) -> MiddlewarePoolConnection {
        MiddlewarePoolConnection::Sqlite {
            conn: Some(conn),
            translate_placeholders,
        }
    }
}
