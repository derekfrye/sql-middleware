use crate::db::DatabaseSetupState;
use chrono::NaiveDateTime;
use serde_json::Value;

// Custom Value enum to support multiple data types
#[derive(Debug, Clone, PartialEq)]
pub enum RowValues {
    Int(i64),
    Float(f64),
    Text(String),
    Bool(bool),
    Timestamp(NaiveDateTime),
    Null,
    JSON(Value),
    Blob(Vec<u8>),
}

#[derive(Debug, Clone)]
pub struct CustomDbRow {
    pub column_names: Vec<String>,
    pub rows: Vec<RowValues>,
}

#[derive(Debug, Clone)]
pub struct ResultSet {
    pub results: Vec<CustomDbRow>,
}

pub struct QueryAndParams {
    pub query: String,
    pub params: Vec<RowValues>,
}

#[derive(Debug, Clone)]
pub struct DatabaseResult<T: Default> {
    pub db_last_exec_state: DatabaseSetupState,
    pub return_result: T,
    pub error_message: Option<String>,
    pub db_object_name: String,
}

impl RowValues {
    pub fn as_int(&self) -> Option<&i64> {
        if let RowValues::Int(value) = self {
            Some(value)
        } else {
            None
        }
    }

    pub fn as_text(&self) -> Option<&str> {
        if let RowValues::Text(value) = self {
            Some(value)
        } else {
            None
        }
    }

    pub fn as_bool(&self) -> Option<&bool> {
        if let RowValues::Bool(value) = self {
            Some(value)
        } else {
            None
        }
    }

    pub fn as_timestamp(&self) -> Option<&NaiveDateTime> {
        if let RowValues::Timestamp(value) = self {
            Some(value)
        } else {
            None
        }
    }

    pub fn as_json(&self) -> Option<&Value> {
        if let RowValues::JSON(value) = self {
            Some(value)
        } else {
            None
        }
    }

    pub fn as_blob(&self) -> Option<&Vec<u8>> {
        if let RowValues::Blob(value) = self {
            Some(value)
        } else {
            None
        }
    }

    pub fn as_float(&self) -> Option<f64> {
        if let RowValues::Float(value) = self {
            Some(*value as f64)
        } else {
            None
        }
    }

    pub fn is_null(&self) -> bool {
        matches!(self, RowValues::Null)
    }
}

impl CustomDbRow {
    pub fn get(&self, col_name: &str) -> Option<&RowValues> {
        // Find the index of the column name
        if let Some(index) = self.column_names.iter().position(|name| name == col_name) {
            // Get the corresponding row value by index
            self.rows.get(index)
        } else {
            None // Column name not found
        }
    }
}

impl<T: Default> DatabaseResult<T> {
    pub fn default() -> DatabaseResult<T> {
        DatabaseResult {
            db_last_exec_state: DatabaseSetupState::NoConnection,
            return_result: Default::default(),
            error_message: None,
            db_object_name: "".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum CheckType {
    Table,
    Constraint,
}

#[derive(Debug, Clone)]
pub struct DatabaseTable {
    pub table_name: String,
    pub ddl: String,
}

#[derive(Debug, Clone)]
pub struct DatabaseConstraint {
    pub table_name: String,
    pub constraint_name: String,
    pub constraint_type: String,
    pub ddl: String,
}

#[derive(Debug, Clone)]
pub enum DatabaseItem {
    Table(DatabaseTable),
    Constraint(DatabaseConstraint),
}
