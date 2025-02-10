use middleware::ConversionMode;
use middleware::ParamConverter;
use middleware::RowValues;

mod postgres;
mod sqlite;

pub mod middleware;

pub use middleware::SqlMiddlewareDbError;

pub use postgres::build_result_set as postgres_build_result_set;
pub use sqlite::build_result_set as sqlite_build_result_set;

pub use postgres::Params as PostgresParams;
pub use sqlite::SqliteParamsExecute;
pub use sqlite::SqliteParamsQuery;

pub fn convert_sql_params<'a, T: ParamConverter<'a>>(
    params: &'a [RowValues],
    mode: ConversionMode,
) -> Result<T::Converted, SqlMiddlewareDbError> {
    T::convert_sql_params(params, mode)
}
