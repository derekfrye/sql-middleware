// postgres.rs
use std::error::Error;

use crate::middleware::{
    ConfigAndPool, ConversionMode, CustomDbRow, DatabaseType, MiddlewarePool, ParamConverter,
    ResultSet, RowValues, SqlMiddlewareDbError,
};
use chrono::NaiveDateTime;
use deadpool_postgres::Transaction;
use deadpool_postgres::{Config as PgConfig, Object};
use serde_json::Value;
use tokio_postgres::{
    types::{to_sql_checked, IsNull, ToSql, Type},
    NoTls, Statement,
};
use tokio_util::bytes;

// If you prefer to keep the `From<tokio_postgres::Error>` for DbError here,
// you can do so. But note we’ve already declared the variant in db_model.
impl From<tokio_postgres::Error> for SqlMiddlewareDbError {
    fn from(err: tokio_postgres::Error) -> Self {
        SqlMiddlewareDbError::PostgresError(err)
    }
}

impl ConfigAndPool {
    /// Asynchronous initializer for ConfigAndPool with Sqlite using deadpool_sqlite
    pub async fn new_postgres(pg_config: PgConfig) -> Result<Self, SqlMiddlewareDbError> {
        if pg_config.dbname.is_none() {
            panic!("dbname is required");
        }

        if pg_config.host.is_none() {
            panic!("host is required");
        }
        if pg_config.port.is_none() {
            panic!("port is required");
        }
        if pg_config.user.is_none() {
            panic!("user is required");
        }
        if pg_config.password.is_none() {
            panic!("password is required");
        }

        let pg_config_res = pg_config.create_pool(Some(deadpool_postgres::Runtime::Tokio1), NoTls);
        match pg_config_res {
            Ok(pg_pool) => Ok(ConfigAndPool {
                pool: MiddlewarePool::Postgres(pg_pool),
                db_type: DatabaseType::Postgres,
            }),
            Err(e) => {
                panic!("Failed to create deadpool_postgres pool: {}", e);
            }
        }
    }
}

pub struct Params<'a> {
    references: Vec<&'a (dyn ToSql + Sync)>,
}

impl<'a> Params<'a> {
    // Single parameter conversion remains mostly the same
    pub fn convert(params: &'a [RowValues]) -> Result<Params<'a>, SqlMiddlewareDbError> {
        let references: Vec<&(dyn ToSql + Sync)> =
            params.iter().map(|p| p as &(dyn ToSql + Sync)).collect();

        Ok(Params { references })
    }

    // Adjusted convert_for_batch method
    pub fn convert_for_batch(
        params: &'a Vec<RowValues>,
    ) -> Result<Vec<&'a (dyn ToSql + Sync + 'a)>, SqlMiddlewareDbError> {
        let mut references = Vec::new();
        for p in params {
            references.push(p as &(dyn ToSql + Sync));
        }

        Ok(references)
    }

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
            RowValues::JSON(jsval) => jsval.to_string().to_sql(ty, out),
            RowValues::Blob(bytes) => bytes.to_sql(ty, out),
        }
    }

    fn accepts(_ty: &Type) -> bool {
        // Implement type acceptance logic based on your needs
        true
    }

    to_sql_checked!();
}

pub async fn build_result_set<'a>(
    stmt: &Statement,
    params: &[&(dyn ToSql + Sync)],
    transaction: &Transaction<'a>,
) -> Result<ResultSet, SqlMiddlewareDbError> {
    // Execute the query
    let rows = transaction
        .query(stmt, params)
        .await
        .map_err(SqlMiddlewareDbError::PostgresError)?;

    let column_names: Vec<String> = stmt
        .columns()
        .iter()
        .map(|col| col.name().to_string())
        .collect();

    let mut result_set = ResultSet::new();

    for row in rows {
        let mut row_values = Vec::new();

        for (i, _col_name) in column_names.iter().enumerate() {
            let value = postgres_extract_value(&row, i)?;
            row_values.push(value);
        }

        result_set.results.push(CustomDbRow {
            column_names: column_names.clone(),
            rows: row_values,
        });

        result_set.rows_affected += 1;
    }

    Ok(result_set)
}

/// Extracts a RowValues from a tokio_postgres Row at the given index
fn postgres_extract_value(
    row: &tokio_postgres::Row,
    idx: usize,
) -> Result<RowValues, SqlMiddlewareDbError> {
    // Determine the type of the column and extract accordingly
    let type_info = row.columns()[idx].type_();

    // Match on the type based on PostgreSQL type OIDs or names
    // For simplicity, we'll handle common types. You may need to expand this.
    if type_info.name() == "int4" || type_info.name() == "int8" {
        let val: Option<i64> = row
            .try_get(idx)
            .map_err(SqlMiddlewareDbError::PostgresError)?;
        Ok(val.map_or(RowValues::Null, RowValues::Int))
    } else if type_info.name() == "float4" || type_info.name() == "float8" {
        let val: Option<f64> = row
            .try_get(idx)
            .map_err(SqlMiddlewareDbError::PostgresError)?;
        Ok(val.map_or(RowValues::Null, RowValues::Float))
    } else if type_info.name() == "bool" {
        let val: Option<bool> = row
            .try_get(idx)
            .map_err(SqlMiddlewareDbError::PostgresError)?;
        Ok(val.map_or(RowValues::Null, RowValues::Bool))
    } else if type_info.name() == "timestamp" || type_info.name() == "timestamptz" {
        let val: Option<NaiveDateTime> = row
            .try_get(idx)
            .map_err(SqlMiddlewareDbError::PostgresError)?;
        Ok(val.map_or(RowValues::Null, RowValues::Timestamp))
    } else if type_info.name() == "json" || type_info.name() == "jsonb" {
        let val: Option<Value> = row
            .try_get(idx)
            .map_err(SqlMiddlewareDbError::PostgresError)?;
        Ok(val.map_or(RowValues::Null, RowValues::JSON))
    } else if type_info.name() == "bytea" {
        let val: Option<Vec<u8>> = row
            .try_get(idx)
            .map_err(SqlMiddlewareDbError::PostgresError)?;
        Ok(val.map_or(RowValues::Null, RowValues::Blob))
    } else if type_info.name() == "text"
        || type_info.name() == "varchar"
        || type_info.name() == "char"
    {
        let val: Option<String> = row
            .try_get(idx)
            .map_err(SqlMiddlewareDbError::PostgresError)?;
        Ok(val.map_or(RowValues::Null, RowValues::Text))
    } else {
        // For other types, attempt to get as string
        let val: Option<String> = row
            .try_get(idx)
            .map_err(SqlMiddlewareDbError::PostgresError)?;
        Ok(val.map_or(RowValues::Null, RowValues::Text))
    }
}

pub async fn execute_batch(
    pg_client: &mut Object,
    query: &str,
) -> Result<(), SqlMiddlewareDbError> {
    // Begin a transaction
    let tx = pg_client.transaction().await?;

    // Execute the batch of queries
    tx.batch_execute(query).await?;

    // Commit the transaction
    tx.commit().await?;

    Ok(())
}

pub async fn execute_select(
    pg_client: &mut Object,
    query: &str,
    params: &[RowValues],
) -> Result<ResultSet, SqlMiddlewareDbError> {
    let params = Params::convert(params)?;
    let tx = pg_client.transaction().await?;
    let stmt = tx.prepare(query).await?;
    let result_set = build_result_set(&stmt, params.as_refs(), &tx).await?;
    tx.commit().await?;
    Ok(result_set)
}

pub async fn execute_dml(
    pg_client: &mut Object,
    query: &str,
    params: &[RowValues],
) -> Result<usize, SqlMiddlewareDbError> {
    let params = Params::convert(params)?;
    let tx = pg_client.transaction().await?;

    let stmt = tx.prepare(query).await?;
    let rows = tx.execute(&stmt, params.as_refs()).await?;
    tx.commit().await?;

    Ok(rows as usize)
}
