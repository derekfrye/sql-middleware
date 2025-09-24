use crate::libsql::{Params as LibsqlParams, build_result_set};
use crate::middleware::{ResultSet, RowValues, SqlMiddlewareDbError};
use deadpool_libsql::Object;

/// Lightweight transaction wrapper for libsql using explicit BEGIN/COMMIT.
pub struct Tx<'a> {
    conn: &'a Object,
}

/// Prepared statement wrapper for libsql.
/// This is a logical/prepared form that stores the SQL string; execution
/// uses the connection within the active transaction.
pub struct Prepared {
    sql: String,
}

pub async fn begin_transaction<'a>(conn: &'a Object) -> Result<Tx<'a>, SqlMiddlewareDbError> {
    conn.execute_batch("BEGIN")
        .await
        .map_err(|e| SqlMiddlewareDbError::ExecutionError(format!("libsql begin tx error: {e}")))?;
    Ok(Tx { conn })
}

impl<'a> Tx<'a> {
    pub async fn prepare(&self, sql: &str) -> Result<Prepared, SqlMiddlewareDbError> {
        Ok(Prepared {
            sql: sql.to_owned(),
        })
    }

    pub async fn commit(&self) -> Result<(), SqlMiddlewareDbError> {
        let _ = self.conn.execute_batch("COMMIT").await.map_err(|e| {
            SqlMiddlewareDbError::ExecutionError(format!("libsql commit error: {e}"))
        })?;
        Ok(())
    }

    pub async fn rollback(&self) -> Result<(), SqlMiddlewareDbError> {
        let _ = self.conn.execute_batch("ROLLBACK").await.map_err(|e| {
            SqlMiddlewareDbError::ExecutionError(format!("libsql rollback error: {e}"))
        })?;
        Ok(())
    }

    pub async fn execute_prepared(
        &self,
        prepared: &Prepared,
        params: &[RowValues],
    ) -> Result<usize, SqlMiddlewareDbError> {
        let converted = LibsqlParams::convert(params)?;
        let affected = self
            .conn
            .execute(&prepared.sql, converted.into_vec())
            .await
            .map_err(|e| {
                SqlMiddlewareDbError::ExecutionError(format!("libsql execute(prepared) error: {e}"))
            })?;
        usize::try_from(affected).map_err(|e| {
            SqlMiddlewareDbError::ExecutionError(format!(
                "libsql affected rows conversion error: {e}"
            ))
        })
    }

    pub async fn query_prepared(
        &self,
        prepared: &Prepared,
        params: &[RowValues],
    ) -> Result<ResultSet, SqlMiddlewareDbError> {
        let converted = LibsqlParams::convert(params)?;
        let rows = self
            .conn
            .query(&prepared.sql, converted.into_vec())
            .await
            .map_err(|e| {
                SqlMiddlewareDbError::ExecutionError(format!("libsql query(prepared) error: {e}"))
            })?;
        build_result_set(rows).await
    }
}
