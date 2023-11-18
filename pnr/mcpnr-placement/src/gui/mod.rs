use crate::{
    center_all_moveable_cells,
    config::DiffusionConfig,
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
use anyhow::Result;
use eframe::{App, CreationContext};
use egui::Ui;
use tracing::info_span;

use self::canvas::{Canvas, CanvasGlobalResources, CanvasWidget};

mod canvas;

struct DiffusionUIState {
    diffusion_config: DiffusionConfig,
    diffusion_placer: DiffusionPlacer,
}

struct UIState {
    // Global configuration
    config: Config,

    // Diffusion placer state, if we're experimenting with one
    diffusion_state: Option<DiffusionUIState>,

    // Net list properties
    cells: NetlistHypergraph,
    creator: String,

    // UI state
    do_debug_render: bool,
    primary_canvas: Canvas,
}

impl DiffusionUIState {
    fn ui(&mut self, ui: &mut Ui, cells: &mut NetlistHypergraph) {
        ui.label("Diffusion placement");
        ui.add(egui::Slider::new(&mut self.diffusion_config.delta_t, 0.01..=0.5).logarithmic(true));
        ui.add(egui::Slider::new(
            &mut self.diffusion_config.iteration_count,
            1..=1024,
        ));

        ui.horizontal(|ui| {
            if ui.button("Resplat").clicked() {
                self.diffusion_placer.splat(cells)
            }

            if ui.button("SingleStep").clicked() {
                self.diffusion_placer.compute_velocities();
                self.diffusion_placer
                    .move_cells(cells, self.diffusion_config.delta_t);
                self.diffusion_placer
                    .step_time(self.diffusion_config.delta_t);
            }
        });

        if ui.button("Run").clicked() {
            let _span = info_span!("diffusion").entered();

            self.diffusion_placer.splat(cells);

            for iteration in 0..self.diffusion_config.iteration_count {
                let _span = info_span!("diffusion_iteration", iteration = iteration).entered();

                self.diffusion_placer.compute_velocities();
                self.diffusion_placer
                    .move_cells(cells, self.diffusion_config.delta_t);
                self.diffusion_placer
                    .step_time(self.diffusion_config.delta_t);
            }
        }
    }
}

impl UIState {
    fn new(
        config: Config,
        cells: NetlistHypergraph,
        creator: String,
        cc: &CreationContext,
    ) -> Self {
        CanvasGlobalResources::register(cc);

        let diffusion_config = DiffusionConfig {
            region_size: 2,
            iteration_count: 128,
            delta_t: 0.1,
        };

        let diffusion_placer = DiffusionPlacer::new(&config, &diffusion_config);

        Self {
            config,

            diffusion_state: Some(DiffusionUIState {
                diffusion_config,
                diffusion_placer,
            }),

            cells,
            creator,
            do_debug_render: false,
            primary_canvas: Canvas::new(cc),
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

            if ui.button("Center Cells").clicked() {
                center_all_moveable_cells(&self.config, &mut self.cells);
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
                let mut checked = self.diffusion_state.is_some();
                ui.checkbox(&mut checked, "Diffusion Placer");

                if checked {
                    if self.diffusion_state.is_none() {
                        let diffusion_config = DiffusionConfig {
                            region_size: 2,
                            iteration_count: 128,
                            delta_t: 0.1,
                        };

                        let diffusion_placer =
                            DiffusionPlacer::new(&self.config, &diffusion_config);
                        self.diffusion_state = Some(DiffusionUIState {
                            diffusion_config,
                            diffusion_placer,
                        })
                    }

                    // Unwrap safety: if the diffusion state was none above, we just assigned to it
                    self.diffusion_state
                        .as_mut()
                        .unwrap()
                        .ui(ui, &mut self.cells);
                } else {
                    self.diffusion_state = None;
                }
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
                self.diffusion_state.as_ref().map(|x| &x.diffusion_placer),
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
