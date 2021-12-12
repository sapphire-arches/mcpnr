mod splat;

use anyhow::{Context, Result};
use itertools::Itertools;
use mcpnr_common::block_storage::BlockStorage;
use mcpnr_common::prost::Message;
use mcpnr_common::protos::mcpnr::PlacedDesign;
use splat::{SplattableStructure, Splatter};
use std::collections::HashMap;
use std::path::PathBuf;

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

fn main() -> Result<()> {
    let config = parse_args();

    let placed_design = {
        let inf = std::fs::read(config.input_file).unwrap();
        PlacedDesign::decode(&inf[..]).unwrap()
    };

    // TODO: compute size from network bounds recovered by parsing input
    let mut output_structure = BlockStorage::new(50, 50, 50);

    let gates: HashMap<String, SplattableStructure> = placed_design
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

            let cell = SplattableStructure::new(cell, &mut output_structure)?;

            Ok((name.into(), cell))
        })
        .try_collect()?;

    let splatter = Splatter::new(&mut output_structure, gates);

    for cell in placed_design.cells.iter() {
        splatter.splat_cell(cell, &mut output_structure)?;
    }

    {
        let outf = std::fs::File::create(config.output_file).unwrap();

        serde_json::ser::to_writer(outf, &output_structure)?;
    }

    Ok(())
}
