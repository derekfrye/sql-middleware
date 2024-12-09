use deadpool_postgres::Config;
use sqlx::{self, sqlite::SqliteConnectOptions, Column, ConnectOptions, Pool, Row};
// use ::function_name::named;

use crate::model::{CustomDbRow, DatabaseResult, QueryAndParams, ResultSet, RowValues};

#[derive(Debug, Clone)]
pub enum DbPool {
    Postgres(Pool<sqlx::Postgres>),
    Sqlite(Pool<sqlx::Sqlite>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum DatabaseType {
    Postgres,
    Sqlite,
}

#[derive(Clone, Debug)]
pub struct DbConfigAndPool {
    pool: DbPool,
    // db_type: DatabaseType,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DatabaseSetupState {
    NoConnection,
    MissingRelations,
    QueryReturnedSuccessfully,
    QueryError,
}

#[derive(Clone, Debug)]
pub struct Db {
    // pub config_and_pool: DbConfigAndPool,
    pub pool: DbPool,
}

impl DbConfigAndPool {
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
                    Ok(pool) => DbConfigAndPool {
                        pool: DbPool::Postgres(pool),
                        // db_type,
                    },
                    Err(e) => {
                        panic!("Failed to create Postgres pool: {}", e);
                    }
                }
            }
            DatabaseType::Sqlite => {
                #[cfg(debug_assertions)]
                {
                    dbg!(&connection_string);
                }
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
                    Ok(pool) => DbConfigAndPool {
                        pool: DbPool::Sqlite(pool),
                        // db_type,
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
    pub fn new(cnf: DbConfigAndPool) -> Result<Self, String> {
        // let cnf_clone = cnf.clone();
        Ok(Self {
            // config_and_pool: cnf,
            pool: cnf.pool,
        })
    }

    pub async fn exec_general_query(
        &self,
        queries: Vec<QueryAndParams>,
        expect_rows: bool,
    ) -> Result<DatabaseResult<Vec<ResultSet>>, sqlx::Error> {
        let mut final_result = DatabaseResult::<Vec<ResultSet>>::default();

        if expect_rows {
            match &self.pool {
                DbPool::Postgres(pool) => {
                    let mut transaction = match pool.begin().await {
                        Ok(tx) => tx,
                        Err(e) => {
                            final_result.db_last_exec_state = DatabaseSetupState::QueryError;
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
                                                    RowValues::Text(row.get::<String, _>(col.name()))
                                                }
                                                "BOOL" | "BOOLEAN" => {
                                                    RowValues::Bool(row.get::<bool, _>(col.name()))
                                                }
                                                "TIMESTAMP" => {
                                                    let timestamp: sqlx::types::chrono::NaiveDateTime =
                                                        row.get(col.name());
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
                                        column_names: column_names,
                                        rows: values,
                                    };
                                    result_set.results.push(custom_row);
                                }
                                final_result.return_result.push(result_set);
                            }
                            Err(e) => {
                                let _ = transaction.rollback().await;
                                final_result.db_last_exec_state = DatabaseSetupState::QueryError;
                                final_result.error_message = Some(e.to_string());
                                return Ok(final_result);
                            }
                        }
                    }
                    let _ = transaction.commit().await;
                    final_result.db_last_exec_state = DatabaseSetupState::QueryReturnedSuccessfully;
                }
                DbPool::Sqlite(pool) => {
                    let mut transaction = match pool.begin().await {
                        Ok(tx) => tx,
                        Err(e) => {
                            final_result.db_last_exec_state = DatabaseSetupState::QueryError;
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
                                                "INTEGER" => {
                                                    RowValues::Int(row.get::<i64, _>(col.name()))
                                                }
                                                "TEXT" => RowValues::Text(
                                                    row.get::<String, _>(col.name()),
                                                ),
                                                "BOOLEAN" => {
                                                    RowValues::Bool(row.get::<bool, _>(col.name()))
                                                }
                                                "TIMESTAMP" => {
                                                    RowValues::Timestamp(row.get(col.name()))
                                                }
                                                _ => {
                                                    eprintln!("Unknown column type: {}", type_info);
                                                    unimplemented!(
                                                        "Unknown column type: {}",
                                                        type_info
                                                    )
                                                }
                                            };
                                            value
                                        })
                                        .collect::<Vec<_>>();

                                    let custom_row = CustomDbRow {
                                        column_names: column_names,
                                        rows: values,
                                    };
                                    result_set.results.push(custom_row);
                                }
                                final_result.return_result.push(result_set);
                            }
                            Err(e) => {
                                let _ = transaction.rollback().await;
                                final_result.db_last_exec_state = DatabaseSetupState::QueryError;
                                final_result.error_message = Some(e.to_string());
                                return Ok(final_result);
                            }
                        }
                    }
                    let _ = transaction.commit().await;
                    final_result.db_last_exec_state = DatabaseSetupState::QueryReturnedSuccessfully;
                }
            }
        } else {
            // expect_rows = false
            match &self.pool {
                DbPool::Postgres(pool) => {
                    let mut transaction = match pool.begin().await {
                        Ok(tx) => tx,
                        Err(e) => {
                            final_result.db_last_exec_state = DatabaseSetupState::QueryError;
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
                                final_result.db_last_exec_state = DatabaseSetupState::QueryError;
                                final_result.error_message = Some(e.to_string());
                                return Ok(final_result);
                            }
                        }
                    }
                    let _ = transaction.commit().await;
                    final_result.db_last_exec_state = DatabaseSetupState::QueryReturnedSuccessfully;
                }
                DbPool::Sqlite(pool) => {
                    let mut transaction = match pool.begin().await {
                        Ok(tx) => tx,
                        Err(e) => {
                            final_result.db_last_exec_state = DatabaseSetupState::QueryError;
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
                                final_result.db_last_exec_state = DatabaseSetupState::QueryError;
                                final_result.error_message = Some(e.to_string());
                                return Ok(final_result);
                            }
                        }
                    }
                    let _ = transaction.commit().await;
                    final_result.db_last_exec_state = DatabaseSetupState::QueryReturnedSuccessfully;
                }
            }
        }

        Ok(final_result)
    }
}
