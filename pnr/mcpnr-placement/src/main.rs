use anyhow::{anyhow, Context, Result};
use clap::{Arg, Command};
use mcpnr_common::prost::Message;
use mcpnr_common::protos::mcpnr::PlacedDesign;
use mcpnr_common::protos::yosys::pb::parameter::Value as YPValue;
use mcpnr_common::protos::yosys::pb::{Design, Parameter};
use placement_cell::CellFactory;
use placer::analytical::{
    AnchoredByNet, Clique, DecompositionStrategy, MoveableStar, ThresholdCrossover,
};
use placer::diffusion::DiffusionPlacer;
use std::convert::TryInto;
use std::path::PathBuf;
use tracing::info_span;
use tracing_subscriber::fmt::format::FmtSpan;

use crate::core::NetlistHypergraph;

mod core;
mod config;
mod gui;
mod placement_cell;
pub mod placer;

fn add_common_args<'help>(command: Command<'help>) -> Command<'help> {
    command
        .arg(
            Arg::new("TECHLIB")
                .long("techlib")
                .value_name("TECHLIB")
                .allow_invalid_utf8(true)
                .required(true)
                .help("Specify the path to the technology library")
                .long_help("
The technology library is expected to be a folder, containing a folder named \"structures\" with a minecraft NBT structure format for each standard cell.
"),
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

fn load_design(config: &Config) -> Result<Design> {
    let inf = std::fs::read(&config.input_file)
        .with_context(|| anyhow!("Open input file {:?}", config.input_file))?;
    Design::decode(&inf[..])
        .with_context(|| anyhow!("Failed to parse file {:?}", config.input_file))
}

fn load_cells(config: &Config, design: Design) -> Result<(NetlistHypergraph, String)> {
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

    let cells = NetlistHypergraph::from_module(top_module, &mut cell_factory)
        .with_context(|| "Extract cells")?;

    Ok((cells, design.creator))
}

fn place_algorithm(config: &Config, cells: &mut NetlistHypergraph) -> Result<()> {
    // Initial placement
    let mut strategy = ThresholdCrossover::new(4, Clique::new(), MoveableStar::new());
    strategy.execute(cells)?;

    for wide_iteration in 0..256 {
        // Density-driven cell spreading
        info_span!("Density placement", iteration = wide_iteration).in_scope(
            || -> Result<()> {
                let mut density = DiffusionPlacer::new(
                    config.size_x.try_into().context("Convert X size")?,
                    config.size_y.try_into().context("Convert Y size")?,
                    config.size_z.try_into().context("Convert Z size")?,
                    0.8,
                    2,
                );

                density.splat(cells);

                for narrow_iteration in 0..128 {
                    let _span = info_span!("narrow_iteration", narrow_iteration = narrow_iteration)
                        .entered();
                    density.compute_velocities();
                    density.move_cells(cells, 0.1);
                    density.step_time(0.1);
                }

                Ok(())
            },
        )?;

        // Analytic wirerecovery
        let mut strategy = ThresholdCrossover::new(2, Clique::new(), AnchoredByNet::new());
        strategy.execute(cells)?;
    }

    Ok(())
}

fn place(config: &Config, design: Design) -> Result<PlacedDesign> {
    let (mut cells, creator) = load_cells(config, design).with_context(|| anyhow!("Load cells"))?;

    place_algorithm(&config, &mut cells)
        .with_context(|| anyhow!("Initial analytical placement"))?;

    Ok(cells.build_output(creator))
}

fn run_placement(config: &Config) -> Result<()> {
    let design = load_design(config).with_context(|| anyhow!("Load design"))?;

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
    {
        use tracing_subscriber::{prelude::*, EnvFilter};

        let fmt_layer = tracing_subscriber::fmt::layer()
            .with_span_events(FmtSpan::NEW | FmtSpan::CLOSE)
            .compact();
        let filter_layer = EnvFilter::try_from_default_env()
            .or_else(|_| EnvFilter::try_new("info"))
            .expect("Failed to initialize tracing env filter");

        tracing_subscriber::registry()
            .with(filter_layer)
            .with(fmt_layer)
            .init();
    }

    let gui_command = add_common_args(
        Command::new("gui").before_help("Run a GUI for interactive debugging of the placer"),
    );
    let place_command =
        add_common_args(Command::new("place").before_help("Run the placer in headless mode"));
    let mut command = Command::new("mcpnr-placement")
        .version(env!("CARGO_PKG_VERSION"))
        .author(env!("CARGO_PKG_AUTHORS"))
        .about("Placement phase for the MCPNR flow")
        .subcommands(vec![gui_command, place_command]);
    let matches = command.get_matches_mut();

    match matches.subcommand() {
        Some(("gui", matches)) => {
            gui::run_gui(&Config::from_args(matches).context("Building config from args")?)
        }
        Some(("place", matches)) => {
            run_placement(&Config::from_args(matches).context("Building config from args")?)
        }
        None => command
            .print_long_help()
            .context("Failed to write long help"),
        e => panic!("Unhandled subcommand {:?}", e),
    }
}
