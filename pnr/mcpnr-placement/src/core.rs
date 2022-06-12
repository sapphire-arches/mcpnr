use std::collections::HashMap;

use anyhow::{anyhow, Context, Result};
use mcpnr_common::protos::{
    mcpnr::{placed_design::Cell, PlacedDesign, Position},
    yosys::pb::{module::Netname, BitVector, Module, Parameter},
};

use crate::placement_cell::{CellFactory, PlacementCell};

pub struct CellMetadata {
    pub attributes: HashMap<String, Parameter>,
    pub connection: HashMap<String, BitVector>,
    pub parameter: HashMap<String, Parameter>,
    pub ty: String,
}

impl CellMetadata {}

pub struct PlaceableCells {
    /// The placer-internal metadata. Order must remain stable so we can zip it up with `metadata`
    /// later.
    pub cells: Vec<PlacementCell>,
    pub metadata: Vec<CellMetadata>,
    pub net_names: Vec<Netname>,
}

impl PlaceableCells {
    pub fn from_module(m: Module, cell_factory: &mut CellFactory) -> Result<Self> {
        let mut cells = Vec::with_capacity(m.cell.len());
        let mut metadata = Vec::with_capacity(m.cell.len());

        for (key, cell) in m.cell {
            cells.push(
                cell_factory
                    .build_cell(&cell)
                    .with_context(|| anyhow!("Pushing cell {:?}", key))?,
            );
            metadata.push(CellMetadata {
                attributes: cell.attribute,
                connection: cell.connection,
                parameter: cell.parameter,
                ty: cell.r#type,
            })
        }

        Ok(Self {
            cells,
            metadata,
            net_names: m.netname,
        })
    }

    pub fn build_output(self, creator: String) -> PlacedDesign {
        PlacedDesign {
            creator: format!(
                "Placed by MCPNR {}, Synth: {}",
                env!("CARGO_PKG_VERSION"),
                creator,
            ),
            nets: self.net_names,
            cells: self
                .cells
                .into_iter()
                .zip(self.metadata.into_iter())
                .map(|(cell, meta)| {
                    let pos = cell.unexpanded_pos();
                    Cell {
                        pos: Some(Position {
                            x: pos[0],
                            y: pos[1],
                            z: pos[2],
                        }),
                        r#type: meta.ty,
                        parameter: meta.parameter,
                        attribute: meta.attributes,
                        connection: meta.connection,
                    }
                })
                .collect(),
        }
    }
}
