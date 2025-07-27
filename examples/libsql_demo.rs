//! LibSQL demonstration example
//! 
//! This example shows how to use the libsql integration with the sql-middleware crate.
//! Run with: cargo run --example libsql_demo --features libsql

#[cfg(feature = "libsql")]
use sql_middleware::{
    middleware::{
        AsyncDatabaseExecutor, ConfigAndPool, DatabaseType, MiddlewarePool,
        MiddlewarePoolConnection, RowValues,
    },
};

#[cfg(feature = "libsql")]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 LibSQL Integration Demo");
    println!("==========================");

    // Create in-memory libsql database
    println!("📊 Creating LibSQL database connection...");
    let config_and_pool = ConfigAndPool::new_libsql(":memory:".to_string()).await?;
    assert_eq!(config_and_pool.db_type, DatabaseType::Libsql);
    println!("✅ Successfully created LibSQL connection pool");

    let pool = config_and_pool.pool.get().await?;
    let mut libsql_conn = MiddlewarePool::get_connection(pool).await?;

    // Verify we got the right connection type
    match &libsql_conn {
        MiddlewarePoolConnection::Libsql(_) => {
            println!("✅ Got LibSQL connection");
        }
        _ => panic!("Expected LibSQL connection"),
    }

    // Create tables
    println!("\n📋 Creating tables...");
    let create_table_sql = "
        CREATE TABLE users (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            email TEXT UNIQUE,
            age INTEGER,
            balance REAL,
            is_active BOOLEAN DEFAULT true,
            created_at TEXT,
            profile_data TEXT
        );
        
        CREATE TABLE posts (
            id INTEGER PRIMARY KEY,
            user_id INTEGER,
            title TEXT NOT NULL,
            content TEXT,
            FOREIGN KEY (user_id) REFERENCES users (id)
        );
    ";

    libsql_conn.execute_batch(create_table_sql).await?;
    println!("✅ Tables created successfully");

    // Insert sample data
    println!("\n📝 Inserting sample data...");
    let users = vec![
        (1, "Alice Johnson", "alice@example.com", 28, 1250.50, true, "2024-01-15 10:30:00", r#"{"role": "admin", "preferences": {"theme": "dark"}}"#),
        (2, "Bob Smith", "bob@example.com", 34, 875.25, true, "2024-01-16 14:20:00", r#"{"role": "user", "preferences": {"theme": "light"}}"#),
        (3, "Carol Davis", "carol@example.com", 42, 2100.75, false, "2024-01-17 09:15:00", r#"{"role": "moderator", "preferences": {"theme": "auto"}}"#),
    ];

    let insert_user_sql = "INSERT INTO users (id, name, email, age, balance, is_active, created_at, profile_data) VALUES (?, ?, ?, ?, ?, ?, ?, ?)";
    
    for (id, name, email, age, balance, is_active, created_at, profile_data) in users {
        let params = vec![
            RowValues::Int(id),
            RowValues::Text(name.to_string()),
            RowValues::Text(email.to_string()),
            RowValues::Int(age),
            RowValues::Float(balance),
            RowValues::Bool(is_active),
            RowValues::Text(created_at.to_string()),
            RowValues::Text(profile_data.to_string()),
        ];
        
        let rows_affected = libsql_conn.execute_dml(insert_user_sql, &params).await?;
        println!("  ✅ Inserted user '{}' (rows affected: {})", name, rows_affected);
    }

    // Insert posts
    let posts = vec![
        (1, 1, "Welcome to LibSQL!", "This is my first post using LibSQL with Rust."),
        (2, 1, "Performance Comparison", "LibSQL shows great performance improvements over traditional SQLite."),
        (3, 2, "Getting Started Guide", "Here's how to get started with the sql-middleware crate."),
    ];

    let insert_post_sql = "INSERT INTO posts (id, user_id, title, content) VALUES (?, ?, ?, ?)";
    
    for (id, user_id, title, content) in posts {
        let params = vec![
            RowValues::Int(id),
            RowValues::Int(user_id),
            RowValues::Text(title.to_string()),
            RowValues::Text(content.to_string()),
        ];
        
        libsql_conn.execute_dml(insert_post_sql, &params).await?;
        println!("  ✅ Inserted post '{}'", title);
    }

    // Query data with JOIN
    println!("\n🔍 Querying data with JOIN...");
    let join_query = "
        SELECT 
            u.name as user_name,
            u.email,
            u.balance,
            p.title as post_title,
            p.content
        FROM users u
        INNER JOIN posts p ON u.id = p.user_id
        WHERE u.is_active = ?
        ORDER BY u.name, p.id
    ";
    
    let result_set = libsql_conn.execute_select(join_query, &[RowValues::Bool(true)]).await?;
    
    println!("📊 Query Results ({} rows):", result_set.results.len());
    println!("┌─────────────────┬─────────────────────┬─────────┬─────────────────────┐");
    println!("│ User            │ Email               │ Balance │ Post Title          │");
    println!("├─────────────────┼─────────────────────┼─────────┼─────────────────────┤");
    
    for row in &result_set.results {
        let user_name = row.get("user_name").unwrap().as_text().unwrap();
        let email = row.get("email").unwrap().as_text().unwrap();
        let balance = row.get("balance").unwrap().as_float().unwrap();
        let post_title = row.get("post_title").unwrap().as_text().unwrap();
        
        println!("│ {:<15} │ {:<19} │ ${:>6.2} │ {:<19} │", 
                 user_name, email, balance, post_title);
    }
    println!("└─────────────────┴─────────────────────┴─────────┴─────────────────────┘");

    // Aggregate query
    println!("\n📈 Running aggregate query...");
    let stats_query = "
        SELECT 
            COUNT(*) as user_count,
            AVG(age) as avg_age,
            SUM(balance) as total_balance,
            MAX(balance) as max_balance
        FROM users 
        WHERE is_active = ?
    ";
    
    let stats_result = libsql_conn.execute_select(stats_query, &[RowValues::Bool(true)]).await?;
    let stats_row = &stats_result.results[0];
    
    println!("📊 Statistics for active users:");
    println!("  • Total users: {}", stats_row.get("user_count").unwrap().as_int().unwrap());
    println!("  • Average age: {:.1}", stats_row.get("avg_age").unwrap().as_float().unwrap());
    println!("  • Total balance: ${:.2}", stats_row.get("total_balance").unwrap().as_float().unwrap());
    println!("  • Highest balance: ${:.2}", stats_row.get("max_balance").unwrap().as_float().unwrap());

    // Update operation
    println!("\n✏️  Performing update operation...");
    let update_sql = "UPDATE users SET balance = balance * 1.05 WHERE is_active = ?";
    let updated_rows = libsql_conn.execute_dml(update_sql, &[RowValues::Bool(true)]).await?;
    println!("✅ Applied 5% interest to {} active users", updated_rows);

    // Verify update
    let verification_query = "SELECT name, balance FROM users WHERE is_active = ? ORDER BY name";
    let updated_result = libsql_conn.execute_select(verification_query, &[RowValues::Bool(true)]).await?;
    
    println!("💰 Updated balances:");
    for row in &updated_result.results {
        let name = row.get("name").unwrap().as_text().unwrap();
        let balance = row.get("balance").unwrap().as_float().unwrap();
        println!("  • {}: ${:.2}", name, balance);
    }

    println!("\n🎉 Demo completed successfully!");
    println!("✨ LibSQL integration is working perfectly with sql-middleware!");
    
    Ok(())
}

#[cfg(not(feature = "libsql"))]
fn main() {
    println!("This example requires the 'libsql' feature to be enabled.");
    println!("Run with: cargo run --example libsql_demo --features libsql");
}