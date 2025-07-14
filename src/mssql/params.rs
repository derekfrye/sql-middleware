use std::borrow::Cow;
use tiberius::{ColumnData, ToSql};

use crate::middleware::{ConversionMode, ParamConverter, RowValues, SqlMiddlewareDbError};

/// Container for SQL Server parameters with lifetime tracking
pub struct Params<'a> {
    pub(crate) references: Vec<&'a (dyn ToSql + Sync)>,
}

impl<'a> Params<'a> {
    /// Convert from a slice of RowValues to SQL Server parameters
    pub fn convert(params: &'a [RowValues]) -> Result<Params<'a>, SqlMiddlewareDbError> {
        // Pre-allocate space for performance
        let mut references = Vec::with_capacity(params.len());

        // Avoid iterator.collect() allocation overhead
        for p in params {
            references.push(p as &(dyn ToSql + Sync));
        }

        Ok(Params { references })
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
        Self::convert(params)
    }

    // SQL Server params support both query and execution modes
    fn supports_mode(_mode: ConversionMode) -> bool {
        true
    }
}

/// ToSql for RowValues for passing parameters
impl ToSql for RowValues {
    fn to_sql(&self) -> tiberius::ColumnData<'_> {
        match self {
            RowValues::Int(i) => ColumnData::I64(Some(*i)),
            RowValues::Float(f) => ColumnData::F64(Some(*f)),
            RowValues::Text(s) => ColumnData::String(Some(Cow::from(s.as_str()))),
            RowValues::Bool(b) => ColumnData::Bit(Some(*b)),
            RowValues::Timestamp(dt) => {
                // Use thread_local storage for efficient timestamp formatting
                thread_local! {
                    static BUF: std::cell::RefCell<String> = std::cell::RefCell::new(String::with_capacity(32));
                }

                // Format the timestamp efficiently
                BUF.with(|buf| {
                    let mut s = buf.borrow_mut();
                    s.clear();
                    use std::fmt::Write;
                    // ISO-8601 format
                    write!(s, "{}", dt.format("%Y-%m-%dT%H:%M:%S%.f")).unwrap();
                    ColumnData::String(Some(Cow::from(s.clone())))
                })
            }
            RowValues::Null => ColumnData::String(None),
            RowValues::JSON(jsval) => ColumnData::String(Some(Cow::from(jsval.to_string()))),
            RowValues::Blob(bytes) => ColumnData::Binary(Some(Cow::from(bytes.as_slice()))),
        }
    }
}
