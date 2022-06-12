use crate::Config;
use eframe::{App, CreationContext};

#[derive(Default)]
struct UIState {
    do_debug_render: bool,
}

impl UIState {
    fn new(_cc: &CreationContext) -> Self {
        Self::default()
    }
}

impl App for UIState {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        egui::SidePanel::right("debug_panel").show(ctx, |ui| {
            ui.collapsing("EGUI inspection", |ui| {
                ui.checkbox(&mut self.do_debug_render, "Do debug rendering");
                ctx.set_debug_on_hover(self.do_debug_render);
                ctx.inspection_ui(ui);
            })
        });
    }
}

pub(crate) fn run_gui(config: &Config) {
    eframe::run_native(
        "mcpnr placement",
        eframe::NativeOptions::default(),
        Box::new(|cc| Box::new(UIState::new(cc))),
    )
}
