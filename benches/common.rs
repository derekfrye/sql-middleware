use rand::Rng;
use rand::SeedableRng;
use rand::distributions::{Distribution, Standard};
use rand_chacha::ChaCha8Rng;
use serde_json::json;
use std::fmt::Write;

#[must_use]
pub fn get_benchmark_rows() -> usize {
    std::env::var("BENCH_ROWS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(10)
}

#[must_use]
pub fn generate_insert_statements(num_rows: usize) -> String {
    let mut rng = ChaCha8Rng::seed_from_u64(42);
    let mut statements = String::with_capacity(num_rows * 200);

    for i in 0..num_rows {
        let a = rng.gen_range(1..1000);
        let b_suffix = rng.gen_range(1..1000);
        let b = format!("text-{b_suffix}");

        let timestamp_year = rng.gen_range(2020..2026);
        let timestamp_month = rng.gen_range(1..13);
        let timestamp_day = rng.gen_range(1..29);
        let timestamp_hour = rng.gen_range(0..24);
        let timestamp_minute = rng.gen_range(0..60);
        let timestamp_second = rng.gen_range(0..60);
        let timestamp = format!(
            "{timestamp_year:04}-{timestamp_month:02}-{timestamp_day:02} {timestamp_hour:02}:{timestamp_minute:02}:{timestamp_second:02}"
        );

        let d = rng.gen_range(0.0..1000.0);
        let e = rng.gen_bool(0.5);

        let blob_len = rng.gen_range(10..21);
        let blob: Vec<u8> = (0..blob_len).map(|_| Standard.sample(&mut rng)).collect();
        let mut blob_hex = String::with_capacity(blob_len * 2);
        for byte in &blob {
            write!(blob_hex, "{byte:02X}").expect("writing to string");
        }

        let validation_tag = rng.gen_range(1..10);
        let audit_tag = rng.gen_range(1..10);
        let json_value = json!({
            "id": i,
            "value": rng.gen_range(1..100),
            "tags": [
                format!("tag-{validation_tag}"),
                format!("tag-{audit_tag}")
            ]
        })
        .to_string();

        let additional_texts: Vec<String> = (0..9).map(|j| format!("text-field-{i}-{j}")).collect();

        let insert = format!(
            "INSERT INTO test (a, b, c, d, e, f, g, h, i, j, k, l, m, n, o, p) VALUES ({a}, '{b}', '{timestamp}', {d}, {bool_flag}, X'{blob_hex}', '{json_value}', '{t0}', '{t1}', '{t2}', '{t3}', '{t4}', '{t5}', '{t6}', '{t7}', '{t8}');\n",
            bool_flag = i32::from(e),
            t0 = additional_texts[0],
            t1 = additional_texts[1],
            t2 = additional_texts[2],
            t3 = additional_texts[3],
            t4 = additional_texts[4],
            t5 = additional_texts[5],
            t6 = additional_texts[6],
            t7 = additional_texts[7],
            t8 = additional_texts[8]
        );

        statements.push_str(&insert);
    }

    statements
}

#[must_use]
pub fn generate_postgres_insert_statements(num_rows: usize) -> String {
    let mut rng = ChaCha8Rng::seed_from_u64(42);
    let mut statements = String::with_capacity(num_rows * 200);

    for i in 0..num_rows {
        let a = rng.gen_range(1..1000);
        let b_suffix = rng.gen_range(1..1000);
        let b = format!("text-{b_suffix}");

        let timestamp_year = rng.gen_range(2020..2026);
        let timestamp_month = rng.gen_range(1..13);
        let timestamp_day = rng.gen_range(1..29);
        let timestamp_hour = rng.gen_range(0..24);
        let timestamp_minute = rng.gen_range(0..60);
        let timestamp_second = rng.gen_range(0..60);
        let timestamp = format!(
            "{timestamp_year:04}-{timestamp_month:02}-{timestamp_day:02} {timestamp_hour:02}:{timestamp_minute:02}:{timestamp_second:02}"
        );

        let d = rng.gen_range(0.0..1000.0);
        let e = rng.gen_bool(0.5);

        let blob_len = rng.gen_range(10..21);
        let blob: Vec<u8> = (0..blob_len).map(|_| Standard.sample(&mut rng)).collect();
        let mut blob_hex = String::with_capacity(blob_len * 2);
        for byte in &blob {
            write!(blob_hex, "{byte:02x}").expect("writing to string");
        }

        let validation_tag = rng.gen_range(1..10);
        let audit_tag = rng.gen_range(1..10);
        let json_value = json!({
            "id": i,
            "value": rng.gen_range(1..100),
            "tags": [
                format!("tag-{validation_tag}"),
                format!("tag-{audit_tag}")
            ]
        })
        .to_string();

        let additional_texts: Vec<String> = (0..9).map(|j| format!("text-field-{i}-{j}")).collect();

        let insert = format!(
            "INSERT INTO test (a, b, c, d, e, f, g, h, i, j, k, l, m, n, o, p) VALUES ({a}, '{b}', '{timestamp}', {d}, {bool_flag}, E'\\\\x{blob_hex}', '{json_value}', '{t0}', '{t1}', '{t2}', '{t3}', '{t4}', '{t5}', '{t6}', '{t7}', '{t8}');\n",
            bool_flag = if e { "true" } else { "false" },
            t0 = additional_texts[0],
            t1 = additional_texts[1],
            t2 = additional_texts[2],
            t3 = additional_texts[3],
            t4 = additional_texts[4],
            t5 = additional_texts[5],
            t6 = additional_texts[6],
            t7 = additional_texts[7],
            t8 = additional_texts[8]
        );

        statements.push_str(&insert);
    }

    statements
}
