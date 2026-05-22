#[cfg(feature = "mssql")]
use bb8::Pool;
#[cfg(feature = "mssql")]
use bb8_tiberius::ConnectionManager;

#[cfg(feature = "mssql")]
use crate::error::SqlMiddlewareDbError;
#[cfg(feature = "mssql")]
use crate::types::StatementCacheMode;

#[cfg(feature = "mssql")]
use super::MiddlewarePoolConnection;

#[cfg(feature = "mssql")]
pub(super) async fn get_connection(
    pool: &Pool<ConnectionManager>,
    translate_placeholders: bool,
    statement_cache_mode: StatementCacheMode,
) -> Result<MiddlewarePoolConnection, SqlMiddlewareDbError> {
    let conn = pool
        .get_owned()
        .await
        .map_err(SqlMiddlewareDbError::PoolErrorMssql)?;
    Ok(MiddlewarePoolConnection::Mssql {
        conn,
        translate_placeholders,
        statement_cache_mode,
    })
}
