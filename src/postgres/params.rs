use std::error::Error;

use crate::middleware::{ConversionMode, ParamConverter, RowValues, SqlMiddlewareDbError};
use tokio_postgres::types::{IsNull, ToSql, Type, to_sql_checked};
use tokio_util::bytes;

/// Container for Postgres parameters with lifetime tracking
pub struct Params<'a> {
    references: Vec<&'a (dyn ToSql + Sync)>,
}

impl<'a> Params<'a> {
    /// Convert from a slice of RowValues to Postgres parameters
    pub fn convert(params: &'a [RowValues]) -> Result<Params<'a>, SqlMiddlewareDbError> {
        let references: Vec<&(dyn ToSql + Sync)> =
            params.iter().map(|p| p as &(dyn ToSql + Sync)).collect();

        Ok(Params { references })
    }

    /// Convert a slice of RowValues for batch operations
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
            RowValues::Int(i) => (*i).to_sql(ty, out),
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
            // Integer types
            Type::INT2 | Type::INT4 | Type::INT8 => true,
            // Floating point types
            Type::FLOAT4 | Type::FLOAT8 => true,
            // Text types
            Type::TEXT | Type::VARCHAR | Type::CHAR | Type::NAME => true,
            // Boolean type
            Type::BOOL => true,
            // Date/time types
            Type::TIMESTAMP | Type::TIMESTAMPTZ | Type::DATE => true,
            // JSON types
            Type::JSON | Type::JSONB => true,
            // Binary data
            Type::BYTEA => true,
            // For any other type, we don't accept
            _ => false,
        }
    }

    to_sql_checked!();
}
