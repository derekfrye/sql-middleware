#[cfg(feature = "libsql")]
use crate::error::SqlMiddlewareDbError;
#[cfg(feature = "libsql")]
use deadpool_libsql::Pool as LibsqlPool;

#[cfg(feature = "libsql")]
use super::MiddlewarePoolConnection;

#[cfg(feature = "libsql")]
pub(super) async fn get_connection(
    pool: &LibsqlPool,
    translate_placeholders: bool,
) -> Result<MiddlewarePoolConnection, SqlMiddlewareDbError> {
    let conn = pool
        .get()
        .await
        .map_err(SqlMiddlewareDbError::PoolErrorLibsql)?;
    Ok(MiddlewarePoolConnection::Libsql {
        conn,
        translate_placeholders,
    })
}
