use sql_middleware::SqlMiddlewareDbError;

use crate::backends::BackendError;

pub(super) fn is_busy_error(err: &BackendError) -> bool {
    match err {
        BackendError::Sql(SqlMiddlewareDbError::SqliteError(rusqlite::Error::SqliteFailure(
            code,
            _,
        ))) => matches!(
            code.code,
            rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked
        ),
        _ => false,
    }
}
