use std::borrow::Cow;

use async_trait::async_trait;

use crate::error::SqlMiddlewareDbError;
use crate::pool::MiddlewarePoolConnection;
use crate::results::ResultSet;
use crate::translation::{PlaceholderStyle, QueryOptions, translate_placeholders};
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

#[async_trait]
pub trait AsyncDatabaseExecutor {
    /// Executes a batch of SQL queries (can be a mix of reads/writes) within a transaction. No parameters are supported.
    async fn execute_batch(&mut self, query: &str) -> Result<(), SqlMiddlewareDbError>;

    /// Executes a single SELECT statement and returns the result set.
    async fn execute_select(
        &mut self,
        query: &str,
        params: &[RowValues],
    ) -> Result<ResultSet, SqlMiddlewareDbError> {
        self.execute_select_with_options(query, params, QueryOptions::default())
            .await
    }

    /// Executes a single SELECT statement with override options and returns the result set.
    async fn execute_select_with_options(
        &mut self,
        query: &str,
        params: &[RowValues],
        options: QueryOptions,
    ) -> Result<ResultSet, SqlMiddlewareDbError>;

    /// Executes a single DML statement (INSERT, UPDATE, DELETE, etc.) and returns the number of rows affected.
    async fn execute_dml(
        &mut self,
        query: &str,
        params: &[RowValues],
    ) -> Result<usize, SqlMiddlewareDbError> {
        self.execute_dml_with_options(query, params, QueryOptions::default())
            .await
    }

    /// Executes a single DML statement (INSERT, UPDATE, DELETE, etc.) with override options and returns the number of rows affected.
    async fn execute_dml_with_options(
        &mut self,
        query: &str,
        params: &[RowValues],
        options: QueryOptions,
    ) -> Result<usize, SqlMiddlewareDbError>;
}

#[async_trait]
impl AsyncDatabaseExecutor for MiddlewarePoolConnection {
    /// Executes a batch of SQL queries within a transaction by delegating to the specific database module.
    async fn execute_batch(&mut self, query: &str) -> Result<(), SqlMiddlewareDbError> {
        match self {
            #[cfg(feature = "postgres")]
            MiddlewarePoolConnection::Postgres {
                client: pg_client, ..
            } => postgres::execute_batch(pg_client, query).await,
            #[cfg(feature = "sqlite")]
            MiddlewarePoolConnection::Sqlite {
                conn: sqlite_client,
                ..
            } => sqlite::execute_batch(sqlite_client, query).await,
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

    async fn execute_select_with_options(
        &mut self,
        query: &str,
        params: &[RowValues],
        options: QueryOptions,
    ) -> Result<ResultSet, SqlMiddlewareDbError> {
        let translated = translate_query(self, query, params, options);
        match self {
            #[cfg(feature = "postgres")]
            MiddlewarePoolConnection::Postgres {
                client: pg_client, ..
            } => postgres::execute_select(pg_client, translated.as_ref(), params).await,
            #[cfg(feature = "sqlite")]
            MiddlewarePoolConnection::Sqlite {
                conn: sqlite_client,
                ..
            } => sqlite::execute_select(sqlite_client, translated.as_ref(), params).await,
            #[cfg(feature = "mssql")]
            MiddlewarePoolConnection::Mssql {
                conn: mssql_client, ..
            } => mssql::execute_select(mssql_client, translated.as_ref(), params).await,
            #[cfg(feature = "libsql")]
            MiddlewarePoolConnection::Libsql {
                conn: libsql_client,
                ..
            } => libsql::execute_select(libsql_client, translated.as_ref(), params).await,
            #[cfg(feature = "turso")]
            MiddlewarePoolConnection::Turso {
                conn: turso_conn, ..
            } => turso::execute_select(turso_conn, translated.as_ref(), params).await,
            #[allow(unreachable_patterns)]
            _ => Err(SqlMiddlewareDbError::Unimplemented(
                "This database type is not enabled in the current build".to_string(),
            )),
        }
    }

    async fn execute_dml_with_options(
        &mut self,
        query: &str,
        params: &[RowValues],
        options: QueryOptions,
    ) -> Result<usize, SqlMiddlewareDbError> {
        let translated = translate_query(self, query, params, options);
        match self {
            #[cfg(feature = "postgres")]
            MiddlewarePoolConnection::Postgres {
                client: pg_client, ..
            } => postgres::execute_dml(pg_client, translated.as_ref(), params).await,
            #[cfg(feature = "sqlite")]
            MiddlewarePoolConnection::Sqlite {
                conn: sqlite_client,
                ..
            } => sqlite::execute_dml(sqlite_client, translated.as_ref(), params).await,
            #[cfg(feature = "mssql")]
            MiddlewarePoolConnection::Mssql {
                conn: mssql_client, ..
            } => mssql::execute_dml(mssql_client, translated.as_ref(), params).await,
            #[cfg(feature = "libsql")]
            MiddlewarePoolConnection::Libsql {
                conn: libsql_client,
                ..
            } => libsql::execute_dml(libsql_client, translated.as_ref(), params).await,
            #[cfg(feature = "turso")]
            MiddlewarePoolConnection::Turso {
                conn: turso_conn, ..
            } => turso::execute_dml(turso_conn, translated.as_ref(), params).await,
            #[allow(unreachable_patterns)]
            _ => Err(SqlMiddlewareDbError::Unimplemented(
                "This database type is not enabled in the current build".to_string(),
            )),
        }
    }
}

fn translation_target(conn: &MiddlewarePoolConnection) -> Option<PlaceholderStyle> {
    match conn {
        #[cfg(feature = "postgres")]
        MiddlewarePoolConnection::Postgres { .. } => Some(PlaceholderStyle::Postgres),
        #[cfg(feature = "sqlite")]
        MiddlewarePoolConnection::Sqlite { .. } => Some(PlaceholderStyle::Sqlite),
        #[cfg(feature = "libsql")]
        MiddlewarePoolConnection::Libsql { .. } => Some(PlaceholderStyle::Sqlite),
        #[cfg(feature = "turso")]
        MiddlewarePoolConnection::Turso { .. } => Some(PlaceholderStyle::Sqlite),
        #[cfg(feature = "mssql")]
        MiddlewarePoolConnection::Mssql { .. } => None,
        #[allow(unreachable_patterns)]
        _ => None,
    }
}

fn translate_query<'a>(
    conn: &MiddlewarePoolConnection,
    query: &'a str,
    params: &[RowValues],
    options: QueryOptions,
) -> Cow<'a, str> {
    if params.is_empty() {
        return Cow::Borrowed(query);
    }

    let Some(target) = translation_target(conn) else {
        return Cow::Borrowed(query);
    };

    let enabled = options.translation.resolve(conn.translation_default());
    translate_placeholders(query, target, enabled)
}
