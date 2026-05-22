#[cfg(feature = "turso")]
use crate::error::SqlMiddlewareDbError;
#[cfg(feature = "turso")]
use crate::turso::{TursoNonTxPreparedStatement, typed::TursoManager};
#[cfg(feature = "turso")]
use crate::types::StatementCacheMode;
#[cfg(feature = "turso")]
use bb8::Pool;

#[cfg(feature = "turso")]
use super::MiddlewarePoolConnection;

#[cfg(feature = "turso")]
pub(super) async fn get_connection(
    pool: &Pool<TursoManager>,
    translate_placeholders: bool,
    statement_cache_mode: StatementCacheMode,
) -> Result<MiddlewarePoolConnection, SqlMiddlewareDbError> {
    let conn = pool
        .get_owned()
        .await
        .map_err(|e| SqlMiddlewareDbError::ConnectionError(format!("turso checkout error: {e}")))?;
    Ok(MiddlewarePoolConnection::Turso {
        conn,
        translate_placeholders,
        statement_cache_mode,
    })
}

#[cfg(feature = "turso")]
impl MiddlewarePoolConnection {
    /// Prepare a Turso statement on this connection.
    ///
    /// # Errors
    /// Returns `SqlMiddlewareDbError` if the statement preparation fails or the connection is not Turso-backed.
    pub async fn prepare_turso_statement(
        &mut self,
        query: &str,
    ) -> Result<TursoNonTxPreparedStatement, SqlMiddlewareDbError> {
        let statement_cache_mode = self.statement_cache_mode_default();
        match self {
            MiddlewarePoolConnection::Turso {
                conn: turso_conn, ..
            } => {
                TursoNonTxPreparedStatement::prepare_with_cache_mode(
                    (**turso_conn).clone(),
                    query,
                    statement_cache_mode,
                )
                .await
            }
            #[cfg(any(feature = "postgres", feature = "sqlite", feature = "mssql"))]
            _ => Err(SqlMiddlewareDbError::Unimplemented(
                "prepare_turso_statement is only available for Turso connections".to_string(),
            )),
        }
    }
}
