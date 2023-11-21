//! Global registry for configuration of the various placement stages.
//!

use anyhow::{Context, Result};
use std::path::PathBuf;

/// Configuration variables related to input/output operations
#[derive(Clone, Debug)]
pub struct IOConfig {
    /// Input file name (a protobuf-formatted yosys design)
    pub input_file: PathBuf,
    /// Output file name (a mcpnr placement file)
    pub output_file: PathBuf,
    /// Directory of the structure database, derviced from the path to the technology library.
    pub structure_directory: PathBuf,
}

/// Geometry of the placement region
#[derive(Clone, Debug)]
pub struct GeometryConfig {
    /// Size of the region along the x axis, in blocks
    pub size_x: u32,
    /// Size of the region along the y axis, in *layers*
    pub size_y: u32,
    /// Size of the region along the z axis, in blocks
    pub size_z: u32,
    /// Desired overall normalized density of the placement, in the range 0-1
    pub target_fill: f32,
}

/// Configuration of the diffusion placer
#[derive(Clone, Debug)]
pub struct DiffusionConfig {
    /// Number of blocks per region.
    pub region_size: u32,
    /// Total amount of internal timesteps for 1 diffusion step
    pub iterations: u32,
    /// How much virtual time we should elapse per internal timestep
    pub delta_t: f32,
}

/// Overall schedule for the placement strategy
///
/// TODO: we probably want some sort of dynamic scheduling, where the system pays attention to
/// factors like cell overlap and whitespace to move between different placement strategies. Fixed
/// schedules are probably fine for now though
#[derive(Clone, Debug)]
pub struct PlacementSchedule {
    pub schedule: Vec<PlacementStep>,
}

/// An individual step in the placement schedule
#[derive(Clone, Debug)]
pub enum PlacementStep {
    /// Center all moveable cells in the placement region. This helps avoid the IO cells clumping
    /// everything in to one of the edges.
    CenterCells,
    /// Basic unconstrained wirelength optimization
    UnconstrainedAnalytical {
        /// The threshold at which we switch from a clique model to a moveable star model in the
        /// placement.
        clique_threshold: usize,
    },
    /// Diffusion placement step, consisting of the actual diffusion and a constrained wirelength
    /// recovery step
    Diffusion(DiffusionConfig),
    /// Analytical wirelength recovery step, constrained by net anchors
    ConstrainedAnalytical {
        /// Threshold for switching between clique model and net-anchored model
        /// wirelength recovery step.
        clique_threshold: usize,
        /// Number of iterations to run
        iterations: usize,
    },
}

/// Overall placement configuration
#[derive(Clone, Debug)]
pub struct Config {
    pub io: IOConfig,
    pub geometry: GeometryConfig,
    pub schedule: PlacementSchedule,
}

impl Config {
    /// Construct a baseline configuration from the clap argument matches
    pub fn from_args(matches: &clap::ArgMatches) -> Result<Self> {
        let techlib_directory = PathBuf::from(matches.value_of_os("TECHLIB").unwrap());
        let clique_threshold = 2;
        let diffusion_config = DiffusionConfig {
            region_size: 2,
            iterations: 512,
            delta_t: 0.1,
        };

        Ok(Config {
            io: IOConfig {
                input_file: PathBuf::from(matches.value_of_os("INPUT").unwrap()),
                output_file: PathBuf::from(matches.value_of_os("OUTPUT").unwrap()),
                structure_directory: techlib_directory.join("structures"),
            },
            geometry: GeometryConfig {
                size_x: matches
                    .value_of("SIZE_X")
                    .unwrap()
                    .parse()
                    .context("Parse SIZE_X")?,
                size_y: matches
                    .value_of("SIZE_Y")
                    .unwrap()
                    .parse()
                    .context("Parse SIZE_Y")?,
                size_z: matches
                    .value_of("SIZE_Z")
                    .unwrap()
                    .parse()
                    .context("Parse SIZE_Z")?,
                target_fill: 0.8,
            },
            schedule: PlacementSchedule {
                schedule: vec![
                    // Initial unconstrained placement
                    PlacementStep::UnconstrainedAnalytical { clique_threshold },
                    // Center cells as setup for diffusion
                    PlacementStep::CenterCells,
                    // Main diffusion steps
                    PlacementStep::Diffusion(diffusion_config.clone()),
                    PlacementStep::ConstrainedAnalytical {
                        clique_threshold,
                        iterations: 2,
                    },
                    PlacementStep::Diffusion(diffusion_config.clone()),
                    PlacementStep::ConstrainedAnalytical {
                        clique_threshold,
                        iterations: 2,
                    },
                    PlacementStep::Diffusion(diffusion_config.clone()),
                    PlacementStep::ConstrainedAnalytical {
                        clique_threshold,
                        iterations: 2,
                    },
                    PlacementStep::Diffusion(diffusion_config.clone()),
                    PlacementStep::ConstrainedAnalytical {
                        clique_threshold,
                        iterations: 1,
                    },
                    PlacementStep::Diffusion(DiffusionConfig {
                        region_size: 2,
                        iterations: 64,
                        delta_t: 0.05,
                    }),
                ],
            },
        })
    }
}
