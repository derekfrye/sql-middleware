# Async Design Decisions

This document explains why certain functions in the codebase are marked with `#[allow(clippy::unused_async)]` and the reasoning behind our async API design.

## The Problem: Clippy's `unused_async` Warning

Clippy warns about functions marked as `async` that don't contain any `await` statements:

```rust
// Clippy warns about this:
pub async fn new_postgres(config: PgConfig) -> Result<ConfigAndPool, Error> {
    validate_config(&config)?;           // Sync operation
    let pool = create_pool(config)?;     // Sync operation  
    Ok(ConfigAndPool { pool, db_type: Postgres }) // Sync operation
}
```

## Why We Chose to Suppress the Warning

### 1. **No Practical Sync-Only Workflow Exists**

Every realistic user workflow here requires async operations immediately after pool creation:

```rust
#[tokio::main]  // User MUST have async runtime anyway
async fn main() -> Result<(), Error> {
    // Step 1: Create pool (could be sync, but...)
    let config = ConfigAndPool::new_postgres(pg_config).await?;
    
    // Step 2: User is ALREADY in async context for everything that follows:
    let conn = config.get_connection().await?; // MUST be async (I/O)
    let results = conn.execute_select("SELECT ...", &[]).await?; // MUST be async (I/O)
    
    // Only NOW can they do sync operations on results
    let name = results.get(0).unwrap().get("name").unwrap(); // Sync
    Ok(())
}
```
