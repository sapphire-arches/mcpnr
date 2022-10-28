use std::{collections::HashMap, sync::atomic::AtomicU64};

use eframe::wgpu;
use egui::{Key, Vec2, Widget, WidgetInfo};
use itertools::Itertools;
use nalgebra as na;

use crate::core::NetlistHypergraph;

mod lines;
mod rectangles;
mod shader;

/// Global render state used to cache pipelines
pub struct CanvasGlobalResources {
    /// Global resources for rendering rectangles
    rectangle: rectangles::GlobalResources,
    /// Global resources for rendering lines
    line: lines::GlobalResources,
    /// Storage for per-canvas resources
    canvases: HashMap<CanvasId, CanvasRenderResources>,
}

/// Per-canvas render resources
struct CanvasRenderResources {
    /// Rectangle resources.
    /// TODO: make this optional, so we can have more specialized canvases and they're cheaper
    rectangle: rectangles::RenderResources,

    /// Line resources.
    /// TODO: make this optional, so we can have more specialized canvases and they're cheaper
    line: lines::RenderResources,
}

#[repr(transparent)]
#[derive(Copy, Clone, PartialEq, Eq, Hash)]
struct CanvasId(u64);

/// Canvas for painting on
pub struct Canvas {
    id: CanvasId,

    /// Scale factor from internal units to pixels
    pixels_per_unit: f32,

    /// Center location for the render
    center: Vec2,
}

/// Ephermeral state, for use with `egui::Ui::add`
pub struct CanvasWidget<'a> {
    canvas: &'a mut Canvas,
    cells: &'a NetlistHypergraph,
}

impl CanvasGlobalResources {
    pub fn register(cc: &eframe::CreationContext) {
        let render_state = cc.wgpu_render_state.as_ref().expect("WGPU enabled");

        let device = &render_state.device;

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("canvas.shader"),
            source: wgpu::ShaderSource::Wgsl(shader::SOURCE.into()),
        });

        render_state
            .egui_rpass
            .write()
            .paint_callback_resources
            .insert(Self {
                rectangle: rectangles::GlobalResources::new(
                    device,
                    &shader,
                    render_state.target_format.into(),
                ),
                line: lines::GlobalResources::new(
                    device,
                    &shader,
                    render_state.target_format.into(),
                ),
                canvases: Default::default(),
            });
    }
}

impl Canvas {
    pub fn new(cc: &eframe::CreationContext) -> Self {
        let render_state = cc.wgpu_render_state.as_ref().expect("WGPU enabled");
        let mut rpass = render_state.egui_rpass.write();

        let global_resources: &mut CanvasGlobalResources =
            rpass.paint_callback_resources.get_mut().unwrap();

        let device = &render_state.device;

        let id = CanvasId::allocate();

        let render_resources = CanvasRenderResources {
            rectangle: global_resources.rectangle.create_local(device),
            line: global_resources.line.create_local(device),
        };

        global_resources.canvases.insert(id, render_resources);

        Self {
            id,
            pixels_per_unit: 16.0,
            center: Vec2::splat(0.0),
        }
    }

    fn render_canvas(&mut self, ui: &mut egui::Ui, cells: &NetlistHypergraph) -> egui::Response {
        let (render_rect, response) =
            ui.allocate_at_least(ui.available_size(), egui::Sense::click_and_drag());

        // Accessiblity properties (mostly just a stub, this is a purely visual component...)
        response.widget_info(|| {
            let mut info = WidgetInfo::new(egui::WidgetType::Other);
            info.label = Some("Canvas".into());
            info
        });

        if response.hovered() {
            let input = ui.input();

            let delta = if input.key_pressed(Key::R) {
                -1.0
            } else if input.key_pressed(Key::F) {
                1.0
            } else {
                input.scroll_delta.y
            };

            const SCALE: f32 = 0.5;

            let factor = if delta > 0.0 {
                SCALE
            } else if delta < 0.0 {
                1.0 / SCALE
            } else {
                1.0
            };

            self.pixels_per_unit = f32::min(128.0, f32::max(1.0, self.pixels_per_unit * factor));

            // Keyboard controls
            const KEY_SCROLL: f32 = 64.0;
            if input.key_pressed(Key::W) {
                self.center.y += KEY_SCROLL / self.pixels_per_unit;
            }
            if input.key_pressed(Key::S) {
                self.center.y -= KEY_SCROLL / self.pixels_per_unit;
            }
            if input.key_pressed(Key::A) {
                self.center.x += KEY_SCROLL / self.pixels_per_unit;
            }
            if input.key_pressed(Key::D) {
                self.center.x -= KEY_SCROLL / self.pixels_per_unit;
            }
        }

        if response.dragged() {
            self.center += response.drag_delta() / self.pixels_per_unit;
        }

        // Compute the size in pixels
        let pixel_width = render_rect.width() * ui.ctx().pixels_per_point();
        let pixel_height = render_rect.height() * ui.ctx().pixels_per_point();

        //
        // Extract the rectangles we should render
        //
        // Compute clip border in internal units
        let clip_width = pixel_width / self.pixels_per_unit;
        let clip_height = pixel_height / self.pixels_per_unit;

        let clip_rect = egui::Rect {
            min: (
                self.center.x - clip_width / 2.0,
                self.center.y - clip_height / 2.0,
            )
                .into(),
            max: (
                self.center.x + clip_width / 2.0,
                self.center.y + clip_height / 2.0,
            )
                .into(),
        };

        // Compute the transform matrix based on the egui rectangle and a scale factor
        let projection_view = na::Translation3::new(-self.center.x, -self.center.y, 0.0);
        // The output of projection_view will be scaled by rect.width() and rect.height() from [-1,
        // 1] on both axes by the viewport transform. Therefore internal units must be scaled by a
        // factor of (2.0 / rect.width()) to get 1 unit = 1 pixel, and then multiplied by
        // pixels_per_unit to get 1 unit = pixels_per_unit pixels
        let projection_view = na::Scale3::new(
            (-2.0 / pixel_width) * self.pixels_per_unit,
            (2.0 / pixel_height) * self.pixels_per_unit,
            1.0,
        )
        .to_homogeneous()
            * projection_view.to_homogeneous();

        self.render_cells(cells, ui, projection_view, render_rect, clip_rect);
        self.render_signals(cells, ui, projection_view, render_rect, clip_rect);

        response
    }

    fn render_cells(
        &mut self,
        cells: &NetlistHypergraph,
        ui: &mut egui::Ui,
        projection_view: na::Matrix4<f32>,
        render_rect: egui::Rect,
        clip_rect: egui::Rect,
    ) {
        self.render_rectangles(
            ui,
            projection_view,
            render_rect,
            clip_rect,
            cells.cells.iter().map(|cell| egui::Rect {
                min: (cell.x, cell.z).into(),
                max: (cell.x + cell.sx, cell.z + cell.sz).into(),
            }),
        );
    }

    fn render_signals(
        &mut self,
        cells: &NetlistHypergraph,
        ui: &mut egui::Ui,
        projection_view: na::Matrix4<f32>,
        render_rect: egui::Rect,
        clip_rect: egui::Rect,
    ) {
        self.render_lines(
            ui,
            projection_view,
            render_rect,
            clip_rect,
            cells.signals.iter().flat_map(|signal| {
                signal
                    .connected_cells
                    .iter()
                    .map(|cell| {
                        let center = &cells.cells[*cell].center_pos();
                        (center.x, center.z)
                    })
                    .tuple_windows()
            }),
        );
    }
}

/// CanvasId counter
static CANVAS_ID_COUNTER: AtomicU64 = AtomicU64::new(0);

impl CanvasId {
    fn allocate() -> Self {
        // technically this can wrap, but 2^64 is a very large number
        Self(CANVAS_ID_COUNTER.fetch_add(1, std::sync::atomic::Ordering::AcqRel))
    }
}

impl<'a> CanvasWidget<'a> {
    pub fn new(canvas: &'a mut Canvas, cells: &'a NetlistHypergraph) -> Self {
        Self { canvas, cells }
    }
}

impl<'a> Widget for CanvasWidget<'a> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        ui.allocate_ui_with_layout(
            ui.available_size(),
            egui::Layout::bottom_up(egui::Align::Min),
            |ui| {
                let info_string = format!(
                    "Scale: {:.02} X: {:0.2} Y: {:0.2}",
                    self.canvas.pixels_per_unit, self.canvas.center.x, self.canvas.center.y,
                );
                ui.label(info_string);

                egui::Frame::canvas(ui.style())
                    .show(ui, |ui| self.canvas.render_canvas(ui, self.cells))
                    .inner
            },
        )
        .inner
    }
}