use bb8::Pool;
use sql_middleware::middleware::{
    ConfigAndPool, DatabaseType, MiddlewarePool, MiddlewarePoolConnection,
};
use std::time::Duration;

use sql_middleware::sqlite::{SqliteConnection, apply_wal_pragmas};
use sql_middleware::sqlite::config::SqliteManager;
use sql_middleware::sqlite::params::Params;
use sql_middleware::sqlite::query::build_result_set;
use sql_middleware::RowValues;
use sql_middleware::SqlMiddlewareDbError;

#[derive(Debug)]
pub(crate) enum BackendError {
    Init(String),
    Sql(SqlMiddlewareDbError),
}

impl std::fmt::Display for BackendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BackendError::Init(message) => write!(f, "{message}"),
            BackendError::Sql(err) => write!(f, "{err}"),
        }
    }
}

impl From<SqlMiddlewareDbError> for BackendError {
    fn from(err: SqlMiddlewareDbError) -> Self {
        BackendError::Sql(err)
    }
}

#[derive(Debug, Clone)]
pub(crate) struct SqliteBackendConfig {
    pub(crate) db_path: String,
    pub(crate) pool_size: usize,
}

impl SqliteBackendConfig {
    pub(crate) fn in_memory(pool_size: usize) -> Self {
        Self {
            db_path: "file::memory:?cache=shared".to_string(),
            pool_size,
        }
    }
}

pub(crate) struct SqliteBackend {
    pool: ConfigAndPool,
}

impl SqliteBackend {
    const BUSY_RETRIES: usize = 8;

    pub(crate) async fn new(config: SqliteBackendConfig) -> Result<Self, BackendError> {
        let pool_size = config.pool_size.max(1) as u32;
        let manager = SqliteManager::new(config.db_path);
        let pool = Pool::builder()
            .max_size(pool_size)
            .build(manager)
            .await
            .map_err(|err| BackendError::Init(format!("sqlite pool error: {err}")))?;

        {
            let mut conn = pool.get_owned().await.map_err(|err| {
                BackendError::Init(format!("sqlite pool checkout error: {err}"))
            })?;
            apply_wal_pragmas(&mut conn).await?;
        }

        Ok(Self {
            pool: ConfigAndPool {
                pool: MiddlewarePool::Sqlite(pool),
                db_type: DatabaseType::Sqlite,
                translate_placeholders: false,
            },
        })
    }

    pub(crate) async fn checkout(&self) -> Result<MiddlewarePoolConnection, BackendError> {
        Ok(self.pool.get_connection().await?)
    }

    fn sqlite_conn_mut(
        conn: &mut MiddlewarePoolConnection,
    ) -> Result<&mut SqliteConnection, BackendError> {
        #[allow(unreachable_patterns)]
        match conn {
            MiddlewarePoolConnection::Sqlite { conn, .. } => conn.as_mut().ok_or_else(|| {
                BackendError::Init("SQLite connection already taken from pool wrapper".to_string())
            }),
            _ => Err(BackendError::Init(
                "SQLite backend called with non-sqlite connection".to_string(),
            )),
        }
    }

    fn is_busy_error(err: &BackendError) -> bool {
        match err {
            BackendError::Sql(SqlMiddlewareDbError::SqliteError(
                rusqlite::Error::SqliteFailure(code, _),
            )) => matches!(
                code.code,
                rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked
            ),
            _ => false,
        }
    }

    pub(crate) async fn begin(
        &self,
        conn: &mut MiddlewarePoolConnection,
    ) -> Result<(), BackendError> {
        let mut delay_ms = 5u64;
        for attempt in 0..=Self::BUSY_RETRIES {
            let result = {
                let sqlite_conn = Self::sqlite_conn_mut(conn)?;
                sqlite_conn.begin().await.map_err(BackendError::from)
            };
            match result {
                Ok(()) => return Ok(()),
                Err(err) if Self::is_busy_error(&err) && attempt < Self::BUSY_RETRIES => {
                    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                    delay_ms = (delay_ms * 2).min(100);
                    continue;
                }
                Err(err) => return Err(err),
            }
        }
        unreachable!("retry loop should return on last attempt");
    }

    pub(crate) async fn commit(
        &self,
        conn: &mut MiddlewarePoolConnection,
    ) -> Result<(), BackendError> {
        let mut delay_ms = 5u64;
        for attempt in 0..=Self::BUSY_RETRIES {
            let result = {
                let sqlite_conn = Self::sqlite_conn_mut(conn)?;
                sqlite_conn.commit().await.map_err(BackendError::from)
            };
            match result {
                Ok(()) => return Ok(()),
                Err(err) if Self::is_busy_error(&err) && attempt < Self::BUSY_RETRIES => {
                    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                    delay_ms = (delay_ms * 2).min(100);
                    continue;
                }
                Err(err) => return Err(err),
            }
        }
        unreachable!("retry loop should return on last attempt");
    }

    pub(crate) async fn rollback(
        &self,
        conn: &mut MiddlewarePoolConnection,
    ) -> Result<(), BackendError> {
        let mut delay_ms = 5u64;
        for attempt in 0..=Self::BUSY_RETRIES {
            let result = {
                let sqlite_conn = Self::sqlite_conn_mut(conn)?;
                sqlite_conn.rollback().await.map_err(BackendError::from)
            };
            match result {
                Ok(()) => return Ok(()),
                Err(err) if Self::is_busy_error(&err) && attempt < Self::BUSY_RETRIES => {
                    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                    delay_ms = (delay_ms * 2).min(100);
                    continue;
                }
                Err(err) => return Err(err),
            }
        }
        unreachable!("retry loop should return on last attempt");
    }

    pub(crate) async fn execute(
        &self,
        conn: &mut MiddlewarePoolConnection,
        sql: &str,
        in_tx: bool,
    ) -> Result<(), BackendError> {
        let mut delay_ms = 5u64;
        for attempt in 0..=Self::BUSY_RETRIES {
            let result = if in_tx {
                let sqlite_conn = Self::sqlite_conn_mut(conn)?;
                sqlite_conn
                    .execute_batch_in_tx(sql)
                    .await
                    .map_err(BackendError::from)
            } else {
                conn.execute_batch(sql)
                    .await
                    .map_err(BackendError::from)
            };
            match result {
                Ok(()) => return Ok(()),
                Err(err) if Self::is_busy_error(&err) && attempt < Self::BUSY_RETRIES => {
                    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                    delay_ms = (delay_ms * 2).min(100);
                    continue;
                }
                Err(err) => return Err(err),
            }
        }
        unreachable!("retry loop should return on last attempt");
    }

    pub(crate) async fn query(
        &self,
        conn: &mut MiddlewarePoolConnection,
        sql: &str,
        in_tx: bool,
    ) -> Result<sql_middleware::ResultSet, BackendError> {
        let mut delay_ms = 5u64;
        for attempt in 0..=Self::BUSY_RETRIES {
            let result = if in_tx {
                let sqlite_conn = Self::sqlite_conn_mut(conn)?;
                let params: &[RowValues] = &[];
                let params = Params::convert(params).map_err(BackendError::from)?;
                sqlite_conn
                    .execute_select_in_tx(sql, params.as_values(), build_result_set)
                    .await
                    .map_err(BackendError::from)
            } else {
                conn.query(sql)
                    .select()
                    .await
                    .map_err(BackendError::from)
            };
            match result {
                Ok(result) => return Ok(result),
                Err(err) if Self::is_busy_error(&err) && attempt < Self::BUSY_RETRIES => {
                    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                    delay_ms = (delay_ms * 2).min(100);
                    continue;
                }
                Err(err) => return Err(err),
            }
        }
        unreachable!("retry loop should return on last attempt");
    }

    pub(crate) async fn sleep(&self, ms: u64) {
        if ms == 0 {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
    }
}
