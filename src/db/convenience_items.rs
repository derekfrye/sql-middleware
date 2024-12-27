use crate::db::db::{DatabaseSetupState, Db};
use crate::model::{CheckType, DatabaseItem, DatabaseResult, QueryAndParams, RowValues};
use function_name::named;
use serde::Deserialize;
// use sqlx::query;

/// we need this to deserialize the json, even though it seems trivial, it's needed for data validation
#[derive(Deserialize, Debug, Clone)]
pub struct MissingDbObjects {
    pub missing_object: String,
}

/// Check if tables or constraints are setup. Expects a particular query result format.
/// This format, from rusty-golf 0x_tables_exist.sql, expects this result format from they query:
///```text
///             tbl        | exists
///     -------------------+--------
///      eup_statistic     | f
///      event             | t
///      event_user_player | t
///      golfstatistic     | t
///      golf_user         | t
///      player            | t
///```
pub async fn test_is_db_setup(
    db: &Db,
    check_type: &CheckType,
    query: &str,
    ddl: &[DatabaseItem],
) -> Result<Vec<DatabaseResult<String>>, Box<dyn std::error::Error>> {
    let mut dbresults = vec![];

    // let query = include_str!("../admin/model/sql/schema/0x_tables_exist.sql");
    let query_and_params = QueryAndParams {
        query: query.to_string(),
        params: vec![],
    };
    let result = db.exec_general_query(vec![query_and_params], true).await;

    let missing_tables = match result {
        Ok(r) => {
            if r.db_last_exec_state == DatabaseSetupState::QueryReturnedSuccessfully {
                r.return_result[0].results.clone()
            } else {
                let mut dbresult: DatabaseResult<String> = DatabaseResult::<String>::default();
                dbresult.db_last_exec_state = r.db_last_exec_state;
                dbresult.error_message = r.error_message;
                return Ok(vec![dbresult]);
            }
        }
        Err(e) => {
            let emessage = format!("Failed in {}, {}: {}", std::file!(), std::line!(), e);
            let mut dbresult: DatabaseResult<String> = DatabaseResult::<String>::default();
            dbresult.error_message = Some(emessage);
            dbresults.push(dbresult);
            return Ok(dbresults);
        }
    };

    // may have to declare as Vec<String>
    let zz: Vec<_> = missing_tables
        .iter()
        .filter_map(|row| {
            let exists_index = row.column_names.iter().position(|col| col == "exists")?;
            let tbl_index = row.column_names.iter().position(|col| col == "tbl")?;

            // Check if the "exists" column value is `Value::Bool(true)` or `Value::Text("t")`
            match &row.rows[exists_index] {
                RowValues::Bool(true) => match &row.rows[tbl_index] {
                    RowValues::Text(tbl_name) => Some(tbl_name.clone()),
                    _ => None,
                },
                RowValues::Text(value) if value == "t" => match &row.rows[tbl_index] {
                    RowValues::Text(tbl_name) => Some(tbl_name.clone()),
                    _ => None,
                },
                _ => None,
            }
        })
        .collect();

    fn local_fn_get_iter<'a>(
        ddl: &'a [DatabaseItem],
        check_type: &'a CheckType,
    ) -> impl Iterator<Item = &'a str> {
        ddl.iter().filter_map(move |item| match (check_type, item) {
            (CheckType::Table, DatabaseItem::Table(table)) => Some(table.table_name.as_str()),
            (CheckType::Constraint, DatabaseItem::Constraint(constraint)) => {
                Some(constraint.constraint_name.as_str())
            }
            _ => None,
        })
    }

    for table in local_fn_get_iter(ddl, check_type) {
        let mut dbresult: DatabaseResult<String> = DatabaseResult::<String>::default();
        dbresult.db_object_name = table.to_string();

        if zz.iter().any(|x| x == table) {
            dbresult.db_last_exec_state = DatabaseSetupState::QueryReturnedSuccessfully;
        } else {
            dbresult.db_last_exec_state = DatabaseSetupState::MissingRelations;
        }

        dbresults.push(dbresult);
    }

    Ok(dbresults)
}

#[named]
pub async fn create_tables(
    db: &Db,
    tables: Vec<MissingDbObjects>,
    check_type: CheckType,
    ddl_for_validation: &[(&str, &str, &str, &str)],
) -> Result<DatabaseResult<String>, Box<dyn std::error::Error>> {
    let mut return_result: DatabaseResult<String> = DatabaseResult::<String>::default();
    return_result.db_object_name = function_name!().to_string();

    let entire_create_stms = if check_type == CheckType::Table {
        ddl_for_validation
            .iter()
            .filter(|x| tables.iter().any(|y| y.missing_object == x.0))
            .map(|af| af.1)
            // .into_iter()
            .collect::<Vec<&str>>()
        // .join("")
        // .flatten()
    } else {
        ddl_for_validation
            .iter()
            .filter(|x| tables.iter().any(|y| y.missing_object == x.2))
            .map(|af| af.3)
            // .collect::<Vec<&str>>()
            // .flatten()
            .collect::<Vec<&str>>()
        // .join("")
    };

    let result = db
        .exec_general_query(
            entire_create_stms
                .iter()
                .map(|x| QueryAndParams {
                    query: x.to_string(),
                    params: vec![],
                })
                .collect(),
            false,
        )
        .await;

    // let query_and_params = QueryAndParams {
    //     query: entire_create_stms,
    //     params: vec![],
    // };
    // let result = self.exec_general_query(vec![query_and_params], false).await;

    let mut dbresult: DatabaseResult<String> = DatabaseResult::<String>::default();

    match result {
        Ok(r) => {
            dbresult.db_last_exec_state = r.db_last_exec_state;
            dbresult.error_message = r.error_message;
            // r.return_result
        }
        Err(e) => {
            let emessage = format!("Failed in {}, {}: {}", std::file!(), std::line!(), e);
            dbresult.error_message = Some(emessage);
        }
    };
    Ok(dbresult)
}

#[cfg(test)]
mod tests {
    use std::{env, vec};

    use crate::db::db::{DatabaseType, DbConfigAndPool};
    use crate::model::CustomDbRow;
    use chrono::NaiveDateTime;
    use function_name::named;
    // use sqlx::query;
    use tokio::runtime::Runtime;

    use super::*;

    #[named]
    async fn create_tables(
        sself: &Db,
        tables: Vec<MissingDbObjects>,
        check_type: CheckType,
        ddl_for_validation: &[(&str, &str, &str, &str)],
    ) -> Result<DatabaseResult<String>, Box<dyn std::error::Error>> {
        let mut return_result: DatabaseResult<String> = DatabaseResult::<String>::default();
        return_result.db_object_name = function_name!().to_string();

        let entire_create_stms = if check_type == CheckType::Table {
            ddl_for_validation
                .iter()
                .filter(|x| tables.iter().any(|y| y.missing_object == x.0))
                .map(|af| af.1)
                // .into_iter()
                .collect::<Vec<&str>>()
            // .join("")
            // .flatten()
        } else {
            ddl_for_validation
                .iter()
                .filter(|x| tables.iter().any(|y| y.missing_object == x.2))
                .map(|af| af.3)
                // .collect::<Vec<&str>>()
                // .flatten()
                .collect::<Vec<&str>>()
            // .join("")
        };

        let result = sself
            .exec_general_query(
                entire_create_stms
                    .iter()
                    .map(|x| QueryAndParams {
                        query: x.to_string(),
                        params: vec![],
                    })
                    .collect(),
                false,
            )
            .await;

        // let query_and_params = QueryAndParams {
        //     query: entire_create_stms,
        //     params: vec![],
        // };
        // let result = self.exec_general_query(vec![query_and_params], false).await;

        let mut dbresult: DatabaseResult<String> = DatabaseResult::<String>::default();

        match result {
            Ok(r) => {
                dbresult.db_last_exec_state = r.db_last_exec_state;
                dbresult.error_message = r.error_message;
                // r.return_result
            }
            Err(e) => {
                let emessage = format!("Failed in {}, {}: {}", std::file!(), std::line!(), e);
                dbresult.error_message = Some(emessage);
            }
        };
        Ok(dbresult)
    }

    #[test]
    fn a_pg_create_and_update_tbls() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            // env::var("DB_USER") = Ok("postgres".to_string());

            const TABLE_DDL: &[(&str, &str, &str, &str)] = &[
                (
                    "test",
                    "CREATE TABLE IF NOT EXISTS -- drop table event cascade
                    test (
                    event_id BIGSERIAL NOT NULL PRIMARY KEY,
                    espn_id BIGINT NOT NULL,
                    name TEXT NOT NULL,
                    ins_ts TIMESTAMP NOT NULL DEFAULT now()
                    );",
                    "",
                    "",
                ),
                (
                    "test_2",
                    "CREATE TABLE IF NOT EXISTS -- drop table event cascade
                    test_2 (
                    event_id BIGSERIAL NOT NULL PRIMARY KEY,
                    espn_id BIGINT NOT NULL,
                    name TEXT NOT NULL,
                    ins_ts TIMESTAMP NOT NULL DEFAULT now()
                    );",
                    "",
                    "",
                ),
            ];

            dotenv::dotenv().unwrap();

            let mut db_pwd = env::var("DB_PASSWORD").unwrap();
            if db_pwd == "/secrets/db_password" {
                // open the file and read the contents
                let contents = std::fs::read_to_string("/secrets/db_password")
                    .unwrap_or("tempPasswordWillbeReplacedIn!AdminPanel".to_string());
                // set the password to the contents of the file
                db_pwd = contents.trim().to_string();
            }
            let mut cfg = deadpool_postgres::Config::new();
            cfg.dbname = Some(env::var("DB_NAME").unwrap());
            cfg.host = Some(env::var("DB_HOST").unwrap());
            cfg.port = Some(env::var("DB_PORT").unwrap().parse::<u16>().unwrap());
            cfg.user = Some(env::var("DB_USER").unwrap());
            cfg.password = Some(db_pwd);

            let postgres_dbconfig: DbConfigAndPool =
                DbConfigAndPool::new(cfg, DatabaseType::Postgres).await;
            let postgres_db = Db::new(postgres_dbconfig).unwrap();

            let pg_objs = vec![
                MissingDbObjects {
                    missing_object: "test".to_string(),
                },
                MissingDbObjects {
                    missing_object: "test_2".to_string(),
                },
            ];

            // create two test tables
            let pg_create_result =
                create_tables(&postgres_db, pg_objs, CheckType::Table, TABLE_DDL)
                    .await
                    .unwrap();

            if pg_create_result.db_last_exec_state == DatabaseSetupState::QueryError {
                eprintln!("Error: {}", pg_create_result.error_message.unwrap());
            }

            assert_eq!(
                pg_create_result.db_last_exec_state,
                DatabaseSetupState::QueryReturnedSuccessfully
            );
            assert_eq!(pg_create_result.return_result, String::default());

            const TABLE_DDL_SYNTAX_ERR: &[(&str, &str, &str, &str)] = &[(
                "testa",
                "CREATE TABLEXXXXXXXX IF NOT EXISTS -- drop table event cascade
                    test (
                    event_id BIGSERIAL NOT NULL PRIMARY KEY,
                    espn_id BIGINT NOT NULL,
                    name TEXT NOT NULL,
                    ins_ts TIMESTAMP NOT NULL DEFAULT now()
                    );",
                "",
                "",
            )];

            let pg_objs = vec![MissingDbObjects {
                missing_object: "testa".to_string(),
            }];

            // create a test table
            let pg_create_result = create_tables(
                &postgres_db,
                pg_objs,
                CheckType::Table,
                TABLE_DDL_SYNTAX_ERR,
            )
            .await
            .unwrap();

            assert_eq!(
                pg_create_result.db_last_exec_state,
                DatabaseSetupState::QueryError,
            );
            assert_eq!(pg_create_result.return_result, String::default());

            let query = "DELETE FROM test;";
            let params: Vec<RowValues> = vec![];
            let _ = postgres_db
                .exec_general_query(
                    vec![QueryAndParams {
                        query: query.to_string(),
                        params: params,
                    }],
                    false,
                )
                .await
                .unwrap();

            let query = "INSERT INTO test (espn_id, name, ins_ts) VALUES ($1, $2, $3)";
            let params: Vec<RowValues> = vec![
                RowValues::Int(123456),
                RowValues::Text("test name".to_string()),
                RowValues::Timestamp(
                    NaiveDateTime::parse_from_str("2021-08-06 16:00:00", "%Y-%m-%d %H:%M:%S")
                        .unwrap(),
                ),
            ];
            // params.push("test name".to_string());
            let x = postgres_db
                .exec_general_query(
                    vec![QueryAndParams {
                        query: query.to_string(),
                        params: params,
                    }],
                    false,
                )
                .await
                .unwrap();

            if x.db_last_exec_state == DatabaseSetupState::QueryError {
                eprintln!("Error: {}", x.error_message.unwrap());
            }

            assert_eq!(
                x.db_last_exec_state,
                DatabaseSetupState::QueryReturnedSuccessfully
            );

            let query = "SELECT * FROM test";
            let params: Vec<RowValues> = vec![];
            let result = postgres_db
                .exec_general_query(
                    vec![QueryAndParams {
                        query: query.to_string(),
                        params: params,
                    }],
                    true,
                )
                .await
                .unwrap();

            let expected_result = DatabaseResult {
                db_last_exec_state: DatabaseSetupState::QueryReturnedSuccessfully,
                return_result: vec![CustomDbRow {
                    column_names: vec![
                        "event_id".to_string(),
                        "espn_id".to_string(),
                        "name".to_string(),
                        "ins_ts".to_string(),
                    ],
                    rows: vec![
                        RowValues::Int(1),
                        RowValues::Int(123456),
                        RowValues::Text("test name".to_string()),
                        RowValues::Timestamp(
                            NaiveDateTime::parse_from_str(
                                "2021-08-06 16:00:00",
                                "%Y-%m-%d %H:%M:%S",
                            )
                            .unwrap(),
                        ),
                    ],
                }],
                error_message: None,
                db_object_name: "exec_general_query".to_string(),
            };

            assert_eq!(
                result.db_last_exec_state,
                expected_result.db_last_exec_state
            );
            assert_eq!(result.error_message, expected_result.error_message);

            let cols_to_actually_check = vec!["espn_id", "name", "ins_ts"];

            for (index, row) in result.return_result.iter().enumerate() {
                let left: Vec<RowValues> = row.results[0]
                    .column_names
                    .iter()
                    .zip(&row.results[0].rows) // Pair column names with corresponding row values
                    .filter(|(col_name, _)| cols_to_actually_check.contains(&col_name.as_str()))
                    .map(|(_, value)| value.clone()) // Collect the filtered row values
                    .collect();

                // Get column names and row values from the expected result
                let right: Vec<RowValues> = expected_result.return_result[index]
                    .column_names
                    .iter()
                    .zip(&expected_result.return_result[index].rows) // Pair column names with corresponding row values
                    .filter(|(col_name, _)| cols_to_actually_check.contains(&col_name.as_str()))
                    .map(|(_, value)| value.clone()) // Collect the filtered row values
                    .collect();

                assert_eq!(left, right);
            }

            let query = "DROP TABLE test;";
            let params: Vec<RowValues> = vec![];
            let result = postgres_db
                .exec_general_query(
                    vec![QueryAndParams {
                        query: query.to_string(),
                        params: params,
                    }],
                    false,
                )
                .await
                .unwrap();

            assert_eq!(
                result.db_last_exec_state,
                DatabaseSetupState::QueryReturnedSuccessfully
            );

            let query = "DROP TABLE test_2;";
            let params: Vec<RowValues> = vec![];
            let result = postgres_db
                .exec_general_query(
                    vec![QueryAndParams {
                        query: query.to_string(),
                        params: params,
                    }],
                    false,
                )
                .await
                .unwrap();

            assert_eq!(
                result.db_last_exec_state,
                DatabaseSetupState::QueryReturnedSuccessfully
            );
        });
    }
}
