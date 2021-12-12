//! Logic for rendering various modules into the world

use crate::minecraft_types::Structure;
use anyhow::{anyhow, Context, Result};
use itertools::Itertools;
use mcpnr_common::{
    block_storage::{Block, BlockStorage, BlockTypeIndex, PropertyValue},
    protos::{mcpnr::placed_design::Cell, yosys::pb::parameter::Value, CellExt},
};
use std::collections::HashMap;

pub struct SplattableStructure {
    structure: Structure,

    /// Map from the structure's palette to the output device
    palette_palette_map: HashMap<i32, BlockTypeIndex>,
}

impl SplattableStructure {
    pub fn new(base: Structure, o: &mut BlockStorage) -> Result<Self> {
        let palette_palette_map = base
            .palette
            .iter()
            .map(|block| -> Result<_> {
                Ok(o.add_new_block_type(Block {
                    name: block.name.clone(),
                    properties: match block.properties.as_ref() {
                        Some(c) => Some(
                            c.inner()
                                .iter()
                                .map(|(k, v)| {
                                    let v = match v {
                                        quartz_nbt::NbtTag::Byte(ref v) => PropertyValue::Byte(*v),
                                        quartz_nbt::NbtTag::String(ref s) => {
                                            PropertyValue::String(s.to_owned())
                                        }
                                        _ => {
                                            return Err(anyhow!(
                                                "Unsupported property tag in mapping {:?}",
                                                v
                                            ))
                                        }
                                    };
                                    Ok((k.to_owned(), v))
                                })
                                .try_collect()
                                .with_context(|| format!("While mapping block {:?}", block))?,
                        ),
                        None => None,
                    },
                }))
            })
            .enumerate()
            .map(|(idx, block)| -> Result<_> { Ok((idx as i32, block?)) })
            .try_collect()
            .with_context(|| format!("While mapping structure {:?}", base))?;

        Ok(Self {
            structure: base,
            palette_palette_map,
        })
    }
}

pub struct Splatter {
    gates: HashMap<String, SplattableStructure>,

    common_blocks: HashMap<String, BlockTypeIndex>,
}

impl Splatter {
    pub fn new(o: &mut BlockStorage, gates: HashMap<String, SplattableStructure>) -> Self {
        let common_blocks = [
            (
                "calcite".to_owned(),
                o.add_new_block_type(Block::new("minecraft:calcite".to_owned())),
            ),
            (
                "redstone_lamp".to_owned(),
                o.add_new_block_type(Block::new("minecraft:redstone_lamp".to_owned())),
            ),
            (
                "switch".to_owned(),
                o.add_new_block_type(Block {
                    name: "minecraft:lever".to_owned(),
                    properties: Some(
                        [
                            ("face".to_owned(), PropertyValue::String("wall".to_owned())),
                            (
                                "facing".to_owned(),
                                PropertyValue::String("north".to_owned()),
                            ),
                        ]
                        .into_iter()
                        .collect(),
                    ),
                }),
            ),
        ]
        .into_iter()
        .collect();

        Self {
            gates,
            common_blocks,
        }
    }

    ///  Splat a module with its minimum (x,y,z) coordinates at the provided
    ///  location
    pub fn splat_cell(&self, cell: &Cell, o: &mut BlockStorage) -> Result<()> {
        (if cell.r#type == "MCPNR_LIGHTS" {
            self.splat_lights(cell, o)
        } else if cell.r#type == "MCPNR_SWITCHES" {
            self.splat_switches(cell, o)
        } else {
            self.splat_structure_cell(cell, o)
        })
        .with_context(|| anyhow!("While processing cell {:?}", cell))
    }

    fn get_common_block(&self, name: &str) -> Result<BlockTypeIndex> {
        self.common_blocks
            .get(name)
            .ok_or_else(|| anyhow!("Failed to find common block {:?}", name))
            .map(|v| *v)
    }

    fn splat_lights(&self, cell: &Cell, o: &mut BlockStorage) -> Result<()> {
        let nlights = cell.get_param_i64_with_default("NLIGHT", 1)?;

        let (base_x, base_y, base_z) = cell
            .pos
            .as_ref()
            .map(|p| (p.x, p.y, p.z))
            .unwrap_or((0, 0, 0));

        let b_calcite = self.get_common_block("calcite")?;
        let b_light = self.get_common_block("redstone_lamp")?;

        for light in 0..nlights {
            let light_x = (light * 2) as u32 + base_x;

            *(o.get_block_mut(light_x + 0, base_y + 0, base_z + 1)?) = b_calcite;
            *(o.get_block_mut(light_x + 0, base_y + 1, base_z + 1)?) = b_light;
            *(o.get_block_mut(light_x + 1, base_y + 0, base_z + 1)?) = b_calcite;
            *(o.get_block_mut(light_x + 1, base_y + 1, base_z + 1)?) = b_calcite;
        }

        Ok(())
    }

    fn splat_switches(&self, cell: &Cell, o: &mut BlockStorage) -> Result<()> {
        let nswitches = cell.get_param_i64_with_default("NSWITCH", 1)?;

        let (base_x, base_y, base_z) = cell
            .pos
            .as_ref()
            .map(|p| (p.x, p.y, p.z))
            .unwrap_or((0, 0, 0));

        let b_calcite = self.get_common_block("calcite")?;
        let b_switch = self.get_common_block("switch")?;

        for switch in 0..nswitches {
            let switch_x = (switch * 2) as u32 + base_x;

            *(o.get_block_mut(switch_x + 0, base_y + 1, base_z + 0)?) = b_switch;

            *(o.get_block_mut(switch_x + 0, base_y + 0, base_z + 1)?) = b_calcite;
            *(o.get_block_mut(switch_x + 0, base_y + 1, base_z + 1)?) = b_calcite;
            *(o.get_block_mut(switch_x + 1, base_y + 0, base_z + 1)?) = b_calcite;
            *(o.get_block_mut(switch_x + 1, base_y + 1, base_z + 1)?) = b_calcite;
        }

        Ok(())
    }

    fn splat_structure_cell(&self, cell: &Cell, o: &mut BlockStorage) -> Result<()> {
        let gate = self
            .gates
            .get(&cell.r#type)
            .ok_or_else(|| anyhow!("Unknown cell type {}", cell.r#type))?;
        let (base_x, base_y, base_z) = cell
            .pos
            .as_ref()
            .map(|p| (p.x, p.y, p.z))
            .unwrap_or((0, 0, 0));
        for sblock in gate.structure.blocks.iter() {
            let [block_x, block_y, block_z] = sblock.pos;
            let x: u32 = (block_x + (base_x as i32)).try_into()?;
            let y: u32 = (block_y + (base_y as i32)).try_into()?;
            let z: u32 = (block_z + (base_z as i32)).try_into()?;

            *(o.get_block_mut(x, y, z)?) = *gate
                .palette_palette_map
                .get(&sblock.state)
                .with_context(|| format!("Invalid block state index {:?}", sblock.state))?;
        }

        Ok(())
    }
}
