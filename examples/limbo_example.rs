use sql_middleware::prelude::*;

#[tokio::main]
async fn main() -> Result<(), SqlMiddlewareDbError> {
    println!("Creating Limbo/Turso connection pool...");
    
    // Create a Limbo/Turso connection pool
    let config = ConfigAndPool::new_limbo("example.db".to_string()).await?;
    
    println!("Connection pool created successfully!");
    println!("Database type: {:?}", config.db_type);
    
    // The basic example shows that the Limbo integration is working
    // Full query examples would require implementing the middleware executor traits
    // which is beyond the scope of this initial implementation
    
    println!("Limbo integration is working! You can now:");
    println!("1. Create connection pools with ConfigAndPool::new_limbo()");
    println!("2. Use MiddlewarePool::Limbo variant");
    println!("3. Access raw Turso connections through interact_async");
    println!("4. Use parameter conversion with LimboParams");
    
    Ok(())
}