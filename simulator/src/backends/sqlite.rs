use bb8::Pool;
use sql_middleware::middleware::{ConfigAndPool, DatabaseType, MiddlewarePool, MiddlewarePoolConnection};
use sql_middleware::sqlite::apply_wal_pragmas;
use sql_middleware::sqlite::config::SqliteManager;
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

    pub(crate) async fn begin(&self, conn: &mut MiddlewarePoolConnection) -> Result<(), BackendError> {
        conn.execute_batch("BEGIN").await?;
        Ok(())
    }

    pub(crate) async fn commit(
        &self,
        conn: &mut MiddlewarePoolConnection,
    ) -> Result<(), BackendError> {
        conn.execute_batch("COMMIT").await?;
        Ok(())
    }

    pub(crate) async fn rollback(
        &self,
        conn: &mut MiddlewarePoolConnection,
    ) -> Result<(), BackendError> {
        conn.execute_batch("ROLLBACK").await?;
        Ok(())
    }

    pub(crate) async fn execute(
        &self,
        conn: &mut MiddlewarePoolConnection,
        sql: &str,
    ) -> Result<(), BackendError> {
        conn.execute_batch(sql).await?;
        Ok(())
    }

    pub(crate) async fn query(
        &self,
        conn: &mut MiddlewarePoolConnection,
        sql: &str,
    ) -> Result<sql_middleware::ResultSet, BackendError> {
        Ok(conn.query(sql).select().await?)
    }

    pub(crate) async fn sleep(&self, ms: u64) {
        if ms == 0 {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
    }
}
