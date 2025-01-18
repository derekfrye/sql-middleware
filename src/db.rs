use clap::ValueEnum;
use deadpool_postgres::Config;
// use serde_json::Value;
use sqlx::{self, sqlite::SqliteConnectOptions, Column, ConnectOptions, Pool, Row, ValueRef};
// use ::function_name::named;

use crate::model::{CustomDbRow, DatabaseResult, QueryAndParams, ResultSet, RowValues};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum QueryState {
    NoConnection,
    MissingRelations,
    QueryReturnedSuccessfully,
    QueryError,
}

#[derive(Debug, Clone)]
pub enum MiddlewarePool {
    Postgres(Pool<sqlx::Postgres>),
    Sqlite(Pool<sqlx::Sqlite>),
}

#[derive(Debug, Clone, PartialEq, ValueEnum)]
pub enum DatabaseType {
    Postgres,
    Sqlite,
}

#[derive(Clone, Debug)]
pub struct ConfigAndPool {
    pub pool: MiddlewarePool,
    pub db_type: DatabaseType,
}

#[derive(Clone, Debug)]
pub struct Db {
    // pub config_and_pool: DbConfigAndPool,
    pub config_and_pool: ConfigAndPool,
}

impl ConfigAndPool {
    pub async fn new(config: Config, db_type: DatabaseType) -> Self {
        if config.dbname.is_none() {
            panic!("dbname is required");
        }
        if db_type != DatabaseType::Sqlite {
            if config.host.is_none() {
                panic!("host is required");
            }

            if config.port.is_none() {
                panic!("port is required");
            }

            if config.user.is_none() {
                panic!("user is required");
            }

            if config.password.is_none() {
                panic!("password is required");
            }
        }

        let config_db_name = config.dbname.clone().unwrap();

        let connection_string = match db_type {
            DatabaseType::Postgres => {
                format!(
                    "postgres://{}:{}@{}:{}/{}",
                    config.user.unwrap(),
                    config.password.unwrap(),
                    config.host.unwrap(),
                    config.port.unwrap(),
                    config.dbname.unwrap()
                )
            }
            DatabaseType::Sqlite => {
                format!("sqlite://{}", config.dbname.unwrap())
            }
        };

        match db_type {
            DatabaseType::Postgres => {
                let pool_result = sqlx::postgres::PgPoolOptions::new()
                    .connect(&connection_string)
                    .await;
                match pool_result {
                    Ok(pool) => ConfigAndPool {
                        pool: MiddlewarePool::Postgres(pool),
                        db_type: db_type,
                    },
                    Err(e) => {
                        panic!("Failed to create Postgres pool: {}", e);
                    }
                }
            }
            DatabaseType::Sqlite => {
                // #[cfg(debug_assertions)]
                // {
                //     dbg!(&connection_string);
                // }
                let connect = SqliteConnectOptions::new()
                    .filename(&config_db_name)
                    .create_if_missing(true)
                    .connect()
                    .await;
                match connect {
                    Ok(_) => {}
                    Err(e) => {
                        let emessage =
                            format!("Failed in {}, {}: {}", std::file!(), std::line!(), e);
                        panic!("failed here 1, {}", emessage);
                    }
                }
                let pool_result = sqlx::sqlite::SqlitePoolOptions::new()
                    .connect(&connection_string)
                    .await;
                match pool_result {
                    Ok(pool) => ConfigAndPool {
                        pool: MiddlewarePool::Sqlite(pool),
                        db_type: db_type,
                    },
                    Err(e) => {
                        panic!("Failed to create SQLite pool: {}", e);
                    }
                }
            }
        }
    }
}

impl Db {
    pub fn new(cnf: ConfigAndPool) -> Result<Self, String> {
        // let cnf_clone = cnf.clone();
        Ok(Self {
            // config_and_pool: cnf,
            config_and_pool: cnf,
        })
    }

    pub async fn exec_general_query(
        &self,
        queries: Vec<QueryAndParams>,
        expect_rows: bool,
    ) -> Result<DatabaseResult<Vec<ResultSet>>, sqlx::Error> {
        let mut final_result = DatabaseResult::<Vec<ResultSet>>::default();

        #[cfg(debug_assertions)]
        {
            if !queries.is_empty()
                && !queries[0].params.is_empty()
                && queries[0].params[0].as_text().is_some()
                && queries[0].params[0].as_text().unwrap().contains("Player1")
            {
                eprintln!("query about to run: {}", queries[0].query);
            }
        }

        if expect_rows {
            match &self.config_and_pool.pool {
                MiddlewarePool::Postgres(pool) => {
                    let mut transaction = match pool.begin().await {
                        Ok(tx) => tx,
                        Err(e) => {
                            final_result.db_last_exec_state = QueryState::QueryError;
                            final_result.error_message = Some(e.to_string());
                            return Ok(final_result);
                        }
                    };

                    for q in queries {
                        let mut query_item = sqlx::query(&q.query);

                        for param in q.params {
                            query_item = match param {
                                RowValues::Int(value) => query_item.bind(value),
                                RowValues::Text(value) => query_item.bind(value),
                                RowValues::Bool(value) => query_item.bind(value),
                                RowValues::Timestamp(value) => query_item.bind(value),
                                RowValues::Null => query_item.bind::<Option<i64>>(None),
                                RowValues::Blob(value) => query_item.bind(value),
                                RowValues::JSON(value) => query_item.bind(value),
                                RowValues::Float(value) => query_item.bind(value),
                            };
                        }

                        let rows_result = query_item.fetch_all(&mut *transaction).await;

                        match rows_result {
                            Ok(rows) => {
                                let mut result_set = ResultSet { results: vec![] };
                                for row in rows {
                                    let column_names = row
                                        .columns()
                                        .iter()
                                        .map(|c| c.name().to_string())
                                        .collect::<Vec<_>>();

                                    let values = row
                                        .columns()
                                        .iter()
                                        .map(|col| {
                                            let type_info = col.type_info().to_string();
                                            let value = match type_info.as_str() {
                                                "INT4" | "INT8" | "BIGINT" | "INTEGER" | "INT" => {
                                                    RowValues::Int(row.get::<i64, _>(col.name()))
                                                }
                                                "TEXT" => {
                                                    RowValues::Text(
                                                        row.get::<String, _>(col.name())
                                                    )
                                                }
                                                "BOOL" | "BOOLEAN" => {
                                                    RowValues::Bool(row.get::<bool, _>(col.name()))
                                                }
                                                "TIMESTAMP" => {
                                                    let timestamp: sqlx::types::chrono::NaiveDateTime = row.get(
                                                        col.name()
                                                    );
                                                    RowValues::Timestamp(timestamp)
                                                }
                                                _ => {
                                                    eprintln!("Unknown column type: {}", type_info);
                                                    unimplemented!("Unknown column type: {}", type_info)
                                                }
                                            };
                                            value
                                        })
                                        .collect::<Vec<_>>();

                                    let custom_row = CustomDbRow {
                                        column_names,
                                        rows: values,
                                    };
                                    result_set.results.push(custom_row);
                                }
                                final_result.return_result.push(result_set);
                            }
                            Err(e) => {
                                let _ = transaction.rollback().await;
                                final_result.db_last_exec_state = QueryState::QueryError;
                                final_result.error_message = Some(e.to_string());
                                return Ok(final_result);
                            }
                        }
                    }
                    let _ = transaction.commit().await;
                    final_result.db_last_exec_state = QueryState::QueryReturnedSuccessfully;
                }
                MiddlewarePool::Sqlite(pool) => {
                    let mut transaction = match pool.begin().await {
                        Ok(tx) => tx,
                        Err(e) => {
                            final_result.db_last_exec_state = QueryState::QueryError;
                            final_result.error_message = Some(e.to_string());
                            return Ok(final_result);
                        }
                    };

                    for q in queries {
                        let mut query_item = sqlx::query(&q.query);

                        for param in &q.params {
                            query_item = match param {
                                RowValues::Int(value) => query_item.bind(value),
                                RowValues::Text(value) => query_item.bind(value),
                                RowValues::Bool(value) => query_item.bind(value),
                                RowValues::Timestamp(value) => query_item.bind(value),
                                RowValues::Null => {
                                    query_item.bind(sqlx::types::chrono::NaiveDateTime::MIN)
                                }
                                RowValues::Blob(value) => query_item.bind(value),
                                RowValues::JSON(value) => query_item.bind(value),
                                RowValues::Float(value) => query_item.bind(value),
                            };
                        }

                        let rows_result = query_item.fetch_all(&mut *transaction).await;

                        match rows_result {
                            Ok(rows) => {
                                let mut result_set = ResultSet { results: vec![] };
                                for row in rows {
                                    let column_names = row
                                        .columns()
                                        .iter()
                                        .map(|c| c.name().to_string())
                                        .collect::<Vec<_>>();

                                    #[cfg(debug_assertions)]
                                    {
                                        if !q.params.is_empty()
                                            && q.params[0].as_text().is_some()
                                            && q.params[0].as_text().unwrap().contains("Player1")
                                        {
                                            // eprintln!("query about to run: {}", queries[0].query);
                                            // for row in rows {
                                            for col in row.columns() {
                                                let mut type_info = col.type_info().to_string();
                                                eprintln!(
                                                    "Debugging Column '{}', type_info: {}",
                                                    col.name(),
                                                    type_info
                                                );
                                                let col_number = col.ordinal();
                                                // let mut type_info = col.type_info().to_string();
                                                if type_info == "NULL" {
                                                    let typ = row.try_get_raw(col_number).unwrap();
                                                    if !typ.is_null() {
                                                        type_info = typ.type_info().to_string();
                                                    }
                                                }
                                                eprintln!(
                                                    "Debugging Column '{}', type_info: {}",
                                                    col.name(),
                                                    type_info
                                                );

                                                // let val = row.try_get::<i64, _>("cnt");
                                                // eprintln!("cnt returned val = {:?}", val);
                                            }
                                        }
                                    }

                                    fn process_column(
                                        row: &sqlx::sqlite::SqliteRow,
                                        column_name: &str,
                                        type_info: &str,
                                    ) -> Result<RowValues, sqlx::Error>
                                    {
                                        match type_info {
                                            "INTEGER" => {
                                                let result = row.try_get::<i64, _>(column_name);
                                                match result {
                                                    Ok(value) => Ok(RowValues::Int(value)),
                                                    Err(err) => Err(err),
                                                }
                                            }
                                            "TEXT" => {
                                                let result =
                                                    row.try_get::<Option<String>, _>(column_name);
                                                match result {
                                                    Ok(value) => Ok(value
                                                        .map(RowValues::Text)
                                                        .unwrap_or(RowValues::Null)),
                                                    Err(err) => {
                                                        eprintln!(
                                                            "Error decoding TEXT for column '{}': {}",
                                                            column_name,
                                                            err
                                                        );
                                                        Err(err)
                                                    }
                                                }
                                            }
                                            "BOOLEAN" => {
                                                let result =
                                                    row.try_get::<Option<bool>, _>(column_name);
                                                match result {
                                                    Ok(value) => Ok(value
                                                        .map(RowValues::Bool)
                                                        .unwrap_or(RowValues::Null)),
                                                    Err(err) => Err(err),
                                                }
                                            }
                                            "DATETIME" => {
                                                let result = row
                                                    .try_get::<Option<chrono::NaiveDateTime>, _>(
                                                        column_name,
                                                    );
                                                match result {
                                                    Ok(value) => Ok(value
                                                        .map(RowValues::Timestamp)
                                                        .unwrap_or(RowValues::Null)),
                                                    Err(err) => Err(err),
                                                }
                                            }
                                            "REAL" => {
                                                let result = row.try_get::<f64, _>(column_name);
                                                match result {
                                                    Ok(value) => Ok(RowValues::Float(value)),
                                                    Err(err) => Err(err),
                                                }
                                            }
                                            // not actually a type stored in sqlite, so we can't detect and decode it
                                            // see https://www.sqlite.org/json1.html#compiling_in_json_support, 3. Interface Overview
                                            // "SQLite stores JSON as ordinary text.
                                            // Backwards compatibility constraints mean that SQLite is only able to store values that are NULL,
                                            // integers, floating-point numbers, text, and BLOBs. It is not possible to add a new "JSON" type."
                                            // "JSON" => {
                                            //     let result =
                                            //         row.try_get::<Option<Value>, _>(column_name);
                                            //     match result {
                                            //         Ok(value) => Ok(value
                                            //             .map(RowValues::JSON)
                                            //             .unwrap_or(RowValues::Null)),
                                            //         Err(err) => Err(err),
                                            //     }
                                            // }
                                            "NULL" => {
                                                let result =
                                                    row.try_get::<Option<String>, _>(column_name);
                                                match result {
                                                    Ok(value) => Ok(value
                                                        .map(RowValues::Text)
                                                        .unwrap_or(RowValues::Null)),
                                                    Err(err) => Err(err),
                                                }
                                            }
                                            "BLOB" => {
                                                let result =
                                                    row.try_get::<Option<Vec<u8>>, _>(column_name);
                                                match result {
                                                    Ok(value) => Ok(value
                                                        .map(RowValues::Blob)
                                                        .unwrap_or(RowValues::Null)),
                                                    Err(err) => Err(err),
                                                }
                                            }
                                            _ => {
                                                eprintln!("sqlx-middleware custom err: Unknown column type: {}", type_info);
                                                unimplemented!("sqlx-middleware custom err: Unknown column type: {}", type_info);
                                            }
                                        }
                                    }

                                    let values1: Result<Vec<RowValues>, sqlx::Error> = row
                                        .columns()
                                        .iter()
                                        .map(|col| {
                                            // Step 1: Get column number and initial type info
                                            let col_number = col.ordinal();
                                            let initial_type_info = col.type_info().to_string();

                                            // Step 2: Adjust type info if it is "NULL"
                                            let type_info = if initial_type_info == "NULL" {
                                                let typ = row.try_get_raw(col_number).unwrap();
                                                if !typ.is_null() {
                                                    typ.type_info().to_string()
                                                } else {
                                                    initial_type_info
                                                }
                                            } else {
                                                initial_type_info
                                            };

                                            // Step 3: Get column name
                                            let column_name = col.name();

                                            // Debugging block (only active in debug builds)
                                            #[cfg(debug_assertions)]
                                            {
                                                if column_name == "g" {
                                                    eprintln!(
                                                        "Debugging Column '{}', type_info: {}",
                                                        col.name(),
                                                        type_info
                                                    );
                                                }
                                            }

                                            // Step 4: Process column and propagate any errors
                                            process_column(&row, column_name, &type_info)
                                        })
                                        .collect();

                                    // dbg!(&values);

                                    let custom_row = CustomDbRow {
                                        column_names,
                                        rows: values1?,
                                    };
                                    result_set.results.push(custom_row);
                                }
                                final_result.return_result.push(result_set);
                            }
                            Err(e) => {
                                let _ = transaction.rollback().await;
                                final_result.db_last_exec_state = QueryState::QueryError;
                                final_result.error_message = Some(e.to_string());
                                return Ok(final_result);
                            }
                        }
                    }
                    let _ = transaction.commit().await;
                    final_result.db_last_exec_state = QueryState::QueryReturnedSuccessfully;
                }
            }
        } else {
            // expect_rows = false
            match &self.config_and_pool.pool {
                MiddlewarePool::Postgres(pool) => {
                    let mut transaction = match pool.begin().await {
                        Ok(tx) => tx,
                        Err(e) => {
                            final_result.db_last_exec_state = QueryState::QueryError;
                            final_result.error_message = Some(e.to_string());
                            return Ok(final_result);
                        }
                    };

                    for q in queries {
                        let mut query_item = sqlx::query(&q.query);

                        for param in q.params {
                            query_item = match param {
                                RowValues::Int(value) => query_item.bind(value),
                                RowValues::Text(value) => query_item.bind(value),
                                RowValues::Bool(value) => query_item.bind(value),
                                RowValues::Timestamp(value) => query_item.bind(value),
                                RowValues::Null => query_item.bind::<Option<i64>>(None),
                                RowValues::Blob(value) => query_item.bind(value),
                                RowValues::JSON(value) => query_item.bind(value),
                                RowValues::Float(value) => query_item.bind(value),
                            };
                        }

                        let exec_result = query_item.execute(&mut *transaction).await;

                        match exec_result {
                            Ok(_) => {
                                final_result
                                    .return_result
                                    .push(ResultSet { results: vec![] });
                            }
                            Err(e) => {
                                let _ = transaction.rollback().await;
                                final_result.db_last_exec_state = QueryState::QueryError;
                                final_result.error_message = Some(e.to_string());
                                return Ok(final_result);
                            }
                        }
                    }
                    let _ = transaction.commit().await;
                    final_result.db_last_exec_state = QueryState::QueryReturnedSuccessfully;
                }
                MiddlewarePool::Sqlite(pool) => {
                    let mut transaction = match pool.begin().await {
                        Ok(tx) => tx,
                        Err(e) => {
                            final_result.db_last_exec_state = QueryState::QueryError;
                            final_result.error_message = Some(e.to_string());
                            return Ok(final_result);
                        }
                    };

                    for q in queries {
                        let mut query_item = sqlx::query(&q.query);

                        for param in q.params {
                            query_item = match param {
                                RowValues::Int(value) => query_item.bind(value),
                                RowValues::Text(value) => query_item.bind(value),
                                RowValues::Bool(value) => query_item.bind(value),
                                RowValues::Timestamp(value) => query_item.bind(value),
                                RowValues::Null => query_item.bind::<Option<i64>>(None),
                                RowValues::Blob(value) => query_item.bind(value),
                                RowValues::JSON(value) => query_item.bind(value),
                                RowValues::Float(value) => query_item.bind(value),
                            };
                        }

                        let exec_result = query_item.execute(&mut *transaction).await;

                        match exec_result {
                            Ok(_) => {
                                final_result
                                    .return_result
                                    .push(ResultSet { results: vec![] });
                            }
                            Err(e) => {
                                eprintln!("sqlx-middleware error: {}", e);
                                let _ = transaction.rollback().await;
                                final_result.db_last_exec_state = QueryState::QueryError;
                                final_result.error_message = Some(e.to_string());
                                return Ok(final_result);
                            }
                        }
                    }
                    let _ = transaction.commit().await;
                    final_result.db_last_exec_state = QueryState::QueryReturnedSuccessfully;
                }
            }
        }

        Ok(final_result)
    }
}
