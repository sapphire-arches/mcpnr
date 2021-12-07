mod minecraft_types;

use mcpnr_common::prost::Message;
use mcpnr_common::protos::mcpnr::PlacedDesign;
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

fn main() {
    let config = parse_args();

    let placed_design = {
        let inf = std::fs::read(config.input_file).unwrap();
        PlacedDesign::decode(&inf[..]).unwrap()
    };

    let nbt_cell = placed_design
        .cells
        .iter()
        .find(|c| c.r#type.ends_with(".nbt"))
        .expect("Failed to find any NBT cells");

    let parsed_cell = {
        let nbt_cell_path = config.structure_directory.join(&nbt_cell.r#type);
        let mut nbt_cell_file = std::fs::File::open(&nbt_cell_path).unwrap();
        let (raw_cell, _) = quartz_nbt::io::read_nbt(&mut nbt_cell_file, quartz_nbt::io::Flavor::GzCompressed).unwrap();
        println!("{}", raw_cell.to_snbt());

        let mut nbt_cell_file = std::fs::File::open(&nbt_cell_path).unwrap();
        let (cell, _): (minecraft_types::Structure, _) = quartz_nbt::serde::deserialize_from(
            &mut nbt_cell_file,
            quartz_nbt::io::Flavor::GzCompressed,
        )
        .expect(&format!(
            "Failed to parse structure for {}",
            nbt_cell.r#type
        ));

        cell
    };

    {
        let mut outf = std::fs::File::create(config.output_file).unwrap();

        quartz_nbt::serde::serialize_into(
            &mut outf,
            &parsed_cell,
            None,
            quartz_nbt::io::Flavor::GzCompressed,
        )
        .expect("Failed to write output");
    }
}
