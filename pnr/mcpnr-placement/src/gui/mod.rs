use crate::{Config, load_design, load_cells, core::PlaceableCells};
use eframe::{App, CreationContext};
use log::info;
use anyhow::Result;

use self::canvas::{Canvas, CanvasGlobalResources, CanvasWidget};

mod canvas;

struct UIState {
    cells: PlaceableCells,
    creator: String,

    // UI state
    do_debug_render: bool,
    primary_canvas: Canvas,
}

impl UIState {
    fn new(cells: PlaceableCells, creator: String, cc: &CreationContext) -> Self {
        CanvasGlobalResources::register(cc);

        Self {
            cells, creator,
            do_debug_render: false,
            primary_canvas: Canvas::new(cc),
        }
    }
}

impl App for UIState {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        egui::SidePanel::right("debug_panel").show(ctx, |ui| {
            ui.collapsing("EGUI inspection", |ui| {
                ui.checkbox(&mut self.do_debug_render, "Do debug rendering");
                ctx.set_debug_on_hover(self.do_debug_render);
                ctx.inspection_ui(ui);
            });

            ui.with_layout(egui::Layout::bottom_up(egui::Align::Max), |ui| {
                if ui.button("quit").clicked() {
                    frame.quit();
                }
            })
        });
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add(CanvasWidget::new(&mut self.primary_canvas, &self.cells));
        });
    }
}

pub(crate) fn run_gui(config: &Config) -> Result<()> {
    let design = load_design(config)?;
    let (cells, creator) = load_cells(config, design)?;

    eframe::run_native(
        "mcpnr placement",
        eframe::NativeOptions::default(),
        Box::new(|cc| Box::new(UIState::new(cells, creator, cc))),
    )
}
