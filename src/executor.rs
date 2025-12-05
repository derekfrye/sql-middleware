use crate::error::SqlMiddlewareDbError;
use crate::pool::MiddlewarePoolConnection;
use crate::query_builder::QueryBuilder;
use crate::results::ResultSet;
use crate::translation::PlaceholderStyle;
use crate::types::RowValues;

#[cfg(feature = "libsql")]
use crate::libsql;
#[cfg(feature = "mssql")]
use crate::mssql;
#[cfg(feature = "postgres")]
use crate::postgres;
#[cfg(feature = "sqlite")]
use crate::sqlite;
#[cfg(feature = "turso")]
use crate::turso;
#[cfg(any(feature = "typed-postgres", feature = "typed-turso", feature = "sqlite"))]
use bb8::PooledConnection;
#[cfg(feature = "typed-postgres")]
use crate::typed_postgres::PgManager;
#[cfg(feature = "typed-turso")]
use crate::typed_turso::TursoManager;
#[cfg(feature = "sqlite")]
use crate::sqlite::config::SqliteManager;

/// Target for batch execution (connection or transaction).
pub enum BatchTarget<'a> {
    Connection(&'a mut MiddlewarePoolConnection),
    #[cfg(feature = "postgres")]
    PostgresTx(&'a postgres::transaction::Tx<'a>),
    #[cfg(feature = "mssql")]
    MssqlTx(&'a mut mssql::transaction::Tx<'a>),
    #[cfg(feature = "libsql")]
    LibsqlTx(&'a libsql::transaction::Tx<'a>),
    #[cfg(feature = "turso")]
    TursoTx(&'a turso::transaction::Tx<'a>),
    #[cfg(feature = "typed-turso")]
    TypedTurso { conn: &'a mut PooledConnection<'static, TursoManager> },
    #[cfg(feature = "typed-turso")]
    TypedTursoTx { conn: &'a mut PooledConnection<'static, TursoManager> },
}

/// Target for query builder dispatch (connection or transaction) with a translation default.
pub struct QueryTarget<'a> {
    pub(crate) kind: QueryTargetKind<'a>,
    translation_default: bool,
}

pub(crate) enum QueryTargetKind<'a> {
    Connection(&'a mut MiddlewarePoolConnection),
    #[cfg(feature = "postgres")]
    PostgresTx(&'a postgres::transaction::Tx<'a>),
    #[cfg(feature = "sqlite")]
    TypedSqlite { conn: &'a mut PooledConnection<'static, SqliteManager> },
    #[cfg(feature = "sqlite")]
    TypedSqliteTx { conn: &'a mut PooledConnection<'static, SqliteManager> },
    #[cfg(feature = "typed-postgres")]
    TypedPostgres { conn: &'a mut PooledConnection<'static, PgManager> },
    #[cfg(feature = "typed-postgres")]
    TypedPostgresTx { conn: &'a mut PooledConnection<'static, PgManager> },
    #[cfg(feature = "typed-turso")]
    TypedTurso { conn: &'a mut PooledConnection<'static, TursoManager> },
    #[cfg(feature = "typed-turso")]
    TypedTursoTx { conn: &'a mut PooledConnection<'static, TursoManager> },
    #[cfg(feature = "mssql")]
    MssqlTx(&'a mut mssql::transaction::Tx<'a>),
    #[cfg(feature = "libsql")]
    LibsqlTx(&'a libsql::transaction::Tx<'a>),
    #[cfg(feature = "turso")]
    TursoTx(&'a turso::transaction::Tx<'a>),
}

impl<'a> From<&'a mut MiddlewarePoolConnection> for BatchTarget<'a> {
    fn from(conn: &'a mut MiddlewarePoolConnection) -> Self {
        BatchTarget::Connection(conn)
    }
}

#[cfg(feature = "postgres")]
impl<'a> From<&'a postgres::transaction::Tx<'a>> for BatchTarget<'a> {
    fn from(tx: &'a postgres::transaction::Tx<'a>) -> Self {
        BatchTarget::PostgresTx(tx)
    }
}

#[cfg(feature = "mssql")]
impl<'a> From<&'a mut mssql::transaction::Tx<'a>> for BatchTarget<'a> {
    fn from(tx: &'a mut mssql::transaction::Tx<'a>) -> Self {
        BatchTarget::MssqlTx(tx)
    }
}

#[cfg(feature = "libsql")]
impl<'a> From<&'a libsql::transaction::Tx<'a>> for BatchTarget<'a> {
    fn from(tx: &'a libsql::transaction::Tx<'a>) -> Self {
        BatchTarget::LibsqlTx(tx)
    }
}

#[cfg(feature = "turso")]
impl<'a> From<&'a turso::transaction::Tx<'a>> for BatchTarget<'a> {
    fn from(tx: &'a turso::transaction::Tx<'a>) -> Self {
        BatchTarget::TursoTx(tx)
    }
}

impl<'a> From<&'a mut MiddlewarePoolConnection> for QueryTarget<'a> {
    fn from(conn: &'a mut MiddlewarePoolConnection) -> Self {
        QueryTarget {
            translation_default: conn.translation_default(),
            kind: QueryTargetKind::Connection(conn),
        }
    }
}

#[cfg(feature = "sqlite")]
impl<'a> QueryTarget<'a> {
    pub(crate) fn from_typed_sqlite(
        conn: &'a mut PooledConnection<'static, SqliteManager>,
        in_tx: bool,
    ) -> Self {
        let kind = if in_tx {
            QueryTargetKind::TypedSqliteTx { conn }
        } else {
            QueryTargetKind::TypedSqlite { conn }
        };
        QueryTarget {
            translation_default: false,
            kind,
        }
    }
}

#[cfg(feature = "typed-postgres")]
impl<'a> QueryTarget<'a> {
    pub(crate) fn from_typed_postgres(
        conn: &'a mut PooledConnection<'static, PgManager>,
        in_tx: bool,
    ) -> Self {
        let kind = if in_tx {
            QueryTargetKind::TypedPostgresTx { conn }
        } else {
            QueryTargetKind::TypedPostgres { conn }
        };
        QueryTarget {
            translation_default: false,
            kind,
        }
    }
}

#[cfg(feature = "postgres")]
impl<'a> From<&'a postgres::transaction::Tx<'a>> for QueryTarget<'a> {
    fn from(tx: &'a postgres::transaction::Tx<'a>) -> Self {
        QueryTarget {
            translation_default: false,
            kind: QueryTargetKind::PostgresTx(tx),
        }
    }
}

#[cfg(feature = "mssql")]
impl<'a> From<&'a mut mssql::transaction::Tx<'a>> for QueryTarget<'a> {
    fn from(tx: &'a mut mssql::transaction::Tx<'a>) -> Self {
        QueryTarget {
            translation_default: false,
            kind: QueryTargetKind::MssqlTx(tx),
        }
    }
}

#[cfg(feature = "libsql")]
impl<'a> From<&'a libsql::transaction::Tx<'a>> for QueryTarget<'a> {
    fn from(tx: &'a libsql::transaction::Tx<'a>) -> Self {
        QueryTarget {
            translation_default: false,
            kind: QueryTargetKind::LibsqlTx(tx),
        }
    }
}

#[cfg(feature = "turso")]
impl<'a> From<&'a turso::transaction::Tx<'a>> for QueryTarget<'a> {
    fn from(tx: &'a turso::transaction::Tx<'a>) -> Self {
        QueryTarget {
            translation_default: false,
            kind: QueryTargetKind::TursoTx(tx),
        }
    }
}

#[cfg(feature = "typed-turso")]
impl<'a> QueryTarget<'a> {
    pub(crate) fn from_typed_turso(
        conn: &'a mut PooledConnection<'static, TursoManager>,
        in_tx: bool,
    ) -> Self {
        let kind = if in_tx {
            QueryTargetKind::TypedTursoTx { conn }
        } else {
            QueryTargetKind::TypedTurso { conn }
        };
        QueryTarget {
            translation_default: true,
            kind,
        }
    }
}

impl QueryTarget<'_> {
    #[must_use]
    pub(crate) fn translation_default(&self) -> bool {
        self.translation_default
    }

    #[must_use]
    pub(crate) fn translation_target(&self) -> Option<PlaceholderStyle> {
        match &self.kind {
            QueryTargetKind::Connection(conn) => translation_target(conn),
            #[cfg(feature = "postgres")]
            QueryTargetKind::PostgresTx(_) => Some(PlaceholderStyle::Postgres),
            #[cfg(feature = "sqlite")]
            QueryTargetKind::TypedSqlite { .. } => Some(PlaceholderStyle::Sqlite),
            #[cfg(feature = "sqlite")]
            QueryTargetKind::TypedSqliteTx { .. } => Some(PlaceholderStyle::Sqlite),
            #[cfg(feature = "typed-postgres")]
            QueryTargetKind::TypedPostgres { .. } => Some(PlaceholderStyle::Postgres),
            #[cfg(feature = "typed-postgres")]
            QueryTargetKind::TypedPostgresTx { .. } => Some(PlaceholderStyle::Postgres),
            #[cfg(feature = "libsql")]
            QueryTargetKind::LibsqlTx(_) => Some(PlaceholderStyle::Sqlite),
            #[cfg(feature = "turso")]
            QueryTargetKind::TursoTx(_) => Some(PlaceholderStyle::Sqlite),
            #[cfg(feature = "typed-turso")]
            QueryTargetKind::TypedTurso { .. } => Some(PlaceholderStyle::Sqlite),
            #[cfg(feature = "typed-turso")]
            QueryTargetKind::TypedTursoTx { .. } => Some(PlaceholderStyle::Sqlite),
            #[cfg(feature = "mssql")]
            QueryTargetKind::MssqlTx(_) => None,
            #[allow(unreachable_patterns)]
            _ => None,
        }
    }
}

/// Execute a batch against either a connection or a transaction.
///
/// # Errors
/// Returns an error propagated from the underlying backend execution or transaction context.
pub async fn execute_batch(
    target: impl Into<BatchTarget<'_>>,
    query: &str,
) -> Result<(), SqlMiddlewareDbError> {
    match target.into() {
        BatchTarget::Connection(conn) => conn.execute_batch(query).await,
        #[cfg(feature = "postgres")]
        BatchTarget::PostgresTx(tx) => tx.execute_batch(query).await,
        #[cfg(feature = "mssql")]
        BatchTarget::MssqlTx(tx) => tx.execute_batch(query).await,
        #[cfg(feature = "libsql")]
        BatchTarget::LibsqlTx(tx) => tx.execute_batch(query).await,
        #[cfg(feature = "turso")]
        BatchTarget::TursoTx(tx) => tx.execute_batch(query).await,
        #[cfg(feature = "typed-turso")]
        BatchTarget::TypedTurso { conn } => {
            crate::typed_turso::dml(conn, query, &[]).await?;
            Ok(())
        }
        #[cfg(feature = "typed-turso")]
        BatchTarget::TypedTursoTx { conn } => {
            crate::typed_turso::dml(conn, query, &[]).await?;
            Ok(())
        }
    }
}

/// Start a fluent builder for either a connection or a transaction.
pub fn query<'a>(target: impl Into<QueryTarget<'a>>, sql: &'a str) -> QueryBuilder<'a, 'a> {
    QueryBuilder::new_target(target.into(), sql)
}

impl MiddlewarePoolConnection {
    /// Executes a batch of SQL queries within a transaction by delegating to the specific database module.
    ///
    /// # Errors
    /// Returns an error if the selected backend cannot execute the batch or the database responds with an error.
    pub async fn execute_batch(&mut self, query: &str) -> Result<(), SqlMiddlewareDbError> {
        match self {
            #[cfg(feature = "postgres")]
            MiddlewarePoolConnection::Postgres {
                client: pg_client, ..
            } => postgres::execute_batch(pg_client, query).await,
            #[cfg(feature = "sqlite")]
            MiddlewarePoolConnection::Sqlite { .. } => {
                let sqlite_client = self.sqlite_conn_mut()?;
                sqlite::execute_batch(sqlite_client, query).await
            }
            #[cfg(feature = "mssql")]
            MiddlewarePoolConnection::Mssql {
                conn: mssql_client, ..
            } => mssql::execute_batch(mssql_client, query).await,
            #[cfg(feature = "libsql")]
            MiddlewarePoolConnection::Libsql {
                conn: libsql_client,
                ..
            } => libsql::execute_batch(libsql_client, query).await,
            #[cfg(feature = "turso")]
            MiddlewarePoolConnection::Turso {
                conn: turso_conn, ..
            } => turso::execute_batch(turso_conn, query).await,
            #[allow(unreachable_patterns)]
            _ => Err(SqlMiddlewareDbError::Unimplemented(
                "This database type is not enabled in the current build".to_string(),
            )),
        }
    }

    /// Start a fluent query builder that can translate placeholders before executing.
    ///
    /// # Examples
    /// ```rust,no_run
    /// use sql_middleware::prelude::*;
    ///
    /// # async fn demo() -> Result<(), SqlMiddlewareDbError> {
    /// let cap = ConfigAndPool::new_sqlite("file::memory:?cache=shared".into()).await?;
    /// let mut conn = cap.get_connection().await?;
    /// conn.execute_batch("CREATE TABLE t (id INTEGER)").await?;
    ///
    /// let rows = conn
    ///     .query("SELECT id FROM t WHERE id = ?1")
    ///     .params(&[RowValues::Int(1)])
    ///     .select()
    ///     .await?;
    /// assert!(rows.results.is_empty());
    /// # Ok(()) }
    /// ```
    pub fn query<'a>(&'a mut self, query: &'a str) -> QueryBuilder<'a, 'a> {
        QueryBuilder::new(self, query)
    }
}

pub(crate) async fn execute_select_dispatch(
    conn: &mut MiddlewarePoolConnection,
    query: &str,
    params: &[RowValues],
) -> Result<ResultSet, SqlMiddlewareDbError> {
    match conn {
        #[cfg(feature = "postgres")]
        MiddlewarePoolConnection::Postgres {
            client: pg_client, ..
        } => postgres::execute_select(pg_client, query, params).await,
        #[cfg(feature = "sqlite")]
        MiddlewarePoolConnection::Sqlite { .. } => {
            let sqlite_client = conn.sqlite_conn_mut()?;
            sqlite::execute_select(sqlite_client, query, params).await
        }
        #[cfg(feature = "mssql")]
        MiddlewarePoolConnection::Mssql {
            conn: mssql_client, ..
        } => mssql::execute_select(mssql_client, query, params).await,
        #[cfg(feature = "libsql")]
        MiddlewarePoolConnection::Libsql {
            conn: libsql_client,
            ..
        } => libsql::execute_select(libsql_client, query, params).await,
        #[cfg(feature = "turso")]
        MiddlewarePoolConnection::Turso {
            conn: turso_conn, ..
        } => turso::execute_select(turso_conn, query, params).await,
        #[allow(unreachable_patterns)]
        _ => Err(SqlMiddlewareDbError::Unimplemented(
            "This database type is not enabled in the current build".to_string(),
        )),
    }
}

pub(crate) async fn execute_dml_dispatch(
    conn: &mut MiddlewarePoolConnection,
    query: &str,
    params: &[RowValues],
) -> Result<usize, SqlMiddlewareDbError> {
    match conn {
        #[cfg(feature = "postgres")]
        MiddlewarePoolConnection::Postgres {
            client: pg_client, ..
        } => postgres::execute_dml(pg_client, query, params).await,
        #[cfg(feature = "sqlite")]
        MiddlewarePoolConnection::Sqlite { .. } => {
            let sqlite_client = conn.sqlite_conn_mut()?;
            sqlite::execute_dml(sqlite_client, query, params).await
        }
        #[cfg(feature = "mssql")]
        MiddlewarePoolConnection::Mssql {
            conn: mssql_client, ..
        } => mssql::execute_dml(mssql_client, query, params).await,
        #[cfg(feature = "libsql")]
        MiddlewarePoolConnection::Libsql {
            conn: libsql_client,
            ..
        } => libsql::execute_dml(libsql_client, query, params).await,
        #[cfg(feature = "turso")]
        MiddlewarePoolConnection::Turso {
            conn: turso_conn, ..
        } => turso::execute_dml(turso_conn, query, params).await,
        #[allow(unreachable_patterns)]
        _ => Err(SqlMiddlewareDbError::Unimplemented(
            "This database type is not enabled in the current build".to_string(),
        )),
    }
}

pub(crate) fn translation_target(conn: &MiddlewarePoolConnection) -> Option<PlaceholderStyle> {
    match conn {
        #[cfg(feature = "postgres")]
        MiddlewarePoolConnection::Postgres { .. } => Some(PlaceholderStyle::Postgres),
        #[cfg(feature = "sqlite")]
        MiddlewarePoolConnection::Sqlite { .. } => Some(PlaceholderStyle::Sqlite),
        #[cfg(feature = "libsql")]
        MiddlewarePoolConnection::Libsql { .. } => Some(PlaceholderStyle::Sqlite),
        #[cfg(feature = "turso")]
        MiddlewarePoolConnection::Turso { .. } => Some(PlaceholderStyle::Sqlite),
        #[cfg(feature = "mssql")]
        MiddlewarePoolConnection::Mssql { .. } => None,
        #[allow(unreachable_patterns)]
        _ => None,
    }
}
