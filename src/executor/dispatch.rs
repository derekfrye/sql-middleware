use crate::error::SqlMiddlewareDbError;
use crate::pool::MiddlewarePoolConnection;
use crate::query_builder::QueryBuilder;
use crate::results::ResultSet;
use crate::types::RowValues;

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

pub(crate) async fn execute_select_dispatch(
    conn: &mut MiddlewarePoolConnection,
    query: &str,
    params: &[RowValues],
) -> Result<ResultSet, SqlMiddlewareDbError> {
    match conn {
        #[cfg(feature = "postgres")]
        MiddlewarePoolConnection::Postgres {
            client: pg_client, ..
        } => postgres::execute_query_on_client(pg_client, query, params).await,
        #[cfg(feature = "sqlite")]
        MiddlewarePoolConnection::Sqlite { .. } => {
            let sqlite_client = conn.sqlite_conn_mut()?;
            sqlite::execute_select(sqlite_client, query, params).await
        }
        #[cfg(feature = "mssql")]
        MiddlewarePoolConnection::Mssql {
            conn: mssql_client, ..
        } => mssql::execute_select(mssql_client, query, params).await,
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

pub(crate) async fn execute_select_prepared_dispatch(
    conn: &mut MiddlewarePoolConnection,
    query: &str,
    params: &[RowValues],
) -> Result<ResultSet, SqlMiddlewareDbError> {
    match conn {
        #[cfg(feature = "postgres")]
        MiddlewarePoolConnection::Postgres {
            client: pg_client, ..
        } => postgres::query::execute_query_prepared_on_client(pg_client, query, params).await,
        #[cfg(feature = "sqlite")]
        MiddlewarePoolConnection::Sqlite { .. } => {
            let sqlite_client = conn.sqlite_conn_mut()?;
            sqlite::execute_select(sqlite_client, query, params).await
        }
        #[cfg(feature = "mssql")]
        MiddlewarePoolConnection::Mssql {
            conn: mssql_client, ..
        } => mssql::execute_select(mssql_client, query, params).await,
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
        } => {
            postgres::execute_dml_on_client(pg_client, query, params, "postgres execute error")
                .await
        }
        #[cfg(feature = "sqlite")]
        MiddlewarePoolConnection::Sqlite { .. } => {
            let sqlite_client = conn.sqlite_conn_mut()?;
            sqlite::execute_dml(sqlite_client, query, params).await
        }
        #[cfg(feature = "mssql")]
        MiddlewarePoolConnection::Mssql {
            conn: mssql_client, ..
        } => mssql::execute_dml(mssql_client, query, params).await,
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

pub(crate) async fn execute_dml_prepared_dispatch(
    conn: &mut MiddlewarePoolConnection,
    query: &str,
    params: &[RowValues],
) -> Result<usize, SqlMiddlewareDbError> {
    match conn {
        #[cfg(feature = "postgres")]
        MiddlewarePoolConnection::Postgres {
            client: pg_client, ..
        } => postgres::query::execute_dml_prepared_on_client(pg_client, query, params).await,
        #[cfg(feature = "sqlite")]
        MiddlewarePoolConnection::Sqlite { .. } => {
            let sqlite_client = conn.sqlite_conn_mut()?;
            sqlite::execute_dml(sqlite_client, query, params).await
        }
        #[cfg(feature = "mssql")]
        MiddlewarePoolConnection::Mssql {
            conn: mssql_client, ..
        } => mssql::execute_dml(mssql_client, query, params).await,
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
