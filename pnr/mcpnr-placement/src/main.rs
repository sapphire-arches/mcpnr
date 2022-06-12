use anyhow::{anyhow, Context, Result};
use clap::{Arg, Command};
use itertools::Itertools;
use mcpnr_common::prost::Message;
use mcpnr_common::protos::mcpnr::{PlacedDesign, Position};
use mcpnr_common::protos::yosys::pb::parameter::Value as YPValue;
use mcpnr_common::protos::yosys::pb::{Design, Parameter};
use placement_cell::{CellFactory, PlacementCell};
use std::path::PathBuf;

use crate::core::PlaceableCells;

mod core;
mod gui;
mod placement_cell;

#[derive(Clone, Debug)]
struct Config {
    input_file: PathBuf,
    output_file: PathBuf,
    structure_directory: PathBuf,
    size_x: u32,
    size_z: u32,
}

impl Config {
    fn from_args(matches: &clap::ArgMatches) -> Result<Self> {
        let techlib_directory = PathBuf::from(matches.value_of_os("TECHLIB").unwrap());
        Ok(Config {
            input_file: PathBuf::from(matches.value_of_os("INPUT").unwrap()),
            output_file: PathBuf::from(matches.value_of_os("OUTPUT").unwrap()),
            structure_directory: techlib_directory.join("structures"),
            size_x: matches
                .value_of("SIZE_X")
                .unwrap()
                .parse()
                .context("Parse SIZE_X")?,
            size_z: matches
                .value_of("SIZE_Z")
                .unwrap()
                .parse()
                .context("Parse SIZE_Z")?,
        })
    }
}

fn add_common_args<'help>(command: Command<'help>) -> Command<'help> {
    command
        .arg(
            Arg::new("TECHLIB")
                .long("techlib")
                .value_name("TECHLIB")
                .allow_invalid_utf8(true)
                .required(true),
        )
        .arg(
            Arg::new("SIZE_X")
                .long("size-x")
                .value_name("SIZE_X")
                .default_value("192"),
        )
        .arg(
            Arg::new("SIZE_Z")
                .long("size-z")
                .value_name("SIZE_Z")
                .default_value("192"),
        )
        .arg(
            Arg::new("INPUT")
                .help("Input design, as the output of a Yosys write_protobuf command")
                .index(1)
                .allow_invalid_utf8(true)
                .required(true),
        )
        .arg(
            Arg::new("OUTPUT")
                .help("Output file location")
                .index(2)
                .allow_invalid_utf8(true)
                .required(true),
        )
}

fn place(config: &Config, design: Design) -> Result<PlacedDesign> {
    let top_module = design
        .modules
        .into_values()
        .find(|m| {
            m.attribute.get("top")
                == Some(&Parameter {
                    value: Some(YPValue::Int(1)),
                })
        })
        .ok_or_else(|| anyhow!("Failed to locate top module"))?;

    let mut cell_factory = CellFactory::new(config.structure_directory.clone());

    let mut cells = PlaceableCells::from_module(top_module, &mut cell_factory)
        .with_context(|| "Extract cells")?;

    // TODO: smart place
    let mut cx = 0;
    let mut cz = 4;
    let mut row_max_z = 0;
    let mut tier = 0;

    for cell in cells.cells.iter_mut() {
        if cell.pos_locked {
            continue;
        }
        if cx + cell.sx > config.size_x {
            // TODO: don't hard code region size
            cz += row_max_z;
            row_max_z = 0;
            cx = 0;
        }
        if cz > config.size_z {
            tier += 1;
            cx = 0;
            cz = 0;
        }
        cell.x = cx;
        cell.z = cz;
        cell.y = tier * 16;

        cx += cell.sx;
        row_max_z = std::cmp::max(cell.sz, row_max_z);
    }
    println!("Required tiers: {}", tier + 1);

    Ok(cells.build_output(design.creator))
}

fn run_placement(config: &Config) -> Result<()> {
    let design = {
        let inf = std::fs::read(&config.input_file)
            .with_context(|| anyhow!("Open input file {:?}", config.input_file))?;
        Design::decode(&inf[..])
            .with_context(|| anyhow!("Failed to parse file {:?}", config.input_file))?
    };

    let placed_design = place(&config, design)
        .with_context(|| anyhow!("Place design from {:?}", config.input_file))?;

    {
        use std::io::Write;
        let mut outf = std::fs::File::create(&config.output_file).with_context(|| {
            anyhow!("Failed to open/create output file {:?}", config.output_file)
        })?;
        let encoded = placed_design.encode_to_vec();

        outf.write_all(&encoded[..])
            .with_context(|| anyhow!("Failed to write to output file {:?}", config.output_file))?;
    }

    Ok(())
}

fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let gui_command = add_common_args(Command::new("gui"))
        .help("Run a GUI for interactive debugging of the placer");
    let place_command =
        add_common_args(Command::new("place")).help("Run the placer in headless mode");
    let matches = Command::new("mcpnr-placement")
        .version(env!("CARGO_PKG_VERSION"))
        .author(env!("CARGO_PKG_AUTHORS"))
        .about("Placement phase for the MCPNR flow")
        .subcommands(vec![gui_command, place_command])
        .get_matches();

    match matches.subcommand() {
        Some(("gui", matches)) => {
            gui::run_gui(&Config::from_args(matches).context("Building config from args")?);
            Ok(())
        }
        Some(("place", matches)) => {
            run_placement(&Config::from_args(matches).context("Building config from args")?)
        }
        e => panic!("Unhandled subcommand {:?}", e),
    }
}
