use std::error::Error;

use crate::middleware::{ConversionMode, ParamConverter, RowValues, SqlMiddlewareDbError};
use tokio_postgres::types::{IsNull, ToSql, Type, to_sql_checked};
use tokio_util::bytes;

/// Container for Postgres parameters with lifetime tracking
pub struct Params<'a> {
    references: Vec<&'a (dyn ToSql + Sync)>,
}

impl<'a> Params<'a> {
    /// Convert from a slice of `RowValues` to Postgres parameters
    ///
    /// # Errors
    /// Currently never returns an error but maintains Result for API consistency.
    pub fn convert(params: &'a [RowValues]) -> Result<Params<'a>, SqlMiddlewareDbError> {
        let references: Vec<&(dyn ToSql + Sync)> =
            params.iter().map(|p| p as &(dyn ToSql + Sync)).collect();

        Ok(Params { references })
    }

    /// Convert a slice of `RowValues` for batch operations
    ///
    /// # Errors
    /// Currently never returns an error but maintains Result for API consistency.
    pub fn convert_for_batch(
        params: &'a [RowValues],
    ) -> Result<Vec<&'a (dyn ToSql + Sync + 'a)>, SqlMiddlewareDbError> {
        // Pre-allocate capacity for better performance
        let mut references = Vec::with_capacity(params.len());

        // Avoid collect() and just push directly
        for p in params {
            references.push(p as &(dyn ToSql + Sync));
        }

        Ok(references)
    }

    /// Get a reference to the underlying parameter array
    #[must_use]
    pub fn as_refs(&self) -> &[&(dyn ToSql + Sync)] {
        &self.references
    }
}

impl<'a> ParamConverter<'a> for Params<'a> {
    type Converted = Params<'a>;

    fn convert_sql_params(
        params: &'a [RowValues],
        _mode: ConversionMode,
    ) -> Result<Self::Converted, SqlMiddlewareDbError> {
        // Simply delegate to your existing conversion:
        Self::convert(params)
    }

    // PostgresParams supports both query and execution modes
    fn supports_mode(_mode: ConversionMode) -> bool {
        true
    }
}

impl ToSql for RowValues {
    fn to_sql(
        &self,
        ty: &Type,
        out: &mut bytes::BytesMut,
    ) -> Result<IsNull, Box<dyn Error + Sync + Send>> {
        match self {
            RowValues::Int(i) => match *ty {
                Type::INT8 => (*i).to_sql(ty, out),
                Type::INT4 => {
                    let v = i32::try_from(*i).map_err(|_| {
                        SqlMiddlewareDbError::ExecutionError(format!(
                            "integer value {i} overflows Postgres INT4 parameter"
                        ))
                    })?;
                    v.to_sql(ty, out)
                }
                Type::INT2 => {
                    let v = i16::try_from(*i).map_err(|_| {
                        SqlMiddlewareDbError::ExecutionError(format!(
                            "integer value {i} overflows Postgres INT2 parameter"
                        ))
                    })?;
                    v.to_sql(ty, out)
                }
                _ => {
                    return Err(Box::new(SqlMiddlewareDbError::ExecutionError(
                        format!("unsupported integer parameter type: {ty:?}"),
                    )))
                }
            },
            RowValues::Float(f) => (*f).to_sql(ty, out),
            RowValues::Text(s) => s.to_sql(ty, out),
            RowValues::Bool(b) => (*b).to_sql(ty, out),
            RowValues::Timestamp(dt) => dt.to_sql(ty, out),
            RowValues::Null => Ok(IsNull::Yes),
            RowValues::JSON(jsval) => jsval.to_sql(ty, out),
            RowValues::Blob(bytes) => bytes.to_sql(ty, out),
        }
    }

    fn accepts(ty: &Type) -> bool {
        // Only accept types we can properly handle
        match *ty {
            // All supported types
            Type::INT2 | Type::INT4 | Type::INT8 |                    // Integer types
            Type::FLOAT4 | Type::FLOAT8 |                             // Floating point types
            Type::TEXT | Type::VARCHAR | Type::CHAR | Type::NAME |    // Text types
            Type::BOOL |                                              // Boolean type
            Type::TIMESTAMP | Type::TIMESTAMPTZ | Type::DATE |        // Date/time types
            Type::JSON | Type::JSONB |                                // JSON types
            Type::BYTEA => true,                                      // Binary data
            // For any other type, we don't accept
            _ => false,
        }
    }

    to_sql_checked!();
}
