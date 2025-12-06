#[cfg(feature = "turso")]
use crate::error::SqlMiddlewareDbError;
#[cfg(feature = "turso")]
use crate::turso::TursoNonTxPreparedStatement;
#[cfg(feature = "turso")]
use turso::Connection as TursoConnection;
#[cfg(feature = "turso")]
use turso::Database;

#[cfg(feature = "turso")]
use super::MiddlewarePoolConnection;

#[cfg(feature = "turso")]
pub(super) async fn get_connection(
    db: &Database,
    translate_placeholders: bool,
) -> Result<MiddlewarePoolConnection, SqlMiddlewareDbError> {
    let conn: TursoConnection = db.connect().map_err(SqlMiddlewareDbError::from)?;
    Ok(MiddlewarePoolConnection::Turso {
        conn,
        translate_placeholders,
    })
}

#[cfg(feature = "turso")]
impl MiddlewarePoolConnection {
    pub async fn prepare_turso_statement(
        &mut self,
        query: &str,
    ) -> Result<TursoNonTxPreparedStatement, SqlMiddlewareDbError> {
        match self {
            MiddlewarePoolConnection::Turso {
                conn: turso_conn, ..
            } => TursoNonTxPreparedStatement::prepare(turso_conn.clone(), query).await,
            _ => Err(SqlMiddlewareDbError::Unimplemented(
                "prepare_turso_statement is only available for Turso connections".to_string(),
            )),
        }
    }
}
