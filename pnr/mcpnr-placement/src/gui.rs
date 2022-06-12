use std::{any::Any, sync::Arc};

use crate::Config;
use eframe::{App, CreationContext};
use wgpu::{Device, Queue};

#[derive(Default)]
struct RendererState {
    counter: u32,
}

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
        egui::CentralPanel::default().show(ctx, |ui| {
            let (rect, _response) =
                ui.allocate_at_least(egui::Vec2::splat(128.0), egui::Sense::click());
            let callback = egui_wgpu::CallbackFn::new()
                .prepare(|_device: &Device, _queue: &Queue, map| {
                    map.entry::<RendererState>()
                        .or_insert_with(Default::default)
                        .counter += 1;
                })
                .paint(|_info, _pass, resources| {
                    eprintln!("{:?}", resources.get::<RendererState>().unwrap().counter)
                });
            let callback = egui::PaintCallback {
                rect,
                callback: Arc::new(callback),
            };

            ui.painter().add(callback)
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
