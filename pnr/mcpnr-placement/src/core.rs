use std::collections::HashMap;

use anyhow::{anyhow, Context, Result};
use mcpnr_common::{
    protos::mcpnr::{
        parameter::Value, placed_design::Cell, BitVector, NetMetadata, Parameter,
        PlacedDesign, Position,
    },
    yosys::{ConstOrSignal, Module},
};

use crate::placement_cell::{CellFactory, LegalizedCell, PlacementCell};

pub struct CellMetadata {
    /// Map from attribute name to value
    pub attributes: HashMap<String, Parameter>,
    /// Map from port name to direction
    pub connection: HashMap<String, BitVector>,
    /// Map from parameter name to parameter value
    pub parameter: HashMap<String, Parameter>,
    /// Type of this cell (either a built-in magic cell, or the name of an NBT file)
    pub ty: String,
}

#[derive(Debug)]
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
    /// later. This vector is ordered such that the first [`NetlistHypergraph::mobile_cell_count`]
    /// cells are the mobile cells.
    pub cells: Vec<PlacementCell>,
    pub metadata: Vec<CellMetadata>,

    pub mobile_cell_count: usize,

    pub signals: Vec<Signal>,
    pub net_names: HashMap<String, NetMetadata>,
}

impl NetlistHypergraph {
    /// Create a hypergraph with the given cell and signal information. This is mostly useful for
    /// testing purposes. `cells` is assumed to be ordered as described by the documentation of
    /// [`NetlistHypergraph::cells`].
    pub fn test_new(
        cells: Vec<PlacementCell>,
        mobile_cell_count: usize,
        signals: Vec<Signal>,
    ) -> Self {
        Self {
            cells,
            metadata: vec![],
            mobile_cell_count,
            signals,
            net_names: Default::default(),
        }
    }

    /// Construct a placement cell from a Yosys module
    pub fn from_module(m: Module, cell_factory: &mut CellFactory) -> Result<Self> {
        let mut cells = Vec::with_capacity(m.cells.len());
        let mut metadata = Vec::with_capacity(m.cells.len());
        let mut signals: HashMap<u64, Vec<usize>> = HashMap::new();

        // For each cell in the module,
        for (key, cell) in m.cells {
            let cell_idx = cells.len();
            cells.push(
                cell_factory
                    .build_cell(&cell)
                    .with_context(|| anyhow!("Pushing cell {:?}", key))?,
            );

            for (_, bits) in &cell.connections {
                for signal in bits.iter() {
                    match signal {
                        ConstOrSignal::Const(_c) => {
                            // log::warn!("Connection to a constant wire {c}")
                        }
                        ConstOrSignal::Signal(s) => signals
                            .entry(*s as u64)
                            .or_insert_with(|| Vec::new())
                            .push(cell_idx),
                    }
                }
            }

            metadata.push(CellMetadata {
                attributes: cell
                    .attributes
                    .into_iter()
                    .map(|(k, v)| {
                        (
                            k,
                            Parameter {
                                value: Some(Value::Str(v)),
                            },
                        )
                    })
                    .collect(),
                connection: cell
                    .connections
                    .into_iter()
                    .map(|(k, v)| {
                        let v = BitVector {
                            signal: v
                                .into_iter()
                                .map(|s| mcpnr_common::protos::mcpnr::Signal {
                                    r#type: Some(s.to_type()),
                                })
                                .collect(),
                        };
                        (k, v)
                    })
                    .collect(),
                parameter: cell
                    .parameters
                    .into_iter()
                    .map(|(k, v)| {
                        let v = Parameter {
                            value: Some(Value::Str(v)),
                        };
                        (k, v)
                    })
                    .collect(),
                ty: cell.ty,
            })
        }

        let mut signals: Vec<_> = signals
            .into_iter()
            .map(|(_, v)| Signal {
                moveable_cells: v.iter().filter(|idx| !cells[**idx].pos_locked).count(),
                connected_cells: v,
            })
            .collect();

        // Swap all position locked cells to the end of the cell list.
        let mut mobile_cell_count = 0;
        let mut next_mobile_index = cells.len() - 1;
        while cells[next_mobile_index].pos_locked {
            next_mobile_index -= 1;
        }
        for i in 0..cells.len() {
            if i >= next_mobile_index {
                // When the forward iteration reaches the next mobile index, we know everything
                // past the next_mobile_index is pos locked and can break
                break;
            }
            if cells[i].pos_locked {
                // This cell is locked early, swap the cell itself, its metadata, and rewrite all
                // signals that reference it
                cells.swap(i, next_mobile_index);
                metadata.swap(i, next_mobile_index);

                for signal in signals.iter_mut() {
                    for cell_idx in signal.connected_cells.iter_mut() {
                        if *cell_idx == i {
                            *cell_idx = next_mobile_index;
                        } else if *cell_idx == next_mobile_index {
                            *cell_idx = i;
                        }
                    }
                }

                // Find the next mobile cell
                while cells[next_mobile_index].pos_locked {
                    next_mobile_index -= 1;
                }
            } else {
                mobile_cell_count += 1;
            }
        }

        // Cleanup: Skip to the end of the mobile cell block
        while !cells[mobile_cell_count].pos_locked {
            mobile_cell_count += 1;
        }

        assert!(cells[0..mobile_cell_count]
            .iter()
            .all(|cell| !cell.pos_locked));
        assert!(cells[mobile_cell_count..]
            .iter()
            .all(|cell| cell.pos_locked));

        Ok(Self {
            cells,
            metadata,
            mobile_cell_count, // Need to implement sort
            signals,
            net_names: m
                .netnames
                .into_iter()
                .map(|(k, v)| {
                    let v = NetMetadata {
                        hide_name: v.hide_name != 0,
                        bits: Some(BitVector {
                            signal: v
                                .bits
                                .into_iter()
                                .map(|b| mcpnr_common::protos::mcpnr::Signal {
                                    r#type: Some(b.to_type()),
                                })
                                .collect(),
                        }),
                        attributes: v
                            .attributes
                            .into_iter()
                            .map(|(k, v)| {
                                let v = Parameter {
                                    value: Some(Value::Str(v)),
                                };
                                (k, v)
                            })
                            .collect(),
                    };
                    (k, v)
                })
                .collect(),
        })
    }

    pub fn build_output(
        self,
        legalized_cells: Vec<LegalizedCell>,
        creator: String,
    ) -> PlacedDesign {
        PlacedDesign {
            creator: format!(
                "Placed by MCPNR {}, Synth: {}",
                env!("CARGO_PKG_VERSION"),
                creator,
            ),
            nets: self.net_names,
            cells: legalized_cells
                .into_iter()
                .zip(self.metadata.into_iter())
                .map(|(cell, meta)| Cell {
                    pos: Some(Position {
                        x: cell.x,
                        y: cell.y,
                        z: cell.z,
                    }),
                    r#type: meta.ty,
                    parameter: meta.parameter,
                    attribute: meta.attributes,
                    connection: meta.connection,
                })
                .collect(),
        }
    }
}
