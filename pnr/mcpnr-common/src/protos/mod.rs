use std::result::Result;

use crate::{CellExt, CellGetAttribError};

include!(concat!(env!("OUT_DIR"), "/protos.rs"));

impl CellExt for mcpnr::placed_design::Cell {
    fn get_param_i64(&self, name: &str) -> Result<i64, CellGetAttribError> {
        use mcpnr::parameter::Value;
        let value = self
            .parameter
            .get(name)
            .and_then(|v| v.value.as_ref())
            .ok_or_else(|| CellGetAttribError::AttributeMissing(name.into()))?;
        match value {
            Value::Int(ref i) => Ok(*i),
            Value::Str(ref s) => s.parse::<i64>().map_err(CellGetAttribError::ParseFailed),
        }
    }
}
