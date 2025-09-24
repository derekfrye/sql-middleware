use crate::middleware::{ConversionMode, ParamConverter, RowValues, SqlMiddlewareDbError};

/// Container for Turso parameters (positional only for now).
pub struct Params(pub turso::params::Params);

fn row_value_to_turso_value(value: &RowValues, _for_execute: bool) -> turso::Value {
    match value {
        RowValues::Int(i) => turso::Value::Integer(*i),
        RowValues::Float(f) => turso::Value::Real(*f),
        RowValues::Text(s) => turso::Value::Text(s.clone()),
        RowValues::Bool(b) => turso::Value::Integer(i64::from(*b)),
        //   - Turso’s Value enum supports: Null, Integer, Real, Text, Blob — no datetime/timestamp.
        //   - SQLite’s storage model treats date/time as TEXT/REAL/INTEGER. In our SQLite converter we already serialize RowValues::Timestamp to TEXT; libsql params do
        //   the same. Turso follows that convention for parity across “SQLite-compatible” backends.
        //   - Using %F %T%.f (e.g., “YYYY-MM-DD HH:MM:SS.sss”) yields:
        //       - Stable, human-readable values.
        //       - Correct lexicographic ordering for chronological sorts.
        //       - Avoids precision loss or timezone surprises that can come with epoch/REAL.
        RowValues::Timestamp(dt) => {
            // Represent as TEXT for compatibility with other backends
            turso::Value::Text(dt.format("%F %T%.f").to_string())
        }
        RowValues::Null => turso::Value::Null,
        RowValues::JSON(j) => turso::Value::Text(j.to_string()),
        RowValues::Blob(bytes) => turso::Value::Blob(bytes.clone()),
    }
}

fn convert_params(params: &[RowValues]) -> turso::params::Params {
    let values: Vec<turso::Value> = params
        .iter()
        .map(|p| row_value_to_turso_value(p, false))
        .collect();
    turso::params::Params::Positional(values)
}

fn convert_params_for_execute(params: &[RowValues]) -> turso::params::Params {
    let values: Vec<turso::Value> = params
        .iter()
        .map(|p| row_value_to_turso_value(p, true))
        .collect();
    turso::params::Params::Positional(values)
}

impl ParamConverter<'_> for Params {
    type Converted = Params;

    fn convert_sql_params(
        params: &[RowValues],
        mode: ConversionMode,
    ) -> Result<Self::Converted, SqlMiddlewareDbError> {
        match mode {
            ConversionMode::Query => Ok(Params(convert_params(params))),
            ConversionMode::Execute => Ok(Params(convert_params_for_execute(params))),
        }
    }

    fn supports_mode(_mode: ConversionMode) -> bool {
        true
    }
}
