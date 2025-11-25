#[cfg(feature = "libsql")]
mod libsql_tests {
    use sql_middleware::middleware::{
        ConfigAndPool, DatabaseType, MiddlewarePoolConnection, RowValues,
    };
    use tokio::runtime::Runtime;

    #[test]
    fn test_libsql_simple() -> Result<(), Box<dyn std::error::Error>> {
        let rt = Runtime::new()?;
        rt.block_on(async {
            // Create in-memory libsql database
            let config_and_pool = ConfigAndPool::new_libsql(":memory:".to_string()).await?;
            assert_eq!(config_and_pool.db_type, DatabaseType::Libsql);
            println!("✓ Created LibSQL connection pool");

            let mut libsql_conn = config_and_pool.get_connection().await?;

            // Verify we got the right connection type
            match &libsql_conn {
                MiddlewarePoolConnection::Libsql { .. } => {
                    println!("✓ Got LibSQL connection");
                }
                _ => panic!("Expected LibSQL connection"),
            }

            // Test batch execution - create table
            let create_table_sql = "
                CREATE TABLE test_table (
                    id INTEGER PRIMARY KEY,
                    name TEXT NOT NULL
                );
            ";

            libsql_conn.execute_batch(create_table_sql).await?;
            println!("✓ Created table via batch execution");

            // Test DML operations - insert data
            let insert_sql = "INSERT INTO test_table (id, name) VALUES (?, ?)";
            let params = vec![RowValues::Int(1), RowValues::Text("Alice".to_string())];

            let rows_affected = libsql_conn.query(insert_sql).params(&params).dml().await?;
            assert_eq!(rows_affected, 1);
            println!("✓ Inserted row via DML execution");

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
            println!("✓ Successfully queried and verified data");

            println!("🎉 All LibSQL tests passed!");
            Ok::<(), Box<dyn std::error::Error>>(())
        })
    }
}
