use serde_json::Value;
use sqlx_middleware::SqlMiddlewareDbError;
// use sqlx::sqlite::SqlitePoolOptions;
use std::sync::Arc;
use std::{collections::HashMap, vec};
use tokio::sync::RwLock;

// use rusty_golf::controller::score;
// use rusty_golf::{controller::score::get_data_for_scores_page, model::CacheMap};

use sqlx_middleware::middleware::{ConfigAndPool as ConfigAndPool2, DatabaseExecutor, MiddlewarePool, MiddlewarePoolConnection, QueryAndParams};

#[tokio::test]
async fn test_new_batch_trait() -> Result<(), Box<dyn std::error::Error>> {
    let sqlite_uri = "file::memory:?cache=shared".to_string();
    
    let config_and_pool = ConfigAndPool2::new_sqlite(sqlite_uri.clone()).await.unwrap();

    // Define the DDL statements
    let ddl = vec![
        include_str!("../tests/sqlite/test4/00_event.sql"),
        // include_str!("../src/admin/model/sql/schema/sqlite/01_golfstatistic.sql"),
        include_str!("../tests/sqlite/test4/02_golfer.sql"),
        include_str!("../tests/sqlite/test4/03_bettor.sql"),
        include_str!("../tests/sqlite/test4/04_event_user_player.sql"),
        include_str!("../tests/sqlite/test4/05_eup_statistic.sql"),
    ];

    let ddl_query = ddl.join("\n");

    let query_and_params = QueryAndParams {
        query: ddl_query,
        params: vec![],
        is_read_only: false,
    };

    // Obtain a connection from the pool
    let pool = config_and_pool.pool.get().await?;
    let mut conn = MiddlewarePool::get_connection(pool).await?; // Make the connection mutable

    // Execute the DDL statements using the trait method
    conn.execute_batch(&query_and_params.query).await?;

    // Define the setup queries
    let setup_queries = include_str!("../tests/test4.sql");
    let setup_query_and_params = QueryAndParams {
        query: setup_queries.to_string(),
        params: vec![],
        is_read_only: false,
    };

    // Execute the setup queries using the trait method
    conn.execute_batch(&setup_query_and_params.query).await?;

    // ... rest of your test code ...

    Ok(())
}