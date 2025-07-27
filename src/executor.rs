use async_trait::async_trait;

use crate::error::SqlMiddlewareDbError;
use crate::pool::MiddlewarePoolConnection;
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

#[async_trait]
pub trait AsyncDatabaseExecutor {
    /// Executes a batch of SQL queries (can be a mix of reads/writes) within a transaction. No parameters are supported.
    async fn execute_batch(&mut self, query: &str) -> Result<(), SqlMiddlewareDbError>;

    /// Executes a single SELECT statement and returns the result set.
    async fn execute_select(
        &mut self,
        query: &str,
        params: &[RowValues],
    ) -> Result<ResultSet, SqlMiddlewareDbError>;

    /// Executes a single DML statement (INSERT, UPDATE, DELETE, etc.) and returns the number of rows affected.
    async fn execute_dml(
        &mut self,
        query: &str,
        params: &[RowValues],
    ) -> Result<usize, SqlMiddlewareDbError>;
}

#[async_trait]
impl AsyncDatabaseExecutor for MiddlewarePoolConnection {
    /// Executes a batch of SQL queries within a transaction by delegating to the specific database module.
    async fn execute_batch(&mut self, query: &str) -> Result<(), SqlMiddlewareDbError> {
        match self {
            #[cfg(feature = "postgres")]
            MiddlewarePoolConnection::Postgres(pg_client) => {
                postgres::execute_batch(pg_client, query).await
            }
            #[cfg(feature = "sqlite")]
            MiddlewarePoolConnection::Sqlite(sqlite_client) => {
                sqlite::execute_batch(sqlite_client, query).await
            }
            #[cfg(feature = "mssql")]
            MiddlewarePoolConnection::Mssql(mssql_client) => {
                mssql::execute_batch(mssql_client, query).await
            }
            #[cfg(feature = "libsql")]
            MiddlewarePoolConnection::Libsql(libsql_client) => {
                libsql::execute_batch(libsql_client, query).await
            }
            #[allow(unreachable_patterns)]
            _ => Err(SqlMiddlewareDbError::Unimplemented(
                "This database type is not enabled in the current build".to_string(),
            )),
        }
    }

    async fn execute_select(
        &mut self,
        query: &str,
        params: &[RowValues],
    ) -> Result<ResultSet, SqlMiddlewareDbError> {
        match self {
            #[cfg(feature = "postgres")]
            MiddlewarePoolConnection::Postgres(pg_client) => {
                postgres::execute_select(pg_client, query, params).await
            }
            #[cfg(feature = "sqlite")]
            MiddlewarePoolConnection::Sqlite(sqlite_client) => {
                sqlite::execute_select(sqlite_client, query, params).await
            }
            #[cfg(feature = "mssql")]
            MiddlewarePoolConnection::Mssql(mssql_client) => {
                mssql::execute_select(mssql_client, query, params).await
            }
            #[cfg(feature = "libsql")]
            MiddlewarePoolConnection::Libsql(libsql_client) => {
                libsql::execute_select(libsql_client, query, params).await
            }
            #[allow(unreachable_patterns)]
            _ => Err(SqlMiddlewareDbError::Unimplemented(
                "This database type is not enabled in the current build".to_string(),
            )),
        }
    }

    async fn execute_dml(
        &mut self,
        query: &str,
        params: &[RowValues],
    ) -> Result<usize, SqlMiddlewareDbError> {
        match self {
            #[cfg(feature = "postgres")]
            MiddlewarePoolConnection::Postgres(pg_client) => {
                postgres::execute_dml(pg_client, query, params).await
            }
            #[cfg(feature = "sqlite")]
            MiddlewarePoolConnection::Sqlite(sqlite_client) => {
                sqlite::execute_dml(sqlite_client, query, params).await
            }
            #[cfg(feature = "mssql")]
            MiddlewarePoolConnection::Mssql(mssql_client) => {
                mssql::execute_dml(mssql_client, query, params).await
            }
            #[cfg(feature = "libsql")]
            MiddlewarePoolConnection::Libsql(libsql_client) => {
                libsql::execute_dml(libsql_client, query, params).await
            }
            #[allow(unreachable_patterns)]
            _ => Err(SqlMiddlewareDbError::Unimplemented(
                "This database type is not enabled in the current build".to_string(),
            )),
        }
    }
}
