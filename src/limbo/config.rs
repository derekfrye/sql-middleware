use deadpool::managed::{Manager, Pool, RecycleResult};
use std::fmt;

use crate::middleware::{ConfigAndPool, DatabaseType, MiddlewarePool, SqlMiddlewareDbError};

/// Turso/Limbo connection manager for deadpool
pub struct TursoManager {
    db_path: String,
}

impl TursoManager {
    pub fn new(db_path: String) -> Self {
        Self { db_path }
    }
}

impl fmt::Debug for TursoManager {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TursoManager")
            .field("db_path", &self.db_path)
            .finish()
    }
}

impl Manager for TursoManager {
    type Type = turso::Connection;
    type Error = SqlMiddlewareDbError;

    async fn create(&self) -> Result<Self::Type, Self::Error> {
        let db = turso::Builder::new_local(&self.db_path)
            .build()
            .await
            .map_err(|e| SqlMiddlewareDbError::ConnectionError(format!("Failed to create Turso database: {e}")))?;
        
        db.connect()
            .map_err(|e| SqlMiddlewareDbError::ConnectionError(format!("Failed to connect to Turso database: {e}")))
    }

    async fn recycle(&self, _conn: &mut Self::Type, _metrics: &deadpool::managed::Metrics) -> RecycleResult<Self::Error> {
        // For now, assume connections are always valid
        // TODO: Add proper connection health check if Turso provides one
        Ok(())
    }
}

pub type TursoPool = Pool<TursoManager>;

impl ConfigAndPool {
    /// Asynchronous initializer for ConfigAndPool with Turso/Limbo using deadpool
    pub async fn new_limbo(db_path: String) -> Result<Self, SqlMiddlewareDbError> {
        let manager = TursoManager::new(db_path);
        
        let pool = Pool::builder(manager)
            .max_size(16) // Default pool size
            .build()
            .map_err(|e| SqlMiddlewareDbError::ConnectionError(format!("Failed to create Turso pool: {e}")))?;

        Ok(ConfigAndPool {
            pool: MiddlewarePool::Limbo(pool),
            db_type: DatabaseType::Limbo,
        })
    }
}