use std::{
    fmt::{Display, Formatter},
    result::Result,
};

include!(concat!(env!("OUT_DIR"), "/protos.rs"));

#[derive(Debug)]
pub enum CellGetAttribError {
    AttributeMissing,
    ParseFailed(<i64 as std::str::FromStr>::Err),
}

impl Display for CellGetAttribError {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        match self {
            Self::AttributeMissing => f.write_str("CellGetAttribError::AttributeMissing"),
            Self::ParseFailed(ref p) => write!(f, "CellGetAttribError::ParseFailed({})", p),
        }
    }
}

impl std::error::Error for CellGetAttribError {}

pub trait CellExt {
    /// Get a numerical attribute from the cell, parsing it if it's a string.
    fn get_attrib_i64(&self, name: &str) -> Result<i64, CellGetAttribError>;

    fn get_attrib_i64_with_default(
        &self,
        name: &str,
        default: i64,
    ) -> Result<i64, CellGetAttribError> {
        self.get_attrib_i64(name).or_else(|e| match e {
            CellGetAttribError::AttributeMissing => Ok(default),
            _ => Err(e),
        })
    }
}

impl CellExt for yosys::pb::module::Cell {
    fn get_attrib_i64(&self, name: &str) -> Result<i64, CellGetAttribError> {
        let value = self
            .parameter
            .get(name)
            .and_then(|v| v.value.as_ref())
            .ok_or(CellGetAttribError::AttributeMissing)?;
        match value {
            yosys::pb::parameter::Value::Int(ref i) => Ok(*i),
            yosys::pb::parameter::Value::Str(ref s) => {
                s.parse::<i64>().map_err(CellGetAttribError::ParseFailed)
            }
        }
    }
}

impl CellExt for mcpnr::placed_design::Cell {
    fn get_attrib_i64(&self, name: &str) -> Result<i64, CellGetAttribError> {
        let value = self
            .parameter
            .get(name)
            .and_then(|v| v.value.as_ref())
            .ok_or(CellGetAttribError::AttributeMissing)?;
        match value {
            yosys::pb::parameter::Value::Int(ref i) => Ok(*i),
            yosys::pb::parameter::Value::Str(ref s) => {
                s.parse::<i64>().map_err(CellGetAttribError::ParseFailed)
            }
        }
    }
}
