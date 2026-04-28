use chrono::NaiveDateTime;
use serde_json::json;

pub(super) fn assert_selected_rows(res: &sql_middleware::ResultSet) {
    assert_alpha_row(res);
    assert_charlie_row(res);
    assert_juliet_row(res);
}

fn assert_alpha_row(res: &sql_middleware::ResultSet) {
    assert_eq!(*res.results[0].get("recid").unwrap().as_int().unwrap(), 1);
    assert_eq!(*res.results[0].get("a").unwrap().as_int().unwrap(), 1);
    assert_eq!(res.results[0].get("b").unwrap().as_text().unwrap(), "Alpha");
    assert_eq!(
        res.results[0].get("c").unwrap().as_timestamp().unwrap(),
        NaiveDateTime::parse_from_str("2024-01-01 08:00:01", "%Y-%m-%d %H:%M:%S").unwrap()
    );
    assert!((res.results[0].get("d").unwrap().as_float().unwrap() - 10.5).abs() < f64::EPSILON);
    assert!(*res.results[0].get("e").unwrap().as_bool().unwrap());
    assert_eq!(
        res.results[0].get("f").unwrap().as_blob().unwrap(),
        b"Blob12"
    );
    assert_eq!(
        json!(res.results[0].get("g").unwrap().as_text().unwrap()),
        json!(r#"{"name": "Alice", "age": 30}"#)
    );
}

fn assert_charlie_row(res: &sql_middleware::ResultSet) {
    assert_eq!(*res.results[2].get("recid").unwrap().as_int().unwrap(), 3);
    assert_eq!(*res.results[2].get("a").unwrap().as_int().unwrap(), 3);
    assert_eq!(
        res.results[2].get("b").unwrap().as_text().unwrap(),
        "Charlie"
    );
    assert_eq!(
        res.results[2].get("c").unwrap().as_timestamp().unwrap(),
        NaiveDateTime::parse_from_str("2024-01-03 10:30:00", "%Y-%m-%d %H:%M:%S").unwrap()
    );
    assert!((res.results[2].get("d").unwrap().as_float().unwrap() - 30.25).abs() < f64::EPSILON);
    assert!(*res.results[2].get("e").unwrap().as_bool().unwrap());
}

fn assert_juliet_row(res: &sql_middleware::ResultSet) {
    assert_eq!(*res.results[3].get("a").unwrap().as_int().unwrap(), 100);
    assert!((res.results[3].get("d").unwrap().as_float().unwrap() - 100.75).abs() < f64::EPSILON);
    assert_eq!(
        res.results[3].get("f").unwrap().as_blob().unwrap(),
        b"Blob11"
    );
}
