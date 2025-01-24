use crate::db::{Db, QueryState};
// use crate::db2::{DatabaseResult as DatabaseResult2, Db as Db2, QueryAndParams as QueryAndParams2};
use crate::middleware::{
    ConfigAndPool, DatabaseResult as DatabaseResult3, DbError, MiddlewarePool,
    QueryState as QueryState3, CheckType as CheckType2,
};
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
            if r.db_last_exec_state == QueryState::QueryReturnedSuccessfully {
                r.return_result[0].results.clone()
            } else {
                let mut dbresult: DatabaseResult<String> = DatabaseResult::<String>::default();
                dbresult.db_last_exec_state = r.db_last_exec_state;
                dbresult.error_message = r.error_message;
                return Ok(vec![dbresult]);
            }
        }
        Err(e) => {
            let emessage = format!("Failed in {}, {}: {:?}", std::file!(), std::line!(), e);
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
            dbresult.db_last_exec_state = QueryState::QueryReturnedSuccessfully;
        } else {
            dbresult.db_last_exec_state = QueryState::MissingRelations;
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
            let emessage = format!("Failed in {}, {}: {:?}", std::file!(), std::line!(), e);
            dbresult.error_message = Some(emessage);
        }
    }
    Ok(dbresult)
}

#[named]
pub async fn create_tables3(
    conf: &ConfigAndPool,
    tables: Vec<MissingDbObjects>,
    check_type: CheckType2,
    ddl_for_validation: &[(&str, &str, &str, &str)],
) -> Result<DatabaseResult3<String>, DbError> {
    let mut return_result: DatabaseResult3<String> = DatabaseResult3::<String>::default();
    return_result.db_object_name = function_name!().to_string();

    let entire_create_stms = if check_type == CheckType2::Table {
        ddl_for_validation
            .iter()
            .filter(|x| tables.iter().any(|y| y.missing_object == x.0))
            .map(|af| af.1.to_string())
            // .into_iter()
            .collect::<Vec<String>>()
        // .join("")
        // .flatten()
    } else {
        ddl_for_validation
            .iter()
            .filter(|x| tables.iter().any(|y| y.missing_object == x.2))
            .map(|af| af.3.to_string())
            // .collect::<Vec<&str>>()
            // .flatten()
            .collect::<Vec<String>>()
    };

    // let connection = conf.pool.get().await.map_err(DbError::PoolError)?;
    // let x = conf.pool;
    let _result = match &conf.pool {
        MiddlewarePool::Sqlite(zz) => {
            let tdfda = zz.get().await.map_err(DbError::PoolErrorSqlite)?;
            // let conn = connection;

            let _ = tdfda
                .interact(move |conn| {
                    let query_pragmas = "
                    PRAGMA journal_mode = WAL;
                    PRAGMA foreign_keys = ON;
                    PRAGMA synchronous = NORMAL;
                ";
                    let all_queries = format!("{} {}", query_pragmas, entire_create_stms.join(";"));
                    conn.execute_batch(&all_queries)
                        .map_err(|e| DbError::SqliteError(e))
                })
                .await?;
        }
        MiddlewarePool::Postgres(_) => {}
    };

    // let result = db
    //     .exec_general_query(
    //         entire_create_stms
    //             .iter()
    //             .map(|x| QueryAndParams3 {
    //                 query: x.to_string(),
    //                 params: vec![],
    //                 is_read_only: false,
    //             })
    //             .collect(),
    //         false,
    //     )
    //     .await;

    // let query_and_params = QueryAndParams {
    //     query: entire_create_stms,
    //     params: vec![],
    // };
    // let result = self.exec_general_query(vec![query_and_params], false).await;

    Ok(DatabaseResult3::<String> {
        db_last_exec_state: QueryState3::QueryReturnedSuccessfully,
        error_message: None,
        return_result: "".to_string(),
        db_object_name: function_name!().to_string(),
    })
}
