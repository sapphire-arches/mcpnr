use anyhow::{anyhow, Context, Result};
use mcpnr_common::{minecraft_types::Structure, protos::yosys::pb::module::Cell, protos::CellExt};
use nalgebra::Vector3;
use std::{collections::HashMap, path::PathBuf};

pub const XZ_EXPANSION: u32 = 2;

pub(crate) struct PlacementStructureData {
    sx: u32,
    sy: u32,
    sz: u32,
}

/// yeah it's a java thing get over it.
///
/// caches structure reads so we can avoid re-parsing on every cell
pub struct CellFactory {
    structure_directory: PathBuf,
    structure_cache: HashMap<String, PlacementStructureData>,
}

pub struct PlacementCell {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub sx: f32,
    pub sy: f32,
    pub sz: f32,
    /// Whether the cell is locked in place (e.g. it's an IO macro)
    ///
    /// TODO: this should be removed, and NetlistHypergraph reworked so that cells are ordered
    /// position-locked first
    pub pos_locked: bool,
}

impl PlacementCell {
    pub fn unexpanded_pos(&self) -> [u32; 3] {
        if self.pos_locked {
            [self.x as u32, self.y as u32, self.z as u32]
        } else {
            [
                self.x as u32 + XZ_EXPANSION,
                self.y as u32,
                self.z as u32 + XZ_EXPANSION,
            ]
        }
    }

    pub fn center_pos(&self) -> Vector3<f32> {
        Vector3::new(
            self.x as f32 + (self.sx as f32 / 2.0),
            self.y as f32 + (self.sy as f32 / 2.0),
            self.z as f32 + (self.sz as f32 / 2.0),
        )
    }
}

impl CellFactory {
    pub fn new(structure_directory: PathBuf) -> Self {
        Self {
            structure_directory,
            structure_cache: Default::default(),
        }
    }

    pub(crate) fn load_structure(
        &mut self,
        structure_name: &str,
    ) -> Result<&PlacementStructureData> {
        if self.structure_cache.contains_key(structure_name) {
            self.structure_cache
                .get(structure_name)
                .ok_or_else(|| -> ! { unreachable!() })
                .map_err(Into::into)
        } else {
            let nbt_cell_file = self.structure_directory.join(structure_name);
            let mut nbt_cell_file = std::fs::File::open(&nbt_cell_file).with_context(|| {
                format!(
                    "Failed to open structure file {:?} for reading",
                    nbt_cell_file
                )
            })?;
            let (cell, _): (Structure, _) = quartz_nbt::serde::deserialize_from(
                &mut nbt_cell_file,
                quartz_nbt::io::Flavor::GzCompressed,
            )
            .with_context(|| format!("Failed to parse structure for {:?}", structure_name))?;

            let cell_extents = cell.blocks.iter().fold(
                ((0, 0, 0), (0, 0, 0)),
                |((lx, ly, lz), (mx, my, mz)), block| {
                    (
                        (
                            std::cmp::min(lx, block.pos[0]),
                            std::cmp::min(ly, block.pos[1]),
                            std::cmp::min(lz, block.pos[2]),
                        ),
                        (
                            std::cmp::max(mx, block.pos[0]),
                            std::cmp::max(my, block.pos[1]),
                            std::cmp::max(mz, block.pos[2]),
                        ),
                    )
                },
            );

            let cell_data = PlacementStructureData {
                sx: (((cell_extents.1).0) - ((cell_extents.0).0)) as u32 + 2 * XZ_EXPANSION,
                sy: (((cell_extents.1).1) - ((cell_extents.0).1)) as u32,
                sz: (((cell_extents.1).2) - ((cell_extents.0).2)) as u32 + 2 * XZ_EXPANSION,
            };

            Ok(self
                .structure_cache
                .entry(structure_name.to_owned())
                .or_insert(cell_data))
        }
    }

    pub fn build_cell(&mut self, cell: &Cell) -> Result<PlacementCell> {
        match cell.r#type.as_ref() {
            "MCPNR_SWITCHES" => self
                .build_switches(cell)
                .context("Failed to build switch module"),
            "MCPNR_LIGHTS" => self
                .build_lights(cell)
                .context("Failed to build light module"),
            _ => self
                .build_from_nbt(cell)
                .with_context(|| anyhow!("Failed to build {} module", cell.r#type)),
        }
    }

    pub fn build_switches<'design>(&mut self, cell: &Cell) -> Result<PlacementCell> {
        let (x, y, z) = get_cell_pos(cell)?;
        let nswitches = cell.get_param_i64_with_default("NSWITCH", 1)?;
        Ok(PlacementCell {
            x: x as f32,
            y: y as f32,
            z: z as f32,
            sx: (nswitches as f32) * 2.0,
            sy: 2.0,
            sz: 4.0,
            pos_locked: true,
        })
    }

    pub fn build_lights<'design>(&mut self, cell: &Cell) -> Result<PlacementCell> {
        let (x, y, z) = get_cell_pos(cell)?;
        let nlight = cell.get_param_i64_with_default("NLIGHT", 1)?;
        Ok(PlacementCell {
            x: x as f32,
            y: y as f32,
            z: z as f32,
            sx: (nlight as f32) * 2.0,
            sy: 2.0,
            sz: 2.0,
            pos_locked: true,
        })
    }

    pub fn build_from_nbt<'design>(&mut self, cell: &Cell) -> Result<PlacementCell> {
        let sd = self.load_structure(&cell.r#type)?;

        Ok(PlacementCell {
            x: 0.0,
            y: 0.0,
            z: 0.0,
            sx: (sd.sx + (sd.sx % 2)) as f32,
            sy: (sd.sy) as f32,
            sz: (sd.sz + (sd.sz % 2)) as f32,
            pos_locked: false,
        })
    }
}

fn get_cell_pos(cell: &Cell) -> Result<(u32, u32, u32)> {
    fn get_u32_param(cell: &Cell, name: &str) -> Result<u32> {
        cell.get_param_i64_with_default(name, 0)
            .context("Get param")?
            .try_into()
            .context("Downcast from i64")
    }

    Ok((
        get_u32_param(cell, "POS_X").context("Read POS_X")?,
        get_u32_param(cell, "POS_Y").context("Read POS_Y")?,
        get_u32_param(cell, "POS_Z").context("Read POS_Z")?,
    ))
}
