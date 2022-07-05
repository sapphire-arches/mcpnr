use crate::Config;
use eframe::{App, CreationContext};
use log::info;

use self::canvas::{Canvas, CanvasGlobalResources};

mod canvas;

struct UIState {
    do_debug_render: bool,

    primary_canvas: Canvas,
}

impl UIState {
    fn new(cc: &CreationContext) -> Self {
        CanvasGlobalResources::register(cc);

        Self {
            do_debug_render: false,
            primary_canvas: Canvas::new(cc),
        }
    }
}

impl App for UIState {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        let panel = egui::SidePanel::right("debug_panel").show(ctx, |ui| {
            ui.collapsing("EGUI inspection", |ui| {
                ui.checkbox(&mut self.do_debug_render, "Do debug rendering");
                ctx.set_debug_on_hover(self.do_debug_render);
                ctx.inspection_ui(ui);
            })
        });
        egui::CentralPanel::default().show(ctx, |ui| {
            self.primary_canvas.ui(ui, ui.available_size());
        });
    }
}

pub(crate) fn run_gui(_config: &Config) {
    eframe::run_native(
        "mcpnr placement",
        eframe::NativeOptions::default(),
        Box::new(|cc| Box::new(UIState::new(cc))),
    )
}
