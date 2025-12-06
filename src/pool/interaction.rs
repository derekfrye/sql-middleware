use super::connection::MiddlewarePoolConnection;
use crate::error::SqlMiddlewareDbError;
use crate::pool::AnyConnWrapper;

impl MiddlewarePoolConnection {
    /// Interact with the connection asynchronously
    ///
    /// # Errors
    /// Returns `SqlMiddlewareDbError::Unimplemented` for unsupported database types.
    #[allow(unused_variables)]
    pub async fn interact_async<F, Fut>(
        &mut self,
        func: F,
    ) -> Result<Fut::Output, SqlMiddlewareDbError>
    where
        F: FnOnce(AnyConnWrapper<'_>) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = Result<(), SqlMiddlewareDbError>> + Send + 'static,
    {
        match self {
            #[cfg(feature = "postgres")]
            MiddlewarePoolConnection::Postgres { client: pg_obj, .. } => {
                // `PooledConnection` dereferences to the underlying `tokio_postgres::Client`.
                let client: &mut tokio_postgres::Client = &mut **pg_obj;
                Ok(func(AnyConnWrapper::Postgres(client)).await)
            }
            #[cfg(feature = "mssql")]
            MiddlewarePoolConnection::Mssql {
                conn: mssql_obj, ..
            } => {
                // Get client from Object
                let client = &mut **mssql_obj;
                Ok(func(AnyConnWrapper::Mssql(client)).await)
            }
            #[cfg(feature = "libsql")]
            MiddlewarePoolConnection::Libsql {
                conn: libsql_obj, ..
            } => Ok(func(AnyConnWrapper::Libsql(libsql_obj)).await),
            #[cfg(feature = "sqlite")]
            MiddlewarePoolConnection::Sqlite { .. } => Err(SqlMiddlewareDbError::Unimplemented(
                "interact_async is not supported for SQLite; use interact_sync instead".to_string(),
            )),
            #[allow(unreachable_patterns)]
            _ => Err(SqlMiddlewareDbError::Unimplemented(
                "interact_async is not implemented for this database type".to_string(),
            )),
        }
    }

    /// Interact with the connection synchronously
    ///
    /// # Errors
    /// Returns `SqlMiddlewareDbError::Unimplemented` for unsupported database types.
    #[allow(unused_variables)]
    pub async fn interact_sync<F, R>(&self, f: F) -> Result<R, SqlMiddlewareDbError>
    where
        F: FnOnce(AnyConnWrapper) -> R + Send + 'static,
        R: Send + 'static,
    {
        match self {
            #[cfg(feature = "sqlite")]
            MiddlewarePoolConnection::Sqlite {
                conn: sqlite_conn, ..
            } => {
                let conn = sqlite_conn.as_ref().ok_or_else(|| {
                    SqlMiddlewareDbError::ExecutionError(
                        "SQLite connection already taken from pool wrapper".into(),
                    )
                })?;

                conn.with_connection(move |conn| {
                    let wrapper = AnyConnWrapper::Sqlite(conn);
                    Ok(f(wrapper))
                })
                .await
            }
            #[cfg(feature = "postgres")]
            MiddlewarePoolConnection::Postgres { .. } => Err(SqlMiddlewareDbError::Unimplemented(
                "interact_sync is not supported for Postgres; use interact_async instead"
                    .to_string(),
            )),
            #[cfg(feature = "mssql")]
            MiddlewarePoolConnection::Mssql { .. } => Err(SqlMiddlewareDbError::Unimplemented(
                "interact_sync is not supported for SQL Server; use interact_async instead"
                    .to_string(),
            )),
            #[allow(unreachable_patterns)]
            _ => Err(SqlMiddlewareDbError::Unimplemented(
                "interact_sync is not implemented for this database type".to_string(),
            )),
        }
    }
}
