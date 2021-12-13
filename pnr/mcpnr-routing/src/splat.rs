//! Logic for rendering various modules into the world

use anyhow::{anyhow, Context, Result};
use mcpnr_common::{
    block_storage::{Block, BlockStorage, BlockTypeIndex, PropertyValue},
    protos::{mcpnr::placed_design::Cell, CellExt},
};
use std::collections::HashMap;

use crate::structure_cache::StructureCache;

pub struct Splatter<'a> {
    structure_cache: &'a StructureCache,
    common_blocks: HashMap<String, BlockTypeIndex>,
}

impl<'a> Splatter<'a> {
    pub fn new(o: &mut BlockStorage, structure_cache: &'a StructureCache) -> Self {
        let common_blocks = [
            (
                "air".to_owned(),
                o.add_new_block_type(Block::new("minecraft:air".to_owned())),
            ),
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
            (
                "wool".to_owned(),
                o.add_new_block_type(Block {
                    name: "minecraft:black_wool".to_owned(),
                    properties: None,
                    // properties: Some(
                    //     [(
                    //         "color".to_owned(),
                    //         PropertyValue::String("black".to_owned()),
                    //     )]
                    //     .into_iter()
                    //     .collect(),
                    // ),
                }),
            ),
        ]
        .into_iter()
        .collect();

        Self {
            structure_cache,
            common_blocks,
        }
    }

    pub fn draw_border(&self, o: &mut BlockStorage) -> Result<()> {
        let extents = o.extents().clone();
        let wool = self.get_common_block("wool").context("Look up wool")?;
        for y in 0..extents[1] {
            for x in 0..extents[0] {
                *(o.get_block_mut(x, y, 0)?) = wool;
                *(o.get_block_mut(x, y, extents[2] - 1)?) = wool;
            }
            for z in 0..extents[2] {
                *(o.get_block_mut(0, y, z)?) = wool;
                *(o.get_block_mut(extents[0] - 1, y, z)?) = wool;
            }
        }

        Ok(())
    }

    /// Splat a module with its minimum (x,y,z) coordinates at the provided
    /// location
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

        let b_air = self.get_common_block("air")?;
        let b_calcite = self.get_common_block("calcite")?;
        let b_light = self.get_common_block("redstone_lamp")?;

        for light in 0..nlights {
            let light_x = (light * 2) as u32 + base_x;

            *(o.get_block_mut(light_x + 0, base_y + 0, base_z + 0)?) = b_air;
            *(o.get_block_mut(light_x + 0, base_y + 1, base_z + 0)?) = b_air;
            *(o.get_block_mut(light_x + 1, base_y + 0, base_z + 0)?) = b_air;
            *(o.get_block_mut(light_x + 1, base_y + 1, base_z + 0)?) = b_air;

            *(o.get_block_mut(light_x + 0, base_y + 0, base_z + 1)?) = b_calcite;
            *(o.get_block_mut(light_x + 0, base_y + 1, base_z + 1)?) = b_light;
            *(o.get_block_mut(light_x + 1, base_y + 0, base_z + 1)?) = b_calcite;
            *(o.get_block_mut(light_x + 1, base_y + 1, base_z + 1)?) = b_calcite;

            *(o.get_block_mut(light_x + 0, base_y + 0, base_z + 2)?) = b_light;
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

        let b_air = self.get_common_block("air")?;
        let b_calcite = self.get_common_block("calcite")?;
        let b_switch = self.get_common_block("switch")?;

        for switch in 0..nswitches {
            let switch_x = (switch * 2) as u32 + base_x;

            *(o.get_block_mut(switch_x + 0, base_y + 0, base_z + 0)?) = b_air;
            *(o.get_block_mut(switch_x + 0, base_y + 1, base_z + 0)?) = b_switch;
            *(o.get_block_mut(switch_x + 1, base_y + 0, base_z + 0)?) = b_air;
            *(o.get_block_mut(switch_x + 1, base_y + 1, base_z + 0)?) = b_air;

            *(o.get_block_mut(switch_x + 0, base_y + 0, base_z + 1)?) = b_calcite;
            *(o.get_block_mut(switch_x + 0, base_y + 1, base_z + 1)?) = b_calcite;
            *(o.get_block_mut(switch_x + 1, base_y + 0, base_z + 1)?) = b_calcite;
            *(o.get_block_mut(switch_x + 1, base_y + 1, base_z + 1)?) = b_calcite;

            *(o.get_block_mut(switch_x + 0, base_y + 0, base_z + 2)?) = b_calcite;
        }

        Ok(())
    }

    fn splat_structure_cell(&self, cell: &Cell, o: &mut BlockStorage) -> Result<()> {
        let gate = self
            .structure_cache
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
