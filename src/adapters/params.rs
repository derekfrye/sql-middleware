use crate::middleware::{RowValues, SqlMiddlewareDbError};
use crate::types::{ConversionMode, ParamConverter};

pub(crate) fn convert_params<'a, C>(
    params: &'a [RowValues],
    mode: ConversionMode,
) -> Result<<C as ParamConverter<'a>>::Converted, SqlMiddlewareDbError>
where
    C: ParamConverter<'a>,
{
    C::convert_sql_params(params, mode)
}
