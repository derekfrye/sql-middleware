use crate::middleware::{ConversionMode, ParamConverter, RowValues, SqlMiddlewareDbError};

/// Container for libsql parameters
pub struct Params(pub Vec<deadpool_libsql::libsql::Value>);

impl Params {
    /// Convert from a slice of `RowValues` to libsql parameters
    pub fn convert(params: &[RowValues]) -> Result<Params, SqlMiddlewareDbError> {
        let mut libsql_params = Vec::with_capacity(params.len());

        for param in params {
            let libsql_value = match param {
                RowValues::Int(i) => deadpool_libsql::libsql::Value::Integer(*i),
                RowValues::Float(f) => deadpool_libsql::libsql::Value::Real(*f),
                RowValues::Text(s) => deadpool_libsql::libsql::Value::Text(s.clone()),
                RowValues::Bool(b) => deadpool_libsql::libsql::Value::Integer(i64::from(*b)),
                RowValues::Timestamp(dt) => {
                    deadpool_libsql::libsql::Value::Text(dt.format("%F %T%.f").to_string())
                }
                RowValues::Null => deadpool_libsql::libsql::Value::Null,
                RowValues::JSON(jval) => deadpool_libsql::libsql::Value::Text(jval.to_string()),
                RowValues::Blob(bytes) => deadpool_libsql::libsql::Value::Blob(bytes.clone()),
            };
            libsql_params.push(libsql_value);
        }

        Ok(Params(libsql_params))
    }

    /// Get a reference to the underlying parameter array
    #[must_use]
    pub fn as_slice(&self) -> &[deadpool_libsql::libsql::Value] {
        &self.0
    }

    /// Convert to owned vector for use with libsql API
    #[must_use]
    pub fn into_vec(self) -> Vec<deadpool_libsql::libsql::Value> {
        self.0
    }
}

impl ParamConverter<'_> for Params {
    type Converted = Self;

    fn convert_sql_params(
        params: &[RowValues],
        _mode: ConversionMode,
    ) -> Result<Self::Converted, SqlMiddlewareDbError> {
        Self::convert(params)
    }

    /// libsql supports both query and execution modes
    fn supports_mode(_mode: ConversionMode) -> bool {
        true
    }
}
