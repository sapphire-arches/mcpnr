use anyhow::{anyhow, Context, Result};
use itertools::Itertools;
use mcpnr_common::{
    block_storage::{Block, BlockStorage, BlockTypeIndex, PropertyValue},
    minecraft_types::Structure,
    protos::mcpnr::PlacedDesign,
};
use quartz_nbt::NbtCompound;
use std::{collections::HashMap, path::Path};

use crate::netlist::{PinDirection, PinMetadata};

pub struct RoutableStructure {
    pub structure: Structure,
    pub palette_palette_map: HashMap<i32, BlockTypeIndex>,
    pub pins: HashMap<String, PinMetadata>,
}

impl RoutableStructure {
    pub fn new(base: Structure) -> Result<Self> {
        let pins = base
            .blocks
            .iter()
            .filter_map(|block| -> Option<Result<_>> {
                block.nbt.as_ref().map(|nbt| {
                    fn get_text_element<'a>(
                        nbt: &'a NbtCompound,
                        element: &str,
                    ) -> Result<String> {
                        let content = nbt.get::<_, &str>(element).context("Get NBT tag")?;
                        let content: serde_json::Value =
                            serde_json::from_str(content).context("JSON parse")?;
                        let content = content.as_object().ok_or_else(|| {
                            anyhow!("JSON content root was not object, got {:?}", content)
                        })?;
                        let content = content.get("text").ok_or_else(|| {
                            anyhow!("Text object was missing 'text' attribute: {:?}", content)
                        })?;
                        let content = content
                            .as_str()
                            .ok_or_else(|| anyhow!("Text object was not text, was {}", content))?;

                        Ok(content.to_owned())
                    }

                    let text1 = get_text_element(&nbt, "Text1").context("Extract Text1")?;
                    let text2 = get_text_element(&nbt, "Text2").context("Extract Text2")?;
                    let text3 = get_text_element(&nbt, "Text3").context("Extract Text3")?;
                    // let text4 = get_text_element(&nbt, "Text4").context("Extract Text4")?;

                    let direction = match text2.as_ref() {
                        "INPUT" => PinDirection::Input,
                        "OUTPUT" => PinDirection::Output,
                        _ => return Err(anyhow!("Unknown pin direction {}", text2)),
                    };

                    let sig_derating = text3
                        .split_once("-")
                        .map(|(_, derating)| {
                            derating
                                .parse::<u32>()
                                .with_context(|| anyhow!("Convert integer {:?}", derating))
                        })
                        .unwrap_or(Ok(0))
                        .with_context(|| anyhow!("Parse derating from {:?}", text3))?;

                    Ok((
                        text1.to_owned(),
                        PinMetadata {
                            offset_x: block.pos[0]
                                .try_into()
                                .context(anyhow!("Converting X coordinate"))?,
                            offset_y: block.pos[1]
                                .try_into()
                                .context(anyhow!("Converting Y coordinate"))?,
                            offset_z: block.pos[2]
                                .try_into()
                                .context(anyhow!("Converting Z coordinate"))?,
                            sig_derating,
                            direction,
                        },
                    ))
                })
            })
            .try_collect()
            .context("Error collecting pins")?;

        Ok(Self {
            structure: base,
            palette_palette_map: Default::default(),
            pins,
        })
    }

    fn build_palette_map(&mut self, output: &mut BlockStorage) -> Result<()> {
        for (idx, block) in self.structure.palette.iter().enumerate() {
            self.palette_palette_map.insert(
                idx as i32,
                output.add_new_block_type(Block {
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
                }),
            );
        }

        Ok(())
    }
}

pub struct StructureCache {
    structures: HashMap<String, RoutableStructure>,
}

impl StructureCache {
    pub fn new(base_path: &Path, design: &PlacedDesign) -> Result<Self> {
        let structures = design
            .cells
            .iter()
            .filter_map(|cell| {
                if cell.r#type.ends_with(".nbt") {
                    Some(&cell.r#type)
                } else {
                    None
                }
            })
            .unique()
            .map(|name| -> Result<_> {
                let nbt_cell_file = (&base_path).join(name);
                let mut nbt_cell_file = std::fs::File::open(&nbt_cell_file).with_context(|| {
                    format!(
                        "Failed to open structure file {:?} for reading",
                        nbt_cell_file
                    )
                })?;
                let (cell, _) = quartz_nbt::serde::deserialize_from(
                    &mut nbt_cell_file,
                    quartz_nbt::io::Flavor::GzCompressed,
                )
                .with_context(|| format!("Failed to parse structure for {:?}", name))?;

                let cell = RoutableStructure::new(cell).with_context(|| anyhow!("Failed to process cell {}", name))?;

                Ok((name.into(), cell))
            })
            .try_collect()?;

        Ok(Self { structures })
    }

    pub fn build_palette_maps(&mut self, output: &mut BlockStorage) -> Result<()> {
        for (name, structure) in self.structures.iter_mut() {
            structure
                .build_palette_map(output)
                .with_context(|| anyhow!("While processing {}", name))?
        }
        Ok(())
    }

    pub fn get(&self, name: &str) -> Option<&RoutableStructure> {
        self.structures.get(name)
    }
}
