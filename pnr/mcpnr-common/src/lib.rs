pub mod block_storage;
pub mod minecraft_types;
pub mod protos;
pub mod yosys;

pub use prost;

use std::fmt::{Display, Formatter};

/// Number of blocks per row along the Z axis. This is chosen to be larger than the maximum
/// Z size of any cell
pub const BLOCKS_PER_Z_ROW: u32 = 6;

/// Number of blocks per tier. Each tier is composed of a layer of cells and 4 "metal" layers, used
/// for routing. The cell layer is 8 blocks high, and each metal layer is 2 blocks high
pub const BLOCKS_PER_TIER: u32 = 16;

/// Error generated when cell attribute retrieval fails
#[derive(Debug)]
pub enum CellGetAttribError {
    AttributeMissing(String),
    ParseFailed(<i64 as std::str::FromStr>::Err),
}

impl Display for CellGetAttribError {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        match self {
            Self::AttributeMissing(s) => write!(f, "CellGetAttribError::AttributeMissing({:?})", s),
            Self::ParseFailed(ref p) => write!(f, "CellGetAttribError::ParseFailed({})", p),
        }
    }
}

impl std::error::Error for CellGetAttribError {}

/// Handy abstraction for grabbing attributes out of cells
pub trait CellExt {
    /// Get a numerical attribute from the cell, parsing it if it's a string.
    fn get_param_i64(&self, name: &str) -> Result<i64, CellGetAttribError>;

    fn get_param_i64_with_default(
        &self,
        name: &str,
        default: i64,
    ) -> Result<i64, CellGetAttribError> {
        self.get_param_i64(name).or_else(|e| match e {
            CellGetAttribError::AttributeMissing(_) => Ok(default),
            _ => Err(e),
        })
    }
}
