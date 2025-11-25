#[cfg(feature = "libsql")]
mod libsql_tests {
    use serde_json::json;
    use sql_middleware::middleware::{
        ConfigAndPool, DatabaseType, MiddlewarePool, MiddlewarePoolConnection, RowValues,
    };
    use tokio::runtime::Runtime;

    #[allow(clippy::float_cmp)]
    #[test]
    fn test_libsql_basic_operations() -> Result<(), Box<dyn std::error::Error>> {
        let rt = Runtime::new()?;
        rt.block_on(async {
            // Create in-memory libsql database
            let config_and_pool = ConfigAndPool::new_libsql(":memory:".to_string()).await?;
            assert_eq!(config_and_pool.db_type, DatabaseType::Libsql);

            let mut libsql_conn = config_and_pool.get_connection().await?;

            // Verify we got the right connection type
            assert!(
                matches!(&libsql_conn, MiddlewarePoolConnection::Libsql { .. }),
                "Expected LibSQL connection"
            );

            // Test batch execution - create table
            let create_table_sql = "
                CREATE TABLE IF NOT EXISTS test_table (
                    id INTEGER PRIMARY KEY,
                    name TEXT NOT NULL,
                    value REAL,
                    is_active BOOLEAN,
                    created_at TEXT,
                    data BLOB,
                    metadata TEXT
                );
            ";

            libsql_conn.execute_batch(create_table_sql).await?;

            // Test DML operations - insert data
            let insert_sql = "INSERT INTO test_table (id, name, value, is_active, created_at, data, metadata) VALUES (?, ?, ?, ?, ?, ?, ?)";
            let params = vec![
                RowValues::Int(1),
                RowValues::Text("Alice".to_string()),
                RowValues::Float(42.5),
                RowValues::Bool(true),
                RowValues::Text("2024-01-01 10:00:00".to_string()),
                RowValues::Blob(b"test_blob".to_vec()),
                RowValues::JSON(json!({"type": "user", "level": 1})),
            ];

            let rows_affected = libsql_conn.query(insert_sql).params(&params).dml().await?;
            assert_eq!(rows_affected, 1);

            // Insert another row
            let params2 = vec![
                RowValues::Int(2),
                RowValues::Text("Bob".to_string()),
                RowValues::Float(24.7),
                RowValues::Bool(false),
                RowValues::Text("2024-01-02 11:30:00".to_string()),
                RowValues::Blob(b"another_blob".to_vec()),
                RowValues::JSON(json!({"type": "admin", "level": 5})),
            ];

            let rows_affected2 = libsql_conn.query(insert_sql).params(&params2).dml().await?;
            assert_eq!(rows_affected2, 1);

            // Test SELECT operations
            let select_sql = "SELECT * FROM test_table WHERE id = ?";
            let select_params = vec![RowValues::Int(1)];

            let result_set = libsql_conn
                .query(select_sql)
                .params(&select_params)
                .select()
                .await?;

            // Verify results
            assert_eq!(result_set.results.len(), 1);
            let row = &result_set.results[0];

            assert_eq!(*row.get("id").unwrap().as_int().unwrap(), 1);
            assert_eq!(row.get("name").unwrap().as_text().unwrap(), "Alice");
            assert_eq!(row.get("value").unwrap().as_float().unwrap(), 42.5);
            assert!(*row.get("is_active").unwrap().as_bool().unwrap());
            assert_eq!(row.get("data").unwrap().as_blob().unwrap(), b"test_blob");

            // Test SELECT with multiple results
            let select_all_sql = "SELECT id, name, value FROM test_table ORDER BY id";
            let all_results = libsql_conn.query(select_all_sql).select().await?;

            assert_eq!(all_results.results.len(), 2);
            assert_eq!(*all_results.results[0].get("id").unwrap().as_int().unwrap(), 1);
            assert_eq!(all_results.results[0].get("name").unwrap().as_text().unwrap(), "Alice");
            assert_eq!(*all_results.results[1].get("id").unwrap().as_int().unwrap(), 2);
            assert_eq!(all_results.results[1].get("name").unwrap().as_text().unwrap(), "Bob");

            // Test UPDATE operation
            let update_sql = "UPDATE test_table SET value = ? WHERE id = ?";
            let update_params = vec![RowValues::Float(99.9), RowValues::Int(1)];
            let updated_rows = libsql_conn
                .query(update_sql)
                .params(&update_params)
                .dml()
                .await?;
            assert_eq!(updated_rows, 1);

            // Verify update worked
            let verify_result = libsql_conn
                .query(select_sql)
                .params(&select_params)
                .select()
                .await?;
            assert_eq!(verify_result.results[0].get("value").unwrap().as_float().unwrap(), 99.9);

            // Test DELETE operation
            let delete_sql = "DELETE FROM test_table WHERE id = ?";
            let delete_params = vec![RowValues::Int(2)];
            let deleted_rows = libsql_conn
                .query(delete_sql)
                .params(&delete_params)
                .dml()
                .await?;
            assert_eq!(deleted_rows, 1);

            // Verify only one row remains
            let final_results = libsql_conn.query(select_all_sql).select().await?;
            assert_eq!(final_results.results.len(), 1);
            assert_eq!(*final_results.results[0].get("id").unwrap().as_int().unwrap(), 1);

            Ok::<(), Box<dyn std::error::Error>>(())
        })
    }

    #[test]
    fn test_libsql_null_values() -> Result<(), Box<dyn std::error::Error>> {
        let rt = Runtime::new()?;
        rt.block_on(async {
            let config_and_pool = ConfigAndPool::new_libsql(":memory:".to_string()).await?;
            let mut libsql_conn = config_and_pool.get_connection().await?;

            // Create table
            libsql_conn
                .execute_batch("CREATE TABLE null_test (id INTEGER, optional_text TEXT)")
                .await?;

            // Insert row with NULL
            let insert_params = vec![RowValues::Int(1), RowValues::Null];
            libsql_conn
                .query("INSERT INTO null_test VALUES (?, ?)")
                .params(&insert_params)
                .dml()
                .await?;

            // Query and verify NULL handling
            let results = libsql_conn
                .query("SELECT * FROM null_test")
                .select()
                .await?;
            assert_eq!(results.results.len(), 1);
            assert_eq!(*results.results[0].get("id").unwrap().as_int().unwrap(), 1);
            assert!(results.results[0].get("optional_text").unwrap().is_null());

            Ok::<(), Box<dyn std::error::Error>>(())
        })
    }
}
