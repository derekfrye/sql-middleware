use crate::error::SqlMiddlewareDbError;
use crate::pool::MiddlewarePoolConnection;
use crate::query_builder::QueryBuilder;

#[cfg(feature = "mssql")]
use crate::mssql;
#[cfg(feature = "postgres")]
use crate::postgres;
#[cfg(feature = "sqlite")]
use crate::sqlite;
#[cfg(feature = "turso")]
use crate::turso;

impl MiddlewarePoolConnection {
    /// Executes a batch of SQL queries within a transaction by delegating to the specific database module.
    ///
    /// # Errors
    /// Returns an error if the selected backend cannot execute the batch or the database responds with an error.
    pub async fn execute_batch(&mut self, query: &str) -> Result<(), SqlMiddlewareDbError> {
        match self {
            #[cfg(feature = "postgres")]
            MiddlewarePoolConnection::Postgres {
                client: pg_client, ..
            } => postgres::execute_batch(pg_client, query).await,
            #[cfg(feature = "sqlite")]
            MiddlewarePoolConnection::Sqlite { .. } => {
                let sqlite_client = self.sqlite_conn_mut()?;
                sqlite::execute_batch(sqlite_client, query).await
            }
            #[cfg(feature = "mssql")]
            MiddlewarePoolConnection::Mssql {
                conn: mssql_client, ..
            } => mssql::execute_batch(mssql_client, query).await,
            #[cfg(feature = "turso")]
            MiddlewarePoolConnection::Turso {
                conn: turso_conn, ..
            } => turso::execute_batch(turso_conn, query).await,
            #[allow(unreachable_patterns)]
            _ => Err(SqlMiddlewareDbError::Unimplemented(
                "This database type is not enabled in the current build".to_string(),
            )),
        }
    }

    /// Start a fluent query builder that can translate placeholders before executing.
    ///
    /// # Examples
    /// ```rust,no_run
    /// use sql_middleware::prelude::*;
    ///
    /// # async fn demo() -> Result<(), SqlMiddlewareDbError> {
    /// let cap = ConfigAndPool::new_sqlite("file::memory:?cache=shared".into()).await?;
    /// let mut conn = cap.get_connection().await?;
    /// conn.execute_batch("CREATE TABLE t (id INTEGER)").await?;
    ///
    /// let rows = conn
    ///     .query("SELECT id FROM t WHERE id = ?1")
    ///     .params(&[RowValues::Int(1)])
    ///     .select()
    ///     .await?;
    /// assert!(rows.results.is_empty());
    /// # Ok(()) }
    /// ```
    pub fn query<'a>(&'a mut self, query: &'a str) -> QueryBuilder<'a, 'a> {
        QueryBuilder::new(self, query)
    }
}
