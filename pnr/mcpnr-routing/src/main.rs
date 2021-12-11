mod minecraft_types;

use anyhow::{Context, Result};
use itertools::Itertools;
use mcpnr_common::block_storage::{Block, BlockStorage, PropertyValue};
use mcpnr_common::prost::Message;
use mcpnr_common::protos::mcpnr::placed_design::Cell;
use mcpnr_common::protos::mcpnr::PlacedDesign;
use quartz_nbt::NbtCompound;
use std::collections::{hash_map::Entry, HashMap};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
struct Config {
    input_file: PathBuf,
    techlib_directory: PathBuf,
    structure_directory: PathBuf,
    output_file: PathBuf,
}

fn parse_args() -> Config {
    use clap::{App, Arg};
    let matches = App::new("MCPNR Placer")
        .version(env!("CARGO_PKG_VERSION"))
        .author(clap::crate_authors!())
        .about("Placement phase for the MCPNR flow")
        .arg(
            Arg::with_name("TECHLIB")
                .long("techlib")
                .value_name("TECHLIB")
                .required(true),
        )
        .arg(
            Arg::with_name("INPUT")
                .help("Input design, as the output of a Yosys write_protobuf command")
                .index(1)
                .required(true),
        )
        .arg(
            Arg::with_name("OUTPUT")
                .help("Output file location")
                .index(2)
                .required(true),
        )
        .get_matches();

    let techlib_directory = PathBuf::from(matches.value_of_os("TECHLIB").unwrap());

    Config {
        input_file: PathBuf::from(matches.value_of_os("INPUT").unwrap()),
        output_file: PathBuf::from(matches.value_of_os("OUTPUT").unwrap()),
        structure_directory: techlib_directory.join("structures"),
        techlib_directory,
    }
}

fn splat_nbt(o: &mut BlockStorage, cell: &Cell, gate: &minecraft_types::Structure) -> Result<()> {
    // TODO: cache this (wrap raw Structure)
    let palette: Vec<Block> = gate
        .palette
        .iter()
        .map(|block| Block {
            name: block.name.clone(),
            properties: block.properties.as_ref().map(|c| {
                c.inner()
                    .iter()
                    .map(|(k, v)| {
                        let v = match v {
                            quartz_nbt::NbtTag::Byte(ref v) => PropertyValue::BYTE(*v),
                            quartz_nbt::NbtTag::String(ref s) => PropertyValue::STR(s.to_owned()),
                            _ => {
                                panic!("Unsupported property tag in mapping {:?}", v)
                            }
                        };
                        (k.to_owned(), v)
                    })
                    .collect()
            }),
        })
        .collect();

    let (base_x, base_y, base_z) = cell
        .pos
        .as_ref()
        .map(|p| (p.x, p.y, p.z))
        .unwrap_or((0, 0, 0));
    for sblock in gate.blocks.iter() {
        let [block_x, block_y, block_z] = sblock.pos;
        let x: u32 = (block_x + (base_x as i32)).try_into()?;
        let y: u32 = (block_y + (base_y as i32)).try_into()?;
        let z: u32 = (block_z + (base_z as i32)).try_into()?;

        let block = palette
            .get(sblock.state as usize)
            .with_context(|| format!("Invalid block state index {:?}", sblock.state))?;
        // TODO: we can probably dodge this clone
        let block = o.add_new_block_type(block.clone());
        o.set_block(x, y, z, block);
    }

    Ok(())
}

fn main() -> Result<()> {
    let config = parse_args();

    let placed_design = {
        let inf = std::fs::read(config.input_file).unwrap();
        PlacedDesign::decode(&inf[..]).unwrap()
    };

    let gates: HashMap<String, minecraft_types::Structure> = placed_design
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
            let nbt_cell_file = (&config.structure_directory).join(name);
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

            Ok((name.into(), cell))
        })
        .try_collect()?;

    // TODO: compute size from network bounds recovered by parsing input
    let mut output_structure = BlockStorage::new(50, 50, 50);

    for cell in placed_design.cells.iter() {
        let gate = gates.get(&cell.r#type);
        if let Some(gate) = gate {
            splat_nbt(&mut output_structure, cell, gate)?;
        } else if cell.r#type == "MCPNR_SWITCHES" {
            eprintln!("TODO: synth switches");
        } else if cell.r#type == "MCPNR_LIGHTS" {
            eprintln!("TODO: synth lights");
        }
    }

    {
        let outf = std::fs::File::create(config.output_file).unwrap();

        serde_json::ser::to_writer(outf, &output_structure)?;
    }

    Ok(())
}
