use std::{marker::PhantomData, sync::atomic::AtomicBool};

use bb8::{Pool, PooledConnection};
use bb8_tiberius::ConnectionManager;

use crate::middleware::SqlMiddlewareDbError;

use super::super::config::{MssqlOptions, build_tiberius_config};

/// Marker types for typestate.
pub enum Idle {}
pub enum InTx {}

/// bb8 manager for MSSQL clients.
pub type MssqlManager = ConnectionManager;

/// Typestate wrapper around a pooled MSSQL client.
pub struct MssqlTypedConnection<State> {
    pub(crate) conn: Option<PooledConnection<'static, MssqlManager>>,
    pub(crate) needs_rollback: bool,
    pub(crate) _state: PhantomData<State>,
}

impl MssqlTypedConnection<Idle> {
    /// Build a pool from MSSQL options.
    ///
    /// # Errors
    /// Returns `SqlMiddlewareDbError` if manager or pool creation fails.
    pub async fn build_pool(
        opts: MssqlOptions,
    ) -> Result<Pool<MssqlManager>, SqlMiddlewareDbError> {
        let manager = ConnectionManager::build(build_tiberius_config(&opts)).map_err(|e| {
            SqlMiddlewareDbError::ConnectionError(format!(
                "Failed to configure SQL Server manager: {e}"
            ))
        })?;

        Pool::builder().build(manager).await.map_err(|e| {
            SqlMiddlewareDbError::ConnectionError(format!("Failed to create SQL Server pool: {e}"))
        })
    }

    /// Checkout a connection from the pool.
    ///
    /// # Errors
    /// Returns `SqlMiddlewareDbError` if acquiring the connection fails.
    pub async fn from_pool(pool: &Pool<MssqlManager>) -> Result<Self, SqlMiddlewareDbError> {
        let conn = pool
            .get_owned()
            .await
            .map_err(SqlMiddlewareDbError::PoolErrorMssql)?;
        Ok(Self::new(conn, false))
    }
}

impl<State> MssqlTypedConnection<State> {
    pub(crate) fn new(conn: PooledConnection<'static, MssqlManager>, needs_rollback: bool) -> Self {
        Self {
            conn: Some(conn),
            needs_rollback,
            _state: PhantomData,
        }
    }

    pub(crate) fn conn_mut(&mut self) -> &mut PooledConnection<'static, MssqlManager> {
        self.conn.as_mut().expect("mssql connection already taken")
    }

    pub(crate) fn take_conn(
        &mut self,
    ) -> Result<PooledConnection<'static, MssqlManager>, SqlMiddlewareDbError> {
        self.conn.take().ok_or_else(|| {
            SqlMiddlewareDbError::ExecutionError("mssql connection already taken".into())
        })
    }
}

pub(crate) static SKIP_DROP_ROLLBACK: AtomicBool = AtomicBool::new(false);
