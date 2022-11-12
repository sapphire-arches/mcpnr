use crate::{
    core::NetlistHypergraph,
    load_cells, load_design, place_algorithm,
    placer::{
        analytical::{
            AnchoredByNet, Clique, DecompositionStrategy, MoveableStar, ThresholdCrossover,
        },
        diffusion::DiffusionPlacer,
    },
    Config,
};
use anyhow::{Context, Result};
use eframe::{App, CreationContext};
use egui::Ui;
use log::error;
use tracing::info_span;

use self::canvas::{Canvas, CanvasGlobalResources, CanvasWidget};

mod canvas;

struct UIState {
    config: Config,
    cells: NetlistHypergraph,
    creator: String,

    // UI state
    do_debug_render: bool,
    primary_canvas: Canvas,

    diffusion_config: DiffusionConfig,
    diffusion_placer: DiffusionPlacer,
}

impl UIState {
    fn new(
        config: Config,
        cells: NetlistHypergraph,
        creator: String,
        cc: &CreationContext,
    ) -> Self {
        CanvasGlobalResources::register(cc);

        // TODO: use this placer for the actual diffusion placement
        let diffusion_placer = DiffusionPlacer::new(
            // TODO: plumb through the error here instead of unwrapping Probably requires
            // implementing eframe::app for Result<UIState> or something like that
            config.size_x.try_into().unwrap(),
            config.size_y.try_into().unwrap(),
            config.size_z.try_into().unwrap(),
            0.2,
            4,
        );

        Self {
            config,
            cells,
            creator,
            do_debug_render: false,
            primary_canvas: Canvas::new(cc),

            diffusion_config: DiffusionConfig {
                step_size: 0.1,
                iterations: 32,
            },
            diffusion_placer,
        }
    }
}

impl App for UIState {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        egui::TopBottomPanel::top("title_panel").show(ctx, |ui| {
            ui.label(&self.creator);
        });
        egui::SidePanel::right("debug_panel").show(ctx, |ui| {
            if ui.button("Run placement").clicked() {
                match place_algorithm(&self.config, &mut self.cells) {
                    Ok(_) => {}
                    Err(e) => log::error!("Placement failure: {:?}", e),
                };
            }

            ui.group(|ui| {
                if ui.button("Unconstrained Analytical").clicked() {
                    let mut strategy =
                        ThresholdCrossover::new(4, Clique::new(), MoveableStar::new());
                    match strategy.execute(&mut self.cells) {
                        Ok(_) => {}
                        Err(e) => log::error!("Unconstrained analytical failure: {:?}", e),
                    };
                }
            });

            ui.group(|ui| {
                if ui.button("Constrained Analytical").clicked() {
                    let mut strategy =
                        ThresholdCrossover::new(2, Clique::new(), AnchoredByNet::new());
                    match strategy.execute(&mut self.cells) {
                        Ok(_) => {}
                        Err(e) => log::error!("Constrained analytical failure: {:?}", e),
                    };
                }
            });

            ui.group(|ui| {
                self.diffusion_config
                    .run_ui(ui, &self.config, &mut self.cells);
            });

            ui.collapsing("EGUI inspection", |ui| {
                ui.checkbox(&mut self.do_debug_render, "Do debug rendering");
                ctx.set_debug_on_hover(self.do_debug_render);
                ctx.inspection_ui(ui);
            });

            ui.with_layout(egui::Layout::bottom_up(egui::Align::Max), |ui| {
                if ui.button("quit").clicked() {
                    frame.close();
                }
            })
        });
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add(CanvasWidget::new(
                &mut self.primary_canvas,
                &self.cells,
                &self.diffusion_placer,
            ));
        });
    }
}

pub(crate) fn run_gui(config: &Config) -> Result<()> {
    let config = config.clone();
    let design = load_design(&config)?;
    let (cells, creator) = load_cells(&config, design)?;

    eframe::run_native(
        "mcpnr placement",
        eframe::NativeOptions::default(),
        Box::new(|cc| Box::new(UIState::new(config, cells, creator, cc))),
    );

    Ok(())
}

struct DiffusionConfig {
    step_size: f32,
    iterations: u32,
}

impl DiffusionConfig {
    fn run_ui(&mut self, ui: &mut Ui, config: &Config, cells: &mut NetlistHypergraph) {
        ui.label("Diffusion placement");
        ui.add(egui::Slider::new(&mut self.step_size, 0.01..=0.5).logarithmic(true));
        ui.add(egui::Slider::new(&mut self.iterations, 1..=128));

        if ui.button("Run").clicked() {
            match run_density(config, cells, self.iterations, self.step_size) {
                Ok(()) => {}
                Err(e) => error!("Failed to run density: {:?}", e),
            };
        }
    }
}

fn run_density(
    config: &Config,
    cells: &mut NetlistHypergraph,
    iterations: u32,
    step_size: f32,
) -> Result<()> {
    let _span = info_span!("diffusion").entered();

    let mut density = DiffusionPlacer::new(
        config.size_x.try_into().context("Convert X size")?,
        config.size_y.try_into().context("Convert Y size")?,
        config.size_z.try_into().context("Convert Z size")?,
        0.2,
        2,
    );

    density.splat(cells);

    for iteration in 0..iterations {
        let _span = info_span!("diffusion_iteration", iteration = iteration).entered();

        density.compute_velocities();
        density.move_cells(cells, step_size);
        density.step_time(step_size);
    }

    Ok(())
}
