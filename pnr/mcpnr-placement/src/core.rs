use std::{collections::HashMap, iter::FromIterator};

use anyhow::{anyhow, Context, Result};
use mcpnr_common::protos::{
    mcpnr::{placed_design::Cell, PlacedDesign, Position},
    yosys::pb::{module::Netname, BitVector, Module, Parameter},
};
use nalgebra::Vector3;

use crate::placement_cell::{CellFactory, PlacementCell};

pub struct CellMetadata {
    pub attributes: HashMap<String, Parameter>,
    pub connection: HashMap<String, BitVector>,
    pub parameter: HashMap<String, Parameter>,
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
            .filter(|idx| **idx < net.mobile_cell_count)
            .map(|x| *x)
    }
}

/// Structure-of-Arrays container for placeable cells
pub struct CellData {
    /// Minimum X coordinate of the cell box
    pub x: Vec<f32>,
    /// Minimum Y coordinate of the cell box
    pub y: Vec<f32>,
    /// Minimum Z coordinate of the cell box
    pub z: Vec<f32>,
    /// X size of the cell box
    pub sx: Vec<f32>,
    /// Y size of the cell box
    pub sy: Vec<f32>,
    /// Z size of the cell box
    pub sz: Vec<f32>,
}

impl CellData {
    /// Allocate a cell data arena with the given capacity
    pub fn with_capacity(cap: usize) -> Self {
        Self {
            x: Vec::with_capacity(cap),
            y: Vec::with_capacity(cap),
            z: Vec::with_capacity(cap),
            sx: Vec::with_capacity(cap),
            sy: Vec::with_capacity(cap),
            sz: Vec::with_capacity(cap),
        }
    }

    /// Convert a placement cell into the SoA representation
    pub fn push(&mut self, cell: PlacementCell) {
        self.x.push(cell.x);
        self.y.push(cell.y);
        self.z.push(cell.z);
        self.sx.push(cell.sx);
        self.sy.push(cell.sy);
        self.sz.push(cell.sz);
    }

    /// Number of cells in this arena
    pub fn len(&self) -> usize {
        self.x.len()
    }

    /// Swap the data for two cells
    pub fn swap(&mut self, i: usize, j: usize) {
        self.x.swap(i, j);
        self.y.swap(i, j);
        self.z.swap(i, j);
        self.sx.swap(i, j);
        self.sy.swap(i, j);
        self.sz.swap(i, j);
    }

    /// Compute the center position of a given cell
    pub fn center_pos(&self, i: usize) -> Vector3<f32> {
        Vector3::new(
            self.x[i] + self.sx[i] / 2.0,
            self.y[i] + self.sy[i] / 2.0,
            self.z[i] + self.sz[i] / 2.0,
        )
    }
}

impl FromIterator<PlacementCell> for CellData {
    fn from_iter<T: IntoIterator<Item = PlacementCell>>(iter: T) -> Self {
        let iter = iter.into_iter();
        let mut s = Self::with_capacity(iter.size_hint().1.unwrap_or(16));
        for cell in iter {
            s.push(cell);
        }

        s
    }
}

/// Represents the netlist as a hypergraph. [`NetlistHypergraph::cells`] are the nodes,
/// [`NetlistHypergraph::signals`] are the edges. Each [`Signal`] contains the list of cells it is
/// connected to, as an index into [`NetlistHypergraph::cells`].
pub struct NetlistHypergraph {
    /// The placer-internal metadata. Order must remain stable so we can zip it up with `metadata`
    /// later. This vector is ordered such that the first [`NetlistHypergraph::mobile_cell_count`]
    /// cells are the mobile cells.
    pub cells: CellData,
    pub metadata: Vec<CellMetadata>,

    pub mobile_cell_count: usize,

    pub signals: Vec<Signal>,
    pub net_names: Vec<Netname>,
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
        let soa_cells = CellData::from_iter(cells);

        Self {
            cells: soa_cells,
            metadata: vec![],
            mobile_cell_count,
            signals,
            net_names: vec![],
        }
    }

    pub fn from_module(m: Module, cell_factory: &mut CellFactory) -> Result<Self> {
        let mut cells = CellData::with_capacity(m.cell.len());
        let mut locks = Vec::with_capacity(m.cell.len());
        let mut metadata = Vec::with_capacity(m.cell.len());
        let mut signals: HashMap<u64, Vec<usize>> = HashMap::new();

        for (key, cell) in m.cell {
            let cell_idx = cells.len();
            let place_cell = cell_factory
                .build_cell(&cell)
                .with_context(|| anyhow!("Pushing cell {:?}", key))?;
            locks.push(place_cell.pos_locked);
            cells.push(place_cell);

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

        let mut signals: Vec<_> = signals
            .into_iter()
            .map(|(_, v)| Signal {
                moveable_cells: v.iter().filter(|idx| !locks[**idx]).count(),
                connected_cells: v,
            })
            .collect();

        // Swap all position locked cells to the end of the cell list.
        let mut mobile_cell_count = 0;
        let mut next_mobile_index = cells.len() - 1;
        while locks[next_mobile_index] {
            next_mobile_index -= 1;
        }
        for i in 0..cells.len() {
            if i >= next_mobile_index {
                // When the forward iteration reaches the next mobile index, we know everything
                // past the next_mobile_index is pos locked and can break
                break;
            }
            if locks[i] {
                // This cell is locked early, swap the cell itself, its metadata, and rewrite all
                // signals that reference it
                cells.swap(i, next_mobile_index);
                locks.swap(i, next_mobile_index);
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
                while locks[next_mobile_index] {
                    next_mobile_index -= 1;
                }
            } else {
                mobile_cell_count += 1;
            }
        }

        // Cleanup: Skip to the end of the mobile cell block
        while !locks[mobile_cell_count] {
            mobile_cell_count += 1;
        }

        assert!(locks[0..mobile_cell_count].iter().all(|lock| !lock));
        assert!(locks[mobile_cell_count..].iter().all(|lock| *lock));

        Ok(Self {
            cells,
            metadata,
            mobile_cell_count, // Need to implement sort
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
                .metadata
                .into_iter()
                .enumerate()
                .map(|(cell, meta)| {
                    let pos = [
                        self.cells.x[cell] as u32,
                        self.cells.y[cell] as u32,
                        self.cells.z[cell] as u32,
                    ];
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

    /// Returns true if the given cell is locked (can't be moved)
    pub fn is_locked(&self, i: usize) -> bool {
        i >= self.mobile_cell_count
    }
}
