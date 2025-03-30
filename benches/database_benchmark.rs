use criterion::{ criterion_group, criterion_main, Criterion, BenchmarkId };
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use rand::Rng;
use serde_json::json;
use sql_middleware::{
    middleware::{ ConfigAndPool, MiddlewarePool, MiddlewarePoolConnection },
    SqlMiddlewareDbError,
};
use std::{ path::Path, fs };
use tokio::runtime::Runtime;

mod common {
    pub mod postgres;
}
use crate::common::postgres::{ setup_postgres_container, stop_postgres_container };

// Function to generate a deterministic set of SQL insert statements
fn generate_insert_statements(num_rows: usize) -> String {
    // Create a deterministic RNG with fixed seed for reproducibility
    let mut rng = ChaCha8Rng::seed_from_u64(42);

    let mut statements = String::with_capacity(num_rows * 200); // Rough estimate of string capacity

    for i in 0..num_rows {
        // Generate random data for each row
        let a = rng.random_range(1..1000);
        let b = format!("text-{}", rng.random_range(1..1000));

        // Generate a timestamp between 2020-01-01 and 2025-12-31
        let timestamp_year = rng.random_range(2020..=2025);
        let timestamp_month = rng.random_range(1..=12);
        let timestamp_day = rng.random_range(1..=28); // Avoiding edge cases with days per month
        let timestamp_hour = rng.random_range(0..=23);
        let timestamp_minute = rng.random_range(0..=59);
        let timestamp_second = rng.random_range(0..=59);
        let timestamp = format!(
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
            timestamp_year,
            timestamp_month,
            timestamp_day,
            timestamp_hour,
            timestamp_minute,
            timestamp_second
        );

        let d = rng.random_range(0.0..1000.0);
        let e = rng.random_bool(0.5);

        // Generate a random blob of 10-20 bytes
        let blob_len = rng.random_range(10..=20);
        let blob: Vec<u8> = (0..blob_len).map(|_| rng.random_range(0..=255)).collect();
        let blob_hex = blob
            .iter()
            .map(|b| format!("{:02X}", b))
            .collect::<String>();

        // Generate a random JSON object
        let json_value =
            json!({
            "id": i,
            "value": rng.random_range(1..100),
            "tags": [
                format!("tag-{}", rng.random_range(1..10)),
                format!("tag-{}", rng.random_range(1..10))
            ]
        }).to_string();

        // Generate additional text fields
        let additional_texts: Vec<String> = (0..9)
            .map(|j| format!("text-field-{}-{}", i, j))
            .collect();

        // Append the SQL statement
        let insert = format!(
            "INSERT INTO test (a, b, c, d, e, f, g, h, i, j, k, l, m, n, o, p) VALUES ({}, '{}', '{}', {}, {}, X'{}', '{}', '{}', '{}', '{}', '{}', '{}', '{}', '{}', '{}', '{}');\n",
            a,
            b,
            timestamp,
            d,
            if e {
                1
            } else {
                0
            },
            blob_hex,
            json_value,
            additional_texts[0],
            additional_texts[1],
            additional_texts[2],
            additional_texts[3],
            additional_texts[4],
            additional_texts[5],
            additional_texts[6],
            additional_texts[7],
            additional_texts[8]
        );

        statements.push_str(&insert);
    }

    statements
}

async fn setup_sqlite_db(db_path: &str) -> Result<ConfigAndPool, SqlMiddlewareDbError> {
    // Ensure the path doesn't exist
    if Path::new(db_path).exists() {
        fs::remove_file(db_path).unwrap();
    }

    let config_and_pool = ConfigAndPool::new_sqlite(db_path.to_string()).await?;

    // Create the test table
    let ddl =
        "CREATE TABLE IF NOT EXISTS test (
        recid INTEGER PRIMARY KEY AUTOINCREMENT,
        a int,
        b text,
        c datetime not null default current_timestamp,
        d real,
        e boolean,
        f blob,
        g json,
        h text, i text, j text, k text, l text, m text, n text, o text, p text
    )";

    let pool = config_and_pool.pool.get().await?;
    let sqlite_conn = MiddlewarePool::get_connection(&pool).await?;

    if let MiddlewarePoolConnection::Sqlite(sconn) = sqlite_conn {
        sconn.interact(move |conn| {
            let tx = conn.transaction()?;
            tx.execute_batch(ddl)?;
            tx.commit()?;
            Ok::<_, SqlMiddlewareDbError>(())
        }).await??;
    } else {
        panic!("Expected SQLite connection");
    }

    Ok(config_and_pool)
}

async fn setup_postgres_db(
    db_user: &str,
    db_pass: &str,
    db_name: &str
) -> Result<(ConfigAndPool, common::postgres::PostgresContainer), Box<dyn std::error::Error>> {
    let mut cfg = deadpool_postgres::Config::new();
    cfg.dbname = Some(db_name.to_string());
    cfg.host = Some("localhost".to_string());

    cfg.user = Some(db_user.to_string());
    cfg.password = Some(db_pass.to_string());
    let postgres_stuff = setup_postgres_container(&cfg)?;
    cfg.port = Some(postgres_stuff.port);

    let config_and_pool = ConfigAndPool::new_postgres(cfg).await?;

    // Create the test table
    let ddl =
        "CREATE TABLE IF NOT EXISTS test (
        recid SERIAL PRIMARY KEY,
        a int,
        b text,
        c timestamp not null default now(),
        d real,
        e boolean,
        f bytea,
        g jsonb,
        h text, i text, j text, k text, l text, m text, n text, o text, p text
    )";

    let pool = config_and_pool.pool.get().await?;
    let conn = MiddlewarePool::get_connection(&pool).await?;

    if let MiddlewarePoolConnection::Postgres(mut pgconn) = conn {
        let tx = pgconn.transaction().await?;
        tx.batch_execute(ddl).await?;
        tx.commit().await?;
    } else {
        panic!("Expected PostgreSQL connection");
    }

    Ok((config_and_pool, postgres_stuff))
}

fn benchmark_sqlite(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    // Generate insert statements once (same for all benchmark runs)
    let insert_statements = generate_insert_statements(500_000);

    let mut group = c.benchmark_group("database_inserts");

    group.bench_function(BenchmarkId::new("sqlite", "500k_rows"), |b| {
        b.to_async(&rt).iter(|| async {
            let db_path = "xxx".to_string();

            // Setup SQLite database
            let config_and_pool = setup_sqlite_db(&db_path).await.unwrap();

            // Get connection from pool
            let pool = config_and_pool.pool.get().await.unwrap();
            let sqlite_conn = MiddlewarePool::get_connection(&pool).await.unwrap();

            if let MiddlewarePoolConnection::Sqlite(sconn) = sqlite_conn {
                let insert_statements_copy = insert_statements.clone();

                // The actual benchmarked part
                sconn
                    .interact(move |conn| {
                        let tx = conn.transaction()?;
                        tx.execute_batch(&insert_statements_copy)?;
                        tx.commit()?;
                        Ok::<_, SqlMiddlewareDbError>(())
                    }).await
                    .unwrap()
                    .unwrap();
            }

            // Clean up
            if Path::new(&db_path).exists() {
                fs::remove_file(&db_path).unwrap();
            }
        });
    });

    group.finish();
}

fn benchmark_postgres(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    // Generate insert statements once (same for all benchmark runs)
    let insert_statements = generate_insert_statements(500_000);

    // Convert SQLite-style insert statements to PostgreSQL syntax
    let postgres_insert_statements = insert_statements
        .replace("INSERT INTO test (", "INSERT INTO test (")
        .replace("X'", "E'\\\\x");

    let mut group = c.benchmark_group("database_inserts");

    group.bench_function(BenchmarkId::new("postgres", "500k_rows"), |b| {
        b.to_async(&rt).iter(|| async {
            let db_user = "test_user";
            let db_pass = "test_password123";
            let db_name = "test_db";

            // Setup PostgreSQL database
            let (config_and_pool, postgres_stuff) = setup_postgres_db(
                db_user,
                db_pass,
                db_name
            ).await.unwrap();

            // Get connection from pool
            let pool = config_and_pool.pool.get().await.unwrap();
            let conn = MiddlewarePool::get_connection(&pool).await.unwrap();

            if let MiddlewarePoolConnection::Postgres(mut pgconn) = conn {
                // The actual benchmarked part
                let tx = pgconn.transaction().await.unwrap();
                tx.batch_execute(&postgres_insert_statements).await.unwrap();
                tx.commit().await.unwrap();
            }

            // Clean up by stopping the container
            stop_postgres_container(postgres_stuff);
        });
    });

    group.finish();
}

criterion_group!(benches, benchmark_sqlite, benchmark_postgres);
criterion_main!(benches);
