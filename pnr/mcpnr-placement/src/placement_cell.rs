use anyhow::{anyhow, Context, Result};
use mcpnr_common::{minecraft_types::Structure, yosys::Cell, CellExt, BLOCKS_PER_TIER};
use nalgebra::Vector3;
use std::{collections::HashMap, path::PathBuf};

/// Internal type containing the metadata we care about from a given cell's NBT data.
pub(crate) struct PlacementStructureData {
    /// X size, in blocks.
    sx: u32,
    /// Y size, in blocks.
    sy: u32,
    /// Z size, in blocks.
    sz: u32,
}

/// yeah it's a java thing get over it.
///
/// caches structure reads so we can avoid re-parsing on every cell
pub struct CellFactory {
    structure_directory: PathBuf,
    structure_cache: HashMap<String, PlacementStructureData>,
}

/// Cell representation for global placement.
#[derive(Debug)]
pub struct PlacementCell {
    pub x: f32,
    pub tier_y: f32,
    pub z: f32,
    pub sx: f32,
    pub s_tier_y: f32,
    pub sz: f32,
    /// Whether the cell is locked in place (e.g. it's an IO macro)
    ///
    /// TODO: this should be removed, and NetlistHypergraph reworked so that cells are ordered
    /// position-locked first
    pub pos_locked: bool,
}

impl PlacementCell {
    pub fn center_pos(&self) -> Vector3<f32> {
        Vector3::new(
            self.x as f32 + (self.sx as f32 / 2.0),
            self.tier_y as f32 + (self.s_tier_y as f32 / 2.0),
            self.z as f32 + (self.sz as f32 / 2.0),
        )
    }
}

/// Cell post-legalization.
#[derive(Debug)]
pub struct LegalizedCell {
    /// Position on the X axis, in blocks.
    pub x: u32,
    /// Position on the Y axis, in tiers.
    pub tier_y: u32,
    /// Position on the Z axis, in blocks.
    pub z: u32,
    /// Size along the X axis, in blocks.
    pub sx: u32,
    /// Size along the Y axis, in tiers.
    pub s_tier_y: u32,
    /// Size along the Z axis, in blocks.
    pub sz: u32,
}

impl LegalizedCell {
    /// Construct a [LegalizedCell] from a [PlacementCell]
    pub fn from_placement(cell: &PlacementCell) -> Self {
        Self {
            x: (cell.x.round() + 0.5) as u32,
            tier_y: (cell.tier_y.round() + 0.5) as u32,
            z: (cell.z.round() + 0.5) as u32,
            sx: (cell.sx.round() + 0.5) as u32,
            s_tier_y: (cell.s_tier_y.round() + 0.5) as u32,
            sz: (cell.sz.round() + 0.5) as u32,
        }
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
                sx: (((cell_extents.1).0) - ((cell_extents.0).0)) as u32,
                sy: (((cell_extents.1).1) - ((cell_extents.0).1)) as u32,
                sz: (((cell_extents.1).2) - ((cell_extents.0).2)) as u32,
            };

            log::info!(
                "Loaded {structure_name}. Size {}x{}x{}",
                cell_data.sx,
                cell_data.sy,
                cell_data.sz
            );

            Ok(self
                .structure_cache
                .entry(structure_name.to_owned())
                .or_insert(cell_data))
        }
    }

    pub fn build_cell(&mut self, cell: &Cell) -> Result<PlacementCell> {
        // TODO: maybe all these should output a sy of 1.0 since most of the rest of the code
        // effectively already assumes that the y coordinate is in layers
        match cell.ty.as_ref() {
            "MCPNR_SWITCHES" => self
                .build_switches(cell)
                .context("Failed to build switch module"),
            "MCPNR_LIGHTS" => self
                .build_lights(cell)
                .context("Failed to build light module"),
            _ => self
                .build_from_nbt(cell)
                .with_context(|| anyhow!("Failed to build {} module", cell.ty)),
        }
    }

    pub fn build_switches<'design>(&mut self, cell: &Cell) -> Result<PlacementCell> {
        let (x, y, z) = get_cell_pos(cell)?;
        let nswitches = cell.get_param_i64_with_default("NSWITCH", 1)?;
        if x > 0 && z > 0 {
            log::warn!(
                "Switches located at (x,z) ({x}, {z}) will cause the legalizer to misbehave!"
            );
        }
        Ok(PlacementCell {
            x: x as f32,
            tier_y: (y / BLOCKS_PER_TIER) as f32,
            z: z as f32,
            sx: (nswitches as f32) * 2.0,
            s_tier_y: 1.0,
            sz: 4.0,
            pos_locked: true,
        })
    }

    pub fn build_lights<'design>(&mut self, cell: &Cell) -> Result<PlacementCell> {
        let (x, y, z) = get_cell_pos(cell)?;
        let nlight = cell.get_param_i64_with_default("NLIGHT", 1)?;
        if x > 0 && z > 0 {
            log::warn!("Lights located at (x,z) ({x}, {z}) will cause the legalizer to misbehave!");
        }
        Ok(PlacementCell {
            x: x as f32,
            tier_y: (y / BLOCKS_PER_TIER) as f32,
            z: z as f32,
            sx: (nlight as f32) * 2.0,
            s_tier_y: 1.0,
            sz: 2.0,
            pos_locked: true,
        })
    }

    pub fn build_from_nbt<'design>(&mut self, cell: &Cell) -> Result<PlacementCell> {
        let sd = self.load_structure(&cell.ty)?;

        let s_tier_y = (sd.sy + BLOCKS_PER_TIER - 1) / BLOCKS_PER_TIER;

        Ok(PlacementCell {
            x: 0.0,
            tier_y: 0.0,
            z: 0.0,
            sx: (sd.sx + (sd.sx % 2)) as f32,
            s_tier_y: s_tier_y as f32,
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
