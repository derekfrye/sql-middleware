use rand::Rng;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use serde_json::json;

pub fn get_benchmark_rows() -> usize {
    std::env::var("BENCH_ROWS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(10)
}

pub fn generate_insert_statements(num_rows: usize) -> String {
    let mut rng = ChaCha8Rng::seed_from_u64(42);
    let mut statements = String::with_capacity(num_rows * 200);

    for i in 0..num_rows {
        let a = rng.gen_range(1, 1000);
        let b = format!("text-{}", rng.gen_range(1, 1000));

        let timestamp_year = rng.gen_range(2020, 2026);
        let timestamp_month = rng.gen_range(1, 13);
        let timestamp_day = rng.gen_range(1, 29);
        let timestamp_hour = rng.gen_range(0, 24);
        let timestamp_minute = rng.gen_range(0, 60);
        let timestamp_second = rng.gen_range(0, 60);
        let timestamp = format!(
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
            timestamp_year,
            timestamp_month,
            timestamp_day,
            timestamp_hour,
            timestamp_minute,
            timestamp_second
        );

        let d = rng.gen_range(0.0, 1000.0);
        let e = rng.gen_bool(0.5);

        let blob_len = rng.gen_range(10, 21);
        let blob: Vec<u8> = (0..blob_len).map(|_| rng.gen_range(0, 256)).collect();
        let blob_hex = blob
            .iter()
            .map(|b| format!("{:02X}", b))
            .collect::<String>();

        let json_value = json!({
            "id": i,
            "value": rng.gen_range(1, 100),
            "tags": [
                format!("tag-{}", rng.gen_range(1, 10)),
                format!("tag-{}", rng.gen_range(1, 10))
            ]
        })
        .to_string();

        let additional_texts: Vec<String> =
            (0..9).map(|j| format!("text-field-{}-{}", i, j)).collect();

        let insert = format!(
            "INSERT INTO test (a, b, c, d, e, f, g, h, i, j, k, l, m, n, o, p) VALUES ({}, '{}', '{}', {}, {}, X'{}', '{}', '{}', '{}', '{}', '{}', '{}', '{}', '{}', '{}', '{}');\n",
            a,
            b,
            timestamp,
            d,
            if e { 1 } else { 0 },
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

pub fn generate_postgres_insert_statements(num_rows: usize) -> String {
    let mut rng = ChaCha8Rng::seed_from_u64(42);
    let mut statements = String::with_capacity(num_rows * 200);

    for i in 0..num_rows {
        let a = rng.gen_range(1, 1000);
        let b = format!("text-{}", rng.gen_range(1, 1000));

        let timestamp_year = rng.gen_range(2020, 2026);
        let timestamp_month = rng.gen_range(1, 13);
        let timestamp_day = rng.gen_range(1, 29);
        let timestamp_hour = rng.gen_range(0, 24);
        let timestamp_minute = rng.gen_range(0, 60);
        let timestamp_second = rng.gen_range(0, 60);
        let timestamp = format!(
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
            timestamp_year,
            timestamp_month,
            timestamp_day,
            timestamp_hour,
            timestamp_minute,
            timestamp_second
        );

        let d = rng.gen_range(0.0, 1000.0);
        let e = rng.gen_bool(0.5);

        let blob_len = rng.gen_range(10, 21);
        let blob: Vec<u8> = (0..blob_len).map(|_| rng.gen_range(0, 256)).collect();
        let blob_hex = blob
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect::<String>();

        let json_value = json!({
            "id": i,
            "value": rng.gen_range(1, 100),
            "tags": [
                format!("tag-{}", rng.gen_range(1, 10)),
                format!("tag-{}", rng.gen_range(1, 10))
            ]
        })
        .to_string();

        let additional_texts: Vec<String> =
            (0..9).map(|j| format!("text-field-{}-{}", i, j)).collect();

        let insert = format!(
            "INSERT INTO test (a, b, c, d, e, f, g, h, i, j, k, l, m, n, o, p) VALUES ({}, '{}', '{}', {}, {}, E'\\\\x{}', '{}', '{}', '{}', '{}', '{}', '{}', '{}', '{}', '{}', '{}');\n",
            a,
            b,
            timestamp,
            d,
            if e { "true" } else { "false" },
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
