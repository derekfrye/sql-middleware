use crate::adapters::params::convert_params;
use crate::middleware::{RowValues, SqlMiddlewareDbError};
use crate::types::ConversionMode;
use tokio_postgres::{Client, Statement};

use super::params::Params as PgParams;

pub(crate) async fn query_rows_on_client(
    client: &Client,
    query: &str,
    stmt: Option<&Statement>,
    params: &[RowValues],
    err_label: &str,
) -> Result<Vec<tokio_postgres::Row>, SqlMiddlewareDbError> {
    let converted = convert_params::<PgParams>(params, ConversionMode::Query)?;
    let refs = converted.as_refs();
    let rows = match stmt {
        Some(stmt) => client.query(stmt, refs).await,
        None => client.query(query, refs).await,
    };
    rows.map_err(|e| SqlMiddlewareDbError::ExecutionError(format!("{err_label}: {e}")))
}

pub(crate) async fn execute_rows_on_client(
    client: &Client,
    query: &str,
    stmt: Option<&Statement>,
    params: &[RowValues],
    err_label: &str,
) -> Result<u64, SqlMiddlewareDbError> {
    let converted = convert_params::<PgParams>(params, ConversionMode::Execute)?;
    let refs = converted.as_refs();
    let rows = match stmt {
        Some(stmt) => client.execute(stmt, refs).await,
        None => client.execute(query, refs).await,
    };
    rows.map_err(|e| SqlMiddlewareDbError::ExecutionError(format!("{err_label}: {e}")))
}
