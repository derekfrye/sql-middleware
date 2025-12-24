#[cfg(feature = "postgres")]
use bb8::Pool;

#[cfg(feature = "postgres")]
use crate::error::SqlMiddlewareDbError;
#[cfg(feature = "postgres")]
use crate::postgres::typed::PgManager;

#[cfg(feature = "postgres")]
use super::MiddlewarePoolConnection;

#[cfg(feature = "postgres")]
pub(super) async fn get_connection(
    pool: &Pool<PgManager>,
    translate_placeholders: bool,
) -> Result<MiddlewarePoolConnection, SqlMiddlewareDbError> {
    let conn = pool
        .get_owned()
        .await
        .map_err(SqlMiddlewareDbError::PoolErrorPostgres)?;
    Ok(MiddlewarePoolConnection::Postgres {
        client: conn,
        translate_placeholders,
    })
}
