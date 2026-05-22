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
        //   - SQLite’s storage model treats date/time as TEXT/REAL/INTEGER. We serialize RowValues::Timestamp to TEXT for parity across “SQLite-compatible” backends.
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
    if params.is_empty() {
        turso::params::Params::None
    } else {
        let values: Vec<turso::Value> = params
            .iter()
            .map(|p| row_value_to_turso_value(p, false))
            .collect();
        turso::params::Params::Positional(values)
    }
}

fn convert_params_for_execute(params: &[RowValues]) -> turso::params::Params {
    if params.is_empty() {
        turso::params::Params::None
    } else {
        let values: Vec<turso::Value> = params
            .iter()
            .map(|p| row_value_to_turso_value(p, true))
            .collect();
        turso::params::Params::Positional(values)
    }
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

/// Reusable Turso parameter buffer for hot prepared-statement loops.
///
/// Callers can allocate once, mutate values in place with the `set_*` methods, and pass the
/// buffer to `TursoNonTxPreparedStatement::*_params` methods without rebuilding driver values from
/// `RowValues` on every iteration.
#[derive(Debug, Clone, Default)]
pub struct TursoParamsBuf {
    values: Vec<turso::Value>,
}

impl TursoParamsBuf {
    /// Create an empty buffer with room for `capacity` parameters.
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            values: Vec::with_capacity(capacity),
        }
    }

    /// Remove all values while keeping allocated capacity.
    pub fn clear(&mut self) {
        self.values.clear();
    }

    /// Number of parameters currently in the buffer.
    #[must_use]
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// Whether the buffer has no parameters.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// Build Turso parameters from the current buffer contents.
    #[must_use]
    pub fn to_params(&self) -> turso::params::Params {
        if self.values.is_empty() {
            turso::params::Params::None
        } else {
            turso::params::Params::Positional(self.values.clone())
        }
    }

    /// Set an integer parameter at zero-based `index`.
    pub fn set_int(&mut self, index: usize, value: i64) {
        self.set_value(index, turso::Value::Integer(value));
    }

    /// Set a floating-point parameter at zero-based `index`.
    pub fn set_float(&mut self, index: usize, value: f64) {
        self.set_value(index, turso::Value::Real(value));
    }

    /// Set a text parameter at zero-based `index`.
    pub fn set_text(&mut self, index: usize, value: impl Into<String>) {
        self.set_value(index, turso::Value::Text(value.into()));
    }

    /// Set a boolean parameter at zero-based `index`.
    pub fn set_bool(&mut self, index: usize, value: bool) {
        self.set_value(index, turso::Value::Integer(i64::from(value)));
    }

    /// Set a timestamp parameter at zero-based `index`.
    pub fn set_timestamp(&mut self, index: usize, value: chrono::NaiveDateTime) {
        self.set_value(
            index,
            turso::Value::Text(value.format("%F %T%.f").to_string()),
        );
    }

    /// Set a JSON parameter at zero-based `index`.
    pub fn set_json(&mut self, index: usize, value: serde_json::Value) {
        self.set_value(index, turso::Value::Text(value.to_string()));
    }

    /// Set a blob parameter at zero-based `index`.
    pub fn set_blob(&mut self, index: usize, value: impl Into<Vec<u8>>) {
        self.set_value(index, turso::Value::Blob(value.into()));
    }

    /// Set a NULL parameter at zero-based `index`.
    pub fn set_null(&mut self, index: usize) {
        self.set_value(index, turso::Value::Null);
    }

    fn set_value(&mut self, index: usize, value: turso::Value) {
        if self.values.len() <= index {
            self.values.resize_with(index + 1, || turso::Value::Null);
        }
        self.values[index] = value;
    }
}
