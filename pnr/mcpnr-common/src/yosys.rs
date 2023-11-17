//! Yosys module

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::{CellExt, CellGetAttribError};

#[derive(Serialize, Deserialize, Clone)]
pub struct Design {
    pub creator: String,
    pub modules: HashMap<String, Module>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Module {
    pub attributes: HashMap<String, String>,
    pub parameter_default_values: Option<HashMap<String, String>>,
    pub ports: HashMap<String, Port>,
    pub cells: HashMap<String, Cell>,
    pub netnames: HashMap<String, NetName>,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(untagged)]
pub enum ConstOrSignal {
    Const(String),
    Signal(i64),
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Port {
    direction: PortDirection,
    bits: Vec<ConstOrSignal>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Cell {
    pub hide_name: usize,
    #[serde(rename = "type")]
    pub ty: String,
    pub parameters: HashMap<String, String>,
    pub attributes: HashMap<String, String>,
    /// Map from port name to direction
    pub port_directions: HashMap<String, PortDirection>,
    /// Map from port name to signal indexes
    pub connections: HashMap<String, Vec<i64>>,
}

#[derive(Serialize, Deserialize, Clone, Copy)]
pub enum PortDirection {
    #[serde(rename = "input")]
    Input,
    #[serde(rename = "output")]
    Output,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct NetName {
    pub hide_name: usize,
    pub bits: Vec<ConstOrSignal>,
    pub attributes: HashMap<String, String>,
}

impl CellExt for Cell {
    fn get_param_i64(&self, name: &str) -> Result<i64, crate::CellGetAttribError> {
        self.parameters
            .get(name)
            .ok_or_else(|| CellGetAttribError::AttributeMissing(name.to_owned()))
            .and_then(|v| { i64::from_str_radix(v, 2) }.map_err(CellGetAttribError::ParseFailed))
    }
}
