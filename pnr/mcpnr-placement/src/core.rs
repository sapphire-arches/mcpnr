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

pub struct Signal {
    /// Vector of indicies into `PlaceableCells::cells`
    pub connected_cells: Vec<usize>,

    /// Number of cells in [`Signal::connected_cells`] that are moveable.
    pub moveable_cells: usize,
}

impl Signal {
    pub fn iter_mobile<'a>(
        &'a self,
        net: &'a NetlistHypergraph,
    ) -> impl Iterator<Item = usize> + 'a {
        self.connected_cells
            .iter()
            .filter(|idx| !net.cells[**idx].pos_locked)
            .map(|x| *x)
    }
}

/// Represents the netlist as a hypergraph. [`NetlistHypergraph::cells`] are the nodes,
/// [`NetlistHypergraph::signals`] are the edges. Each [`Signal`] contains the list of cells it is
/// connected to, as an index into [`NetlistHypergraph::cells`].
pub struct NetlistHypergraph {
    /// The placer-internal metadata. Order must remain stable so we can zip it up with `metadata`
    /// later.
    pub cells: Vec<PlacementCell>,
    pub metadata: Vec<CellMetadata>,

    pub signals: Vec<Signal>,
    pub net_names: Vec<Netname>,
}

impl NetlistHypergraph {
    /// Create a hypergraph with the given cell and signal information. This is mostly useful for
    /// testing purposes.
    pub fn test_new(cells: Vec<PlacementCell>, signals: Vec<Signal>) -> Self {
        Self {
            cells,
            metadata: vec![],
            signals,
            net_names: vec![],
        }
    }

    pub fn from_module(m: Module, cell_factory: &mut CellFactory) -> Result<Self> {
        let mut cells = Vec::with_capacity(m.cell.len());
        let mut metadata = Vec::with_capacity(m.cell.len());
        let mut signals: HashMap<u64, Vec<usize>> = HashMap::new();

        for (key, cell) in m.cell {
            let cell_idx = cells.len();
            cells.push(
                cell_factory
                    .build_cell(&cell)
                    .with_context(|| anyhow!("Pushing cell {:?}", key))?,
            );

            for (_, bits) in &cell.connection {
                for signal in &bits.signal {
                    use mcpnr_common::protos::yosys::pb::signal::Type;
                    match signal.r#type {
                        Some(Type::Id(i)) => signals
                            .entry(i as u64)
                            .or_insert_with(|| Vec::new())
                            .push(cell_idx),
                        _ => {}
                    }
                }
            }

            metadata.push(CellMetadata {
                attributes: cell.attribute,
                connection: cell.connection,
                parameter: cell.parameter,
                ty: cell.r#type,
            })
        }

        let signals = signals
            .into_iter()
            .map(|(_, v)| Signal {
                moveable_cells: v.iter().filter(|idx| !cells[**idx].pos_locked).count(),
                connected_cells: v,
            })
            .collect();

        Ok(Self {
            cells,
            metadata,
            signals,
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
