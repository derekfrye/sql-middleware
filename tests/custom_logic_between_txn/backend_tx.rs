use sql_middleware::middleware::{
    MiddlewarePoolConnection, PlaceholderStyle, RowValues, SqlMiddlewareDbError, TxOutcome,
    translate_placeholders,
};

#[cfg(feature = "postgres")]
use sql_middleware::postgres::{
    Prepared as PostgresPrepared, Tx as PostgresTx, begin_transaction as begin_postgres_tx,
};
#[cfg(feature = "sqlite")]
use sql_middleware::sqlite::{
    Prepared as SqlitePrepared, Tx as SqliteTx, begin_transaction as begin_sqlite_tx,
};
#[cfg(feature = "turso")]
use sql_middleware::turso::{
    Prepared as TursoPrepared, Tx as TursoTx, begin_transaction as begin_turso_tx,
};

enum BackendTx<'conn> {
    #[cfg(feature = "turso")]
    Turso(TursoTx<'conn>),
    #[cfg(feature = "postgres")]
    Postgres(PostgresTx<'conn>),
    #[cfg(feature = "sqlite")]
    Sqlite(SqliteTx<'conn>),
}

enum PreparedStmt {
    #[cfg(feature = "turso")]
    Turso(TursoPrepared),
    #[cfg(feature = "postgres")]
    Postgres(PostgresPrepared),
    #[cfg(feature = "sqlite")]
    Sqlite(SqlitePrepared),
}

impl BackendTx<'_> {
    async fn commit(self) -> Result<TxOutcome, SqlMiddlewareDbError> {
        match self {
            #[cfg(feature = "turso")]
            BackendTx::Turso(tx) => tx.commit().await,
            #[cfg(feature = "postgres")]
            BackendTx::Postgres(tx) => tx.commit().await,
            #[cfg(feature = "sqlite")]
            BackendTx::Sqlite(tx) => tx.commit().await,
        }
    }

    async fn rollback(self) -> Result<TxOutcome, SqlMiddlewareDbError> {
        match self {
            #[cfg(feature = "turso")]
            BackendTx::Turso(tx) => tx.rollback().await,
            #[cfg(feature = "postgres")]
            BackendTx::Postgres(tx) => tx.rollback().await,
            #[cfg(feature = "sqlite")]
            BackendTx::Sqlite(tx) => tx.rollback().await,
        }
    }
}

impl PreparedStmt {
    async fn execute(
        &mut self,
        tx: &mut BackendTx<'_>,
        params: &[RowValues],
    ) -> Result<usize, SqlMiddlewareDbError> {
        match (tx, self) {
            #[cfg(feature = "turso")]
            (BackendTx::Turso(tx), PreparedStmt::Turso(stmt)) => {
                tx.execute(stmt).params(params).run().await
            }
            #[cfg(feature = "postgres")]
            (BackendTx::Postgres(tx), PreparedStmt::Postgres(stmt)) => {
                tx.execute(stmt).params(params).run().await
            }
            #[cfg(feature = "sqlite")]
            (BackendTx::Sqlite(tx), PreparedStmt::Sqlite(stmt)) => {
                tx.execute(stmt).params(params).run().await
            }
            _ => unreachable!("transaction and prepared variants should align"),
        }
    }
}

pub(super) async fn execute_with_finalize(
    conn: &mut MiddlewarePoolConnection,
    query: &str,
    params: Vec<RowValues>,
) -> Result<usize, SqlMiddlewareDbError> {
    let (mut tx, mut stmt) = prepare_backend_tx_and_stmt(conn, query).await?;
    let result = stmt.execute(&mut tx, &params).await;
    match result {
        Ok(rows) => {
            tx.commit().await?;
            Ok(rows)
        }
        Err(e) => {
            let _ = tx.rollback().await;
            Err(e)
        }
    }
}

async fn prepare_backend_tx_and_stmt<'conn>(
    conn: &'conn mut MiddlewarePoolConnection,
    base_query: &str,
) -> Result<(BackendTx<'conn>, PreparedStmt), SqlMiddlewareDbError> {
    match conn {
        #[cfg(feature = "turso")]
        MiddlewarePoolConnection::Turso { conn, .. } => {
            let tx = begin_turso_tx(conn).await?;
            let q = translate_placeholders(base_query, PlaceholderStyle::Sqlite, true);
            let stmt = tx.prepare(q.as_ref()).await?;
            Ok((BackendTx::Turso(tx), PreparedStmt::Turso(stmt)))
        }
        #[cfg(feature = "postgres")]
        MiddlewarePoolConnection::Postgres { client, .. } => {
            let tx = begin_postgres_tx(client).await?;
            let stmt = tx.prepare(base_query).await?;
            Ok((BackendTx::Postgres(tx), PreparedStmt::Postgres(stmt)))
        }
        #[cfg(feature = "sqlite")]
        MiddlewarePoolConnection::Sqlite {
            translate_placeholders: translate_default,
            ..
        } => {
            let translate_default = *translate_default;
            let tx = begin_sqlite_tx(conn).await?;
            let q = translate_placeholders(base_query, PlaceholderStyle::Sqlite, translate_default);
            let stmt = tx.prepare(q.as_ref())?;
            Ok((BackendTx::Sqlite(tx), PreparedStmt::Sqlite(stmt)))
        }
        _ => Err(SqlMiddlewareDbError::Unimplemented(
            "expected Turso, Postgres, or SQLite connection".to_string(),
        )),
    }
}
