use crate::error::SqlMiddlewareDbError;
use crate::pool::MiddlewarePoolConnection;
use crate::query_builder::QueryBuilder;
use crate::results::ResultSet;
use crate::types::RowValues;

#[cfg(feature = "libsql")]
use crate::libsql;
#[cfg(feature = "mssql")]
use crate::mssql;
#[cfg(feature = "postgres")]
use crate::postgres;
#[cfg(feature = "sqlite")]
use crate::sqlite;
#[cfg(feature = "turso")]
use crate::turso;

use super::targets::{BatchTarget, QueryTarget};

/// Execute a batch against either a connection or a transaction.
///
/// # Errors
/// Returns an error propagated from the underlying backend execution or transaction context.
pub async fn execute_batch(
    target: impl Into<BatchTarget<'_>>,
    query: &str,
) -> Result<(), SqlMiddlewareDbError> {
    match target.into() {
        BatchTarget::Connection(conn) => conn.execute_batch(query).await,
        #[cfg(feature = "postgres")]
        BatchTarget::PostgresTx(tx) => tx.execute_batch(query).await,
        #[cfg(feature = "mssql")]
        BatchTarget::MssqlTx(tx) => tx.execute_batch(query).await,
        #[cfg(feature = "libsql")]
        BatchTarget::LibsqlTx(tx) => tx.execute_batch(query).await,
        #[cfg(feature = "turso")]
        BatchTarget::TursoTx(tx) => tx.execute_batch(query).await,
        #[cfg(feature = "turso")]
        BatchTarget::TypedTurso { conn } => {
            crate::typed_turso::dml(conn, query, &[]).await?;
            Ok(())
        }
        #[cfg(feature = "turso")]
        BatchTarget::TypedTursoTx { conn } => {
            crate::typed_turso::dml(conn, query, &[]).await?;
            Ok(())
        }
    }
}

/// Start a fluent builder for either a connection or a transaction.
pub fn query<'a>(target: impl Into<QueryTarget<'a>>, sql: &'a str) -> QueryBuilder<'a, 'a> {
    QueryBuilder::new_target(target.into(), sql)
}

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
            #[cfg(feature = "libsql")]
            MiddlewarePoolConnection::Libsql {
                conn: libsql_client,
                ..
            } => libsql::execute_batch(libsql_client, query).await,
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

pub(crate) async fn execute_select_dispatch(
    conn: &mut MiddlewarePoolConnection,
    query: &str,
    params: &[RowValues],
) -> Result<ResultSet, SqlMiddlewareDbError> {
    match conn {
        #[cfg(feature = "postgres")]
        MiddlewarePoolConnection::Postgres {
            client: pg_client, ..
        } => postgres::execute_select(pg_client, query, params).await,
        #[cfg(feature = "sqlite")]
        MiddlewarePoolConnection::Sqlite { .. } => {
            let sqlite_client = conn.sqlite_conn_mut()?;
            sqlite::execute_select(sqlite_client, query, params).await
        }
        #[cfg(feature = "mssql")]
        MiddlewarePoolConnection::Mssql {
            conn: mssql_client, ..
        } => mssql::execute_select(mssql_client, query, params).await,
        #[cfg(feature = "libsql")]
        MiddlewarePoolConnection::Libsql {
            conn: libsql_client,
            ..
        } => libsql::execute_select(libsql_client, query, params).await,
        #[cfg(feature = "turso")]
        MiddlewarePoolConnection::Turso {
            conn: turso_conn, ..
        } => turso::execute_select(turso_conn, query, params).await,
        #[allow(unreachable_patterns)]
        _ => Err(SqlMiddlewareDbError::Unimplemented(
            "This database type is not enabled in the current build".to_string(),
        )),
    }
}

pub(crate) async fn execute_dml_dispatch(
    conn: &mut MiddlewarePoolConnection,
    query: &str,
    params: &[RowValues],
) -> Result<usize, SqlMiddlewareDbError> {
    match conn {
        #[cfg(feature = "postgres")]
        MiddlewarePoolConnection::Postgres {
            client: pg_client, ..
        } => postgres::execute_dml(pg_client, query, params).await,
        #[cfg(feature = "sqlite")]
        MiddlewarePoolConnection::Sqlite { .. } => {
            let sqlite_client = conn.sqlite_conn_mut()?;
            sqlite::execute_dml(sqlite_client, query, params).await
        }
        #[cfg(feature = "mssql")]
        MiddlewarePoolConnection::Mssql {
            conn: mssql_client, ..
        } => mssql::execute_dml(mssql_client, query, params).await,
        #[cfg(feature = "libsql")]
        MiddlewarePoolConnection::Libsql {
            conn: libsql_client,
            ..
        } => libsql::execute_dml(libsql_client, query, params).await,
        #[cfg(feature = "turso")]
        MiddlewarePoolConnection::Turso {
            conn: turso_conn, ..
        } => turso::execute_dml(turso_conn, query, params).await,
        #[allow(unreachable_patterns)]
        _ => Err(SqlMiddlewareDbError::Unimplemented(
            "This database type is not enabled in the current build".to_string(),
        )),
    }
}
