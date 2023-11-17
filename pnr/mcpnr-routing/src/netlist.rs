use std::collections::HashMap;

use anyhow::{anyhow, ensure, Context, Result};
use itertools::Itertools;
use mcpnr_common::protos::mcpnr::{PlacedDesign, signal::Type};

use crate::structure_cache::StructureCache;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum PinDirection {
    Input,
    Output,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PinMetadata {
    pub offset_x: u32,
    pub offset_y: u32,
    pub offset_z: u32,
    pub sig_derating: u32,
    pub direction: PinDirection,
}

#[derive(Debug)]
pub struct Pin {
    pub x: u32,
    pub y: u32,
    pub z: u32,
    pub direction: PinDirection,
}

#[derive(Default, Debug)]
pub struct Net {
    drivers: Vec<u32>,
    sinks: Vec<u32>,
}

pub struct Netlist {
    pins: Vec<Pin>,
    nets: HashMap<i64, Net>,
}

impl Netlist {
    pub fn new(design: &PlacedDesign, structure_cache: &StructureCache) -> Result<Self> {
        let mut pins = Vec::with_capacity(design.cells.len() * 2);
        let mut design_nets: HashMap<i64, Net> = HashMap::default();

        for cell in design.cells.iter() {
            let (base_x, base_y, base_z) = cell
                .pos
                .as_ref()
                .map(|p| (p.x, p.y, p.z))
                .unwrap_or((0, 0, 0));
            for (port, cell_nets) in cell.connection.iter() {
                for (bit_idx, net) in cell_nets.signal.iter().enumerate() {
                    let pin_metadata = pin_metadata(structure_cache, &cell.r#type, &port, bit_idx)
                        .with_context(|| {
                            anyhow!(
                                "Error while getting pin metadata for pin {}[{}] (cell {:?})",
                                port,
                                bit_idx,
                                cell,
                            )
                        })?;
                    let net_idx = match net.r#type {
                        Some(Type::Id(x)) => x,
                        // TODO: plumb through cell names so we can report better errors here and elsewhere
                        _ => {
                            return Err(anyhow!(
                            "Unsupported net index type {:?} processing pin {}[{}] (cell: {:?})",
                            net.r#type,
                            port,
                            bit_idx,
                            cell
                        ))
                        }
                    };

                    let pin_idx = pins
                        .len()
                        .try_into()
                        .context("Pin count exceeds u32::MAX")?;
                    pins.push(Pin {
                        x: base_x + pin_metadata.offset_x,
                        y: base_y + pin_metadata.offset_y,
                        z: base_z + pin_metadata.offset_z,
                        direction: pin_metadata.direction,
                    });
                    let net = design_nets.entry(net_idx).or_default();

                    match pin_metadata.direction {
                        PinDirection::Input => net.sinks.push(pin_idx),
                        PinDirection::Output => net.drivers.push(pin_idx),
                    }
                }
            }
        }

        for net in design_nets.values_mut() {
            net.drivers.sort();
            net.sinks.sort();
        }

        pins.shrink_to_fit();
        Ok(Netlist {
            pins,
            nets: design_nets,
        })
    }

    pub fn iter_pins(&self) -> impl Iterator<Item = &Pin> {
        self.pins.iter()
    }

    pub fn iter_nets(&self) -> impl Iterator<Item = (&i64, &Net)> {
        self.nets.iter().sorted_by_key(|f| f.0)
    }
}

impl Net {
    pub fn iter_drivers<'netlist>(
        &'netlist self,
        parent: &'netlist Netlist,
    ) -> impl Iterator<Item = &'netlist Pin> {
        self.drivers.iter().map(|idx| &parent.pins[*idx as usize])
    }

    pub fn iter_sinks<'netlist>(
        &'netlist self,
        parent: &'netlist Netlist,
    ) -> impl Iterator<Item = &'netlist Pin> {
        self.sinks.iter().map(|idx| &parent.pins[*idx as usize])
    }
}

fn pin_metadata(
    structure_cache: &StructureCache,
    cell_type: &str,
    port: &str,
    bit_idx: usize,
) -> Result<PinMetadata> {
    match cell_type {
        "MCPNR_LIGHTS" => {
            ensure!(
                port == "I",
                "MCPNR_LIGHTS only supports an \"I\" port (got {:?})",
                port
            );
            Ok(PinMetadata {
                offset_x: (bit_idx as u32) * 2,
                offset_y: 1,
                offset_z: 2,
                sig_derating: 0,
                direction: PinDirection::Input,
            })
        }
        "MCPNR_SWITCHES" => {
            ensure!(
                port == "O",
                "MCPNR_SWITCHES only supports an \"O\" port (got {:?})",
                port
            );
            Ok(PinMetadata {
                offset_x: (bit_idx as u32) * 2,
                offset_y: 1,
                offset_z: 2,
                sig_derating: 0,
                direction: PinDirection::Output,
            })
        }
        _ => {
            ensure!(
                bit_idx == 0,
                "NBT-style cells (like {:?}) only support single-bit input ports",
                cell_type
            );
            structure_cache
                .get(cell_type)
                .ok_or_else(|| anyhow!("Unknown cell type {:?}", cell_type))
                .and_then(|v| {
                    v.pins.get(port).map(|v| v.clone()).ok_or_else(|| {
                        anyhow!("Unknown port {:?} for cell type {:?}", port, cell_type)
                    })
                })
        }
    }
}
