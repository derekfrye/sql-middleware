use crate::middleware::{ConversionMode, ParamConverter, RowValues, SqlMiddlewareDbError};

/// Convert a single RowValue to a Turso parameter value
pub fn row_value_to_turso_value(value: &RowValues) -> turso::Value {
    match value {
        RowValues::Int(i) => turso::Value::Integer(*i),
        RowValues::Float(f) => turso::Value::Real(*f),
        RowValues::Text(s) => turso::Value::Text(s.clone()),
        RowValues::Bool(b) => turso::Value::Integer(*b as i64),
        RowValues::Timestamp(dt) => {
            // Format timestamp as ISO 8601 string
            let timestamp_str = dt.format("%Y-%m-%dT%H:%M:%S%.fZ").to_string();
            turso::Value::Text(timestamp_str)
        }
        RowValues::Null => turso::Value::Null,
        RowValues::JSON(jval) => {
            // Serialize JSON to string
            let json_str = jval.to_string();
            turso::Value::Text(json_str)
        }
        RowValues::Blob(bytes) => turso::Value::Blob(bytes.clone()),
    }
}

/// Convert middleware params to Turso parameter values
pub fn convert_params(params: &[RowValues]) -> Result<Vec<turso::Value>, SqlMiddlewareDbError> {
    let mut turso_values = Vec::with_capacity(params.len());
    for param in params {
        turso_values.push(row_value_to_turso_value(param));
    }
    Ok(turso_values)
}

/// Wrapper for Turso parameters for queries
pub struct LimboParams(pub Vec<turso::Value>);

/// Wrapper for Turso parameters for execution (same as queries for now)
pub struct LimboParamsExecute(pub Vec<turso::Value>);

impl ParamConverter<'_> for LimboParams {
    type Converted = Vec<turso::Value>;

    fn convert_sql_params(
        params: &[RowValues],
        _mode: ConversionMode,
    ) -> Result<Self::Converted, SqlMiddlewareDbError> {
        convert_params(params)
    }

    fn supports_mode(_mode: ConversionMode) -> bool {
        true
    }
}

impl ParamConverter<'_> for LimboParamsExecute {
    type Converted = Vec<turso::Value>;

    fn convert_sql_params(
        params: &[RowValues],
        _mode: ConversionMode,
    ) -> Result<Self::Converted, SqlMiddlewareDbError> {
        convert_params(params)
    }

    fn supports_mode(_mode: ConversionMode) -> bool {
        true
    }
}