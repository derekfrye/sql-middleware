//! Parameter conversion utilities.
//!
//! This module provides utilities for converting parameters between different
//! database-specific formats.

use crate::middleware::{ConversionMode, ParamConverter, RowValues, SqlMiddlewareDbError};

/// Convert a slice of generic `RowValues` into database-specific parameters.
///
/// This function uses the `ParamConverter` trait to convert a set of parameters
/// into the format required by a specific database backend.
///
/// # Arguments
///
/// * `params` - The slice of `RowValues` to convert
/// * `mode` - Whether the parameters will be used for a query or execution
///
/// # Returns
///
/// The converted parameters, or an error if conversion fails
///
/// # Errors
///
/// Returns `SqlMiddlewareDbError` if:
/// - The parameter converter doesn't support the specified mode
/// - Parameter conversion fails for any of the provided values
///
/// # Example
///
/// ```rust,no_run
/// use sql_middleware::prelude::*;
/// use sql_middleware::postgres::Params as PostgresParams;
/// use sql_middleware::conversion::convert_sql_params;
///
/// fn convert_parameters<'a>(values: &'a [RowValues]) -> Result<PostgresParams<'a>, SqlMiddlewareDbError> {
///     let mode = ConversionMode::Query;
///     let postgres_params = convert_sql_params::<PostgresParams>(values, mode)?;
///     Ok(postgres_params)
/// }
/// ```
pub fn convert_sql_params<'a, T: ParamConverter<'a>>(
    params: &'a [RowValues],
    mode: ConversionMode,
) -> Result<T::Converted, SqlMiddlewareDbError> {
    // Check if the converter supports this mode
    if !T::supports_mode(mode) {
        return Err(SqlMiddlewareDbError::ParameterError(format!(
            "Converter doesn't support mode: {mode:?}"
        )));
    }
    T::convert_sql_params(params, mode)
}
