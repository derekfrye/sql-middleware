// postgres.rs
use std::error::Error;

use crate::middleware::{
    ConfigAndPool, ConversionMode, DatabaseType, MiddlewarePool, ParamConverter,
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

// The #[from] attribute on the SqlMiddlewareDbError::PostgresError variant
// automatically generates this implementation

impl ConfigAndPool {
    /// Asynchronous initializer for ConfigAndPool with Postgres
    pub async fn new_postgres(pg_config: PgConfig) -> Result<Self, SqlMiddlewareDbError> {
        // Validate all required config fields are present
        if pg_config.dbname.is_none() {
            return Err(SqlMiddlewareDbError::ConfigError("dbname is required".to_string()));
        }

        if pg_config.host.is_none() {
            return Err(SqlMiddlewareDbError::ConfigError("host is required".to_string()));
        }
        if pg_config.port.is_none() {
            return Err(SqlMiddlewareDbError::ConfigError("port is required".to_string()));
        }
        if pg_config.user.is_none() {
            return Err(SqlMiddlewareDbError::ConfigError("user is required".to_string()));
        }
        if pg_config.password.is_none() {
            return Err(SqlMiddlewareDbError::ConfigError("password is required".to_string()));
        }

        // Attempt to create connection pool
        let pg_pool = pg_config
            .create_pool(Some(deadpool_postgres::Runtime::Tokio1), NoTls)
            .map_err(|e| SqlMiddlewareDbError::ConnectionError(format!("Failed to create Postgres pool: {}", e)))?;
            
        Ok(ConfigAndPool {
            pool: MiddlewarePool::Postgres(pg_pool),
            db_type: DatabaseType::Postgres,
        })
    }
}

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

/// Build a result set from a Postgres query execution
pub async fn build_result_set(
    stmt: &Statement,
    params: &[&(dyn ToSql + Sync)],
    transaction: &Transaction<'_>,
) -> Result<ResultSet, SqlMiddlewareDbError> {
    // Execute the query
    let rows = transaction
        .query(stmt, params)
        .await?;

    let column_names: Vec<String> = stmt
        .columns()
        .iter()
        .map(|col| col.name().to_string())
        .collect();

    // Preallocate capacity if we can estimate the number of rows
    let capacity = rows.len();
    let mut result_set = ResultSet::with_capacity(capacity);
    // Store column names once in the result set
    let column_names_rc = std::sync::Arc::new(column_names);
    result_set.set_column_names(column_names_rc);

    for row in rows {
        let mut row_values = Vec::new();

        let col_count = result_set.get_column_names()
            .ok_or_else(|| SqlMiddlewareDbError::ExecutionError("No column names available".to_string()))?
            .len();
            
        for i in 0..col_count {
            let value = postgres_extract_value(&row, i)?;
            row_values.push(value);
        }

        result_set.add_row_values(row_values);
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
            .try_get(idx)?;
        Ok(val.map_or(RowValues::Null, RowValues::Int))
    } else if type_info.name() == "float4" || type_info.name() == "float8" {
        let val: Option<f64> = row
            .try_get(idx)?;
        Ok(val.map_or(RowValues::Null, RowValues::Float))
    } else if type_info.name() == "bool" {
        let val: Option<bool> = row
            .try_get(idx)?;
        Ok(val.map_or(RowValues::Null, RowValues::Bool))
    } else if type_info.name() == "timestamp" || type_info.name() == "timestamptz" {
        let val: Option<NaiveDateTime> = row
            .try_get(idx)?;
        Ok(val.map_or(RowValues::Null, RowValues::Timestamp))
    } else if type_info.name() == "json" || type_info.name() == "jsonb" {
        let val: Option<Value> = row
            .try_get(idx)?;
        Ok(val.map_or(RowValues::Null, RowValues::JSON))
    } else if type_info.name() == "bytea" {
        let val: Option<Vec<u8>> = row
            .try_get(idx)?;
        Ok(val.map_or(RowValues::Null, RowValues::Blob))
    } else if type_info.name() == "text"
        || type_info.name() == "varchar"
        || type_info.name() == "char"
    {
        let val: Option<String> = row
            .try_get(idx)?;
        Ok(val.map_or(RowValues::Null, RowValues::Text))
    } else {
        // For other types, attempt to get as string
        let val: Option<String> = row
            .try_get(idx)?;
        Ok(val.map_or(RowValues::Null, RowValues::Text))
    }
}

/// Execute a batch of SQL statements for Postgres
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

/// Execute a SELECT query with parameters
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

/// Execute a DML query (INSERT, UPDATE, DELETE) with parameters
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