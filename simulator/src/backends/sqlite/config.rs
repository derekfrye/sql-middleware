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
