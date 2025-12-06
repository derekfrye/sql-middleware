use crate::middleware::SqlMiddlewareDbError;

use super::{SqliteConnection, run_blocking};

impl SqliteConnection {
    /// Prepare a statement for repeated execution (auto-commit mode only).
    ///
    /// # Errors
    /// Returns `SqlMiddlewareDbError` if preparing the statement fails or a transaction is active.
    pub async fn prepare_statement(
        &mut self,
        query: &str,
    ) -> Result<crate::sqlite::prepared::SqlitePreparedStatement<'_>, SqlMiddlewareDbError> {
        if self.in_transaction {
            return Err(SqlMiddlewareDbError::ExecutionError(
                "SQLite transaction in progress; operation not permitted (prepare statement)"
                    .into(),
            ));
        }
        let query_arc = std::sync::Arc::new(query.to_owned());
        // warm the cache so repeated executions don't re-prepare.
        let query_clone = std::sync::Arc::clone(&query_arc);
        run_blocking(self.conn_handle(), move |guard| {
            let _ = guard
                .prepare_cached(query_clone.as_ref())
                .map_err(SqlMiddlewareDbError::SqliteError)?;
            Ok(())
        })
        .await?;
        Ok(crate::sqlite::prepared::SqlitePreparedStatement::new(
            self, query_arc,
        ))
    }
}
