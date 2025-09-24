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

/// Open a transaction on the supplied libsql connection.
///
/// # Errors
/// Returns an error if the `BEGIN` statement fails.
pub async fn begin_transaction(conn: &Object) -> Result<Tx<'_>, SqlMiddlewareDbError> {
    conn.execute_batch("BEGIN").await.map_err(|error| {
        SqlMiddlewareDbError::ExecutionError(format!("libsql begin tx error: {error}"))
    })?;
    Ok(Tx { conn })
}

impl Tx<'_> {
    /// Create a prepared statement bound to this transaction.
    ///
    /// # Errors
    /// Currently infallible; matches the rest of the transaction API signature expectations.
    pub fn prepare(&self, sql: &str) -> Result<Prepared, SqlMiddlewareDbError> {
        Ok(Prepared {
            sql: sql.to_owned(),
        })
    }

    /// Commit the active transaction.
    ///
    /// # Errors
    /// Returns an error if the `COMMIT` statement fails.
    pub async fn commit(&self) -> Result<(), SqlMiddlewareDbError> {
        let _ = self.conn.execute_batch("COMMIT").await.map_err(|error| {
            SqlMiddlewareDbError::ExecutionError(format!("libsql commit error: {error}"))
        })?;
        Ok(())
    }

    /// Roll back the active transaction.
    ///
    /// # Errors
    /// Returns an error if the `ROLLBACK` statement fails.
    pub async fn rollback(&self) -> Result<(), SqlMiddlewareDbError> {
        let _ = self.conn.execute_batch("ROLLBACK").await.map_err(|error| {
            SqlMiddlewareDbError::ExecutionError(format!("libsql rollback error: {error}"))
        })?;
        Ok(())
    }

    /// Execute a pre-bound statement, returning the affected row count.
    ///
    /// # Errors
    /// Returns an error if parameter conversion fails, the execution fails, or the affected-row
    /// count cannot be converted to `usize`.
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
            .map_err(|error| {
                SqlMiddlewareDbError::ExecutionError(format!(
                    "libsql execute(prepared) error: {error}"
                ))
            })?;
        usize::try_from(affected).map_err(|e| {
            SqlMiddlewareDbError::ExecutionError(format!(
                "libsql affected rows conversion error: {e}"
            ))
        })
    }

    /// Execute a query and build a `ResultSet` from the returned rows.
    ///
    /// # Errors
    /// Returns an error when parameter conversion fails, the query execution fails, or result-set
    /// materialisation encounters a downstream error.
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
            .map_err(|error| {
                SqlMiddlewareDbError::ExecutionError(format!(
                    "libsql query(prepared) error: {error}"
                ))
            })?;
        build_result_set(rows).await
    }
}
