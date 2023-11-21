use anyhow::{anyhow, Context, Result};
use clap::{Arg, Command};
use config::PlacementStep;
use legalizer::{tetris::TetrisLegalizer, Legalizer};
use mcpnr_common::prost::Message;
use mcpnr_common::protos::mcpnr::PlacedDesign;
use mcpnr_common::yosys::Design;
use nalgebra::Vector3;
use placement_cell::{CellFactory, LegalizedCell};
use placer::analytical::{
    AnchoredByNet, Clique, DecompositionStrategy, MoveableStar, ThresholdCrossover,
};
use placer::diffusion::DiffusionPlacer;
use tracing::{debug_span, info_span};
use tracing_subscriber::fmt::format::FmtSpan;

use crate::config::Config;
use crate::core::NetlistHypergraph;

mod config;
mod core;
mod gui;
pub mod legalizer;
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
            Arg::new("SIZE_Y")
                .long("size-y")
                .value_name("SIZE_Y")
                .default_value("4"),
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
    let reader = std::fs::File::open(&config.io.input_file)
        .with_context(|| anyhow!("Open input file {:?}", config.io.input_file))?;
    let reader = std::io::BufReader::new(reader);
    serde_json::from_reader(reader).with_context(|| anyhow!("Failed to parse reader"))
}

fn load_cells(config: &Config, design: Design) -> Result<(NetlistHypergraph, String)> {
    let top_module = design
        .modules
        .get("top")
        .ok_or_else(|| anyhow!("Failed to locate top module"))?;

    let mut cell_factory = CellFactory::new(config.io.structure_directory.clone());

    let cells = NetlistHypergraph::from_module(top_module.clone(), &mut cell_factory)
        .with_context(|| "Extract cells")?;

    Ok((cells, design.creator))
}

fn min_f32(a: f32, b: f32) -> f32 {
    if a < b {
        a
    } else {
        b
    }
}

fn max_f32(a: f32, b: f32) -> f32 {
    if a > b {
        a
    } else {
        b
    }
}

fn center_all_moveable_cells(config: &Config, cells: &mut NetlistHypergraph) {
    // Set our initial guess for the minimum position to the maximum
    let mut current_min = Vector3::new(
        config.geometry.size_x as f32,
        config.geometry.size_y as f32,
        config.geometry.size_z as f32,
    );
    // and use that handy value to compute the desired center
    let desired_center = current_min / 2.0;
    let mut current_max = Vector3::zeros();

    for cell in cells.cells.iter_mut() {
        if cell.pos_locked {
            continue;
        }
        current_max.x = max_f32(current_max.x, cell.x + cell.sx);
        current_max.y = max_f32(current_max.y, cell.y + cell.sy);
        current_max.z = max_f32(current_max.z, cell.z + cell.sz);

        current_min.x = min_f32(current_min.x, cell.x);
        current_min.y = min_f32(current_min.y, cell.y);
        current_min.z = min_f32(current_min.z, cell.z);
    }

    let delta = ((current_max - current_min) / 2.0) + current_min - desired_center;

    for cell in cells.cells.iter_mut() {
        if cell.pos_locked {
            continue;
        }
        cell.x -= delta.x;
        cell.y -= delta.y;
        cell.z -= delta.z;
    }
}

fn place_algorithm(config: &Config, cells: &mut NetlistHypergraph) -> Result<()> {
    let _span = info_span!("overall_place").entered();
    for step in &config.schedule.schedule {
        match step {
            PlacementStep::CenterCells => {
                let _span = info_span!("center_cells").entered();
                center_all_moveable_cells(config, cells);
            }
            PlacementStep::UnconstrainedAnalytical { clique_threshold } => {
                let _span = info_span!("unconstrained").entered();
                let mut strategy =
                    ThresholdCrossover::new(*clique_threshold, Clique::new(), MoveableStar::new());
                strategy.execute(cells)?;
            }
            PlacementStep::Diffusion(diffusion_config) => {
                let _span = info_span!(
                    "diffusion",
                    iterations = diffusion_config.iterations,
                    region_size = diffusion_config.region_size,
                    delta_t = diffusion_config.delta_t
                )
                .entered();
                // Iterate between diffusion and some light analytic recover
                let mut density = DiffusionPlacer::new(&config, &diffusion_config);

                density.splat(cells);

                // Diffusion simulation
                for narrow_iteration in 0..diffusion_config.iterations {
                    let _span =
                        debug_span!("narrow_iteration", narrow_iteration = narrow_iteration)
                            .entered();
                    density.compute_velocities();
                    density.move_cells(cells, diffusion_config.delta_t);
                    density.step_time(diffusion_config.delta_t);
                }
            }
            PlacementStep::ConstrainedAnalytical {
                clique_threshold,
                iterations,
            } => {
                let _span = info_span!(
                    "analytical",
                    iterations = iterations,
                    clique_threshold = clique_threshold
                )
                .entered();
                for _ in 0..*iterations {
                    // Analytic wirelength recovery phase
                    let mut strategy = ThresholdCrossover::new(
                        *clique_threshold,
                        Clique::new(),
                        AnchoredByNet::new(),
                    );

                    strategy.execute(cells)?;
                }
            }
        }
    }

    Ok(())
}

fn legalize_algorithm(config: &Config, netlist: &NetlistHypergraph) -> Vec<LegalizedCell> {
    TetrisLegalizer::new(config.legalizer.left_limit).legalize(&config.geometry, &netlist.cells)
}

fn place(config: &Config, design: Design) -> Result<PlacedDesign> {
    let (mut cells, creator) = load_cells(config, design).with_context(|| anyhow!("Load cells"))?;

    place_algorithm(&config, &mut cells)
        .with_context(|| anyhow!("Initial analytical placement"))?;

    let legalized_cells = legalize_algorithm(&config, &cells);

    Ok(cells.build_output(legalized_cells, creator))
}

fn run_placement(config: &Config) -> Result<()> {
    let design = load_design(config).with_context(|| anyhow!("Load design"))?;

    let placed_design = place(&config, design)
        .with_context(|| anyhow!("Place design from {:?}", config.io.input_file))?;

    {
        use std::io::Write;
        let mut outf = std::fs::File::create(&config.io.output_file).with_context(|| {
            anyhow!(
                "Failed to open/create output file {:?}",
                config.io.output_file
            )
        })?;
        let encoded = placed_design.encode_to_vec();

        outf.write_all(&encoded[..]).with_context(|| {
            anyhow!("Failed to write to output file {:?}", config.io.output_file)
        })?;
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
