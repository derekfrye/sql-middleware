use sql_middleware::benchmark::libsql::get_libsql_instance;
use tokio::runtime::Runtime;

fn main() {
    let rt = Runtime::new().unwrap();
    
    println!("Starting LibSQL benchmark debug...");
    
    rt.block_on(async {
        println!("About to call get_libsql_instance...");
        let config_and_pool = get_libsql_instance().await;
        println!("Got LibSQL instance successfully!");
        
        println!("Config and pool: {:?}", config_and_pool);
    });
    
    println!("Debug completed!");
}