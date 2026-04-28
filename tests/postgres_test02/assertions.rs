use chrono::NaiveDateTime;
use sql_middleware::middleware::RowValues;

pub(super) fn assert_inserted_row(result: &sql_middleware::ResultSet) {
    let expected_result = [sql_middleware::test_helpers::create_test_row(
        vec![
            "event_id".to_string(),
            "espn_id".to_string(),
            "name".to_string(),
            "ins_ts".to_string(),
        ],
        vec![
            RowValues::Int(1),
            RowValues::Int(123_456),
            RowValues::Text("test name".to_string()),
            RowValues::Timestamp(
                NaiveDateTime::parse_from_str("2021-08-06 16:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            ),
        ],
    )];
    let cols_to_actually_check = ["espn_id", "name", "ins_ts"];
    for (index, row) in result.results.iter().enumerate() {
        let left = filtered_values(&row.column_names, &row.rows, &cols_to_actually_check);
        let right = filtered_values(
            &expected_result[index].column_names,
            &expected_result[index].rows,
            &cols_to_actually_check,
        );
        assert_eq!(left, right);
    }
}

fn filtered_values(
    columns: &[String],
    values: &[RowValues],
    cols_to_check: &[&str],
) -> Vec<RowValues> {
    columns
        .iter()
        .zip(values)
        .filter(|(col_name, _)| cols_to_check.contains(&col_name.as_str()))
        .map(|(_, value)| value.clone())
        .collect()
}
