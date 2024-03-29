use std::{collections::HashMap, sync::atomic::AtomicU64};

use egui::{Key, Vec2, Widget, WidgetInfo};
use itertools::Itertools;
use nalgebra as na;

use crate::{
    core::NetlistHypergraph, placement_cell::LegalizedCell, placer::diffusion::DiffusionPlacer,
};

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

const RECT_IDX_CELLS: usize = 0;
const RECT_IDX_LEGAL: usize = 1;

/// Per-canvas render resources
struct CanvasRenderResources {
    /// Rectangle resources.
    /// TODO: make this optional, so we can have more specialized canvases and they're cheaper
    rectangle_resources: Vec<rectangles::RenderResources>,

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

    /// Maximum density observed
    density_max: f32,

    /// Minimum density observed
    density_min: f32,

    /// Selected layer
    selected_layer: usize,
}

/// Ephermeral state, for use with `egui::Ui::add`
pub struct CanvasWidget<'a> {
    canvas: &'a mut Canvas,
    cells: &'a NetlistHypergraph,
    diffusion: Option<&'a DiffusionPlacer>,
    legalized_cells: Option<&'a [LegalizedCell]>,
}

impl CanvasGlobalResources {
    pub fn register(cc: &eframe::CreationContext) {
        let render_state = cc.wgpu_render_state.as_ref().expect("WGPU enabled");

        let device = &render_state.device;

        render_state
            .egui_rpass
            .write()
            .paint_callback_resources
            .insert(Self {
                rectangle: rectangles::GlobalResources::new(
                    device,
                    render_state.target_format.into(),
                ),
                line: lines::GlobalResources::new(device, render_state.target_format.into()),
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
            rectangle_resources: vec![
                global_resources.rectangle.create_local(device),
                global_resources.rectangle.create_local(device),
            ],
            line: global_resources.line.create_local(device),
        };

        global_resources.canvases.insert(id, render_resources);

        Self {
            id,
            pixels_per_unit: 16.0,
            center: Vec2::splat(0.0),
            density_max: 0.0,
            density_min: 0.0,
            selected_layer: 0,
        }
    }

    fn render_canvas(
        &mut self,
        ui: &mut egui::Ui,
        cells: &NetlistHypergraph,
        diffusion: Option<&DiffusionPlacer>,
        legalized_cells: Option<&[LegalizedCell]>,
    ) -> egui::Response {
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

            let max_layers = diffusion.map(|d| d.density.shape()[1]).unwrap_or(1);
            self.selected_layer = if input.key_pressed(Key::Q) {
                self.selected_layer + 1
            } else if input.key_pressed(Key::E) {
                self.selected_layer + max_layers - 1
            } else {
                self.selected_layer
            };
            self.selected_layer %= max_layers;

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

        let (density_min, density_max) = diffusion
            .and_then(|d| {
                d.density
                    .iter()
                    .copied()
                    .minmax_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
                    .into_option()
            })
            .unwrap_or((0.0, 0.0));

        self.density_max = density_max;
        self.density_min = density_min;

        let selected_layer = self.selected_layer;

        self.render_lines(
            ui,
            projection_view,
            render_rect,
            clip_rect,
            // Render signals
            cells
                .signals
                .iter()
                .flat_map(|signal| {
                    signal
                        .connected_cells
                        .iter()
                        .map(|cell| {
                            let center = &cells.cells[*cell].center_pos();
                            lines::Vertex {
                                color: egui::Color32::RED,
                                position: (center.x, center.z),
                            }
                        })
                        .tuple_windows()
                })
                .chain(diffusion.into_iter().flat_map(|diffusion| {
                    let shape = diffusion.density.shape();
                    let scale = diffusion.region_size as f32;
                    let size_x = shape[0] - 2;
                    let size_y = shape[2] - 2;

                    // Vertical lines for the diffusion placement grid
                    (0..=(size_x + 2))
                        .map(move |x| {
                            let x = (x as f32) - 1.0;

                            (
                                lines::Vertex {
                                    color: egui::Color32::GREEN,
                                    position: (x * scale, -1.0 * scale),
                                },
                                lines::Vertex {
                                    color: egui::Color32::GREEN,
                                    position: (x * scale, ((size_y as f32) + 1.0) * scale),
                                },
                            )
                        })
                        // Horizontal lines for the diffusion placement grid
                        .chain((0..=(size_y + 2)).map(move |y| {
                            let y = (y as f32) - 1.0;

                            (
                                lines::Vertex {
                                    color: egui::Color32::GREEN,
                                    position: (-1.0 * scale, y * scale),
                                },
                                lines::Vertex {
                                    color: egui::Color32::GREEN,
                                    position: (((size_x as f32) + 1.0) * scale, y * scale),
                                },
                            )
                        }))
                        // Velocity rendering
                        .chain(diffusion.vel_x.indexed_iter().map(|(pos, x_vel)| {
                            let pos_scale = diffusion.region_size as f32;
                            let z_vel = diffusion.vel_z[pos];

                            let base_pos = (pos.0 as f32 * pos_scale, pos.2 as f32 * pos_scale);
                            const SCALE: f32 = 5.0;

                            (
                                lines::Vertex {
                                    color: egui::Color32::KHAKI,
                                    position: base_pos,
                                },
                                lines::Vertex {
                                    color: egui::Color32::KHAKI,
                                    position: (
                                        base_pos.0 + x_vel * SCALE,
                                        base_pos.1 + z_vel * SCALE,
                                    ),
                                },
                            )
                        }))
                        .chain((0..(size_x + 2) * (size_y + 2)).map(move |xy| {
                            let x = xy / (size_y + 2);
                            let y = xy % (size_y + 2);
                            let fill_level =
                                (diffusion.density.get((x, selected_layer, y)).unwrap()
                                    - density_min)
                                    / (density_max - density_min);

                            let x = ((x as f32) - 0.5) * scale;
                            let y = ((y as f32) - 0.9) * scale;

                            (
                                lines::Vertex {
                                    color: egui::Color32::GREEN,
                                    position: (x, y),
                                },
                                lines::Vertex {
                                    color: egui::Color32::GOLD,
                                    position: (x, y + fill_level),
                                },
                            )
                        }))
                })),
        );

        self.render_rectangles(
            ui,
            projection_view,
            render_rect,
            clip_rect,
            // Cell rendering
            egui::Color32::from_rgba_unmultiplied(255, 0, 255, 255),
            RECT_IDX_CELLS,
            cells.cells.iter().map(|cell| egui::Rect {
                min: (cell.x, cell.z).into(),
                max: (cell.x + cell.sx, cell.z + cell.sz).into(),
            }),
        );

        if let Some(cells) = legalized_cells {
            self.render_rectangles(
                ui,
                projection_view,
                render_rect,
                clip_rect,
                egui::Color32::from_rgba_unmultiplied(0, 255, 255, 255),
                RECT_IDX_LEGAL,
                cells.iter().filter_map(|cell| {
                    if cell.tier_y as usize == self.selected_layer {
                        const INSET: f32 = 0.05;
                        Some(egui::Rect {
                            min: (cell.x as f32 + INSET, cell.z as f32 + INSET).into(),
                            max: (
                                (cell.x + cell.sx) as f32 - INSET,
                                (cell.z + cell.sz) as f32 - INSET,
                            )
                                .into(),
                        })
                    } else {
                        None
                    }
                }),
            )
        }

        response
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
    pub fn new(
        canvas: &'a mut Canvas,
        cells: &'a NetlistHypergraph,
        diffusion: Option<&'a DiffusionPlacer>,
        legalized_cells: Option<&'a [LegalizedCell]>,
    ) -> Self {
        Self {
            canvas,
            cells,
            diffusion,
            legalized_cells,
        }
    }
}

impl<'a> Widget for CanvasWidget<'a> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        ui.allocate_ui_with_layout(
            ui.available_size(),
            egui::Layout::bottom_up(egui::Align::Min),
            |ui| {
                let info_string = format!(
                    "Scale: {:.02} X: {:0.2} Y: {:0.2}, min/max density: {:}/{:}, layer: {}",
                    self.canvas.pixels_per_unit,
                    self.canvas.center.x,
                    self.canvas.center.y,
                    self.canvas.density_min,
                    self.canvas.density_max,
                    self.canvas.selected_layer,
                );
                ui.label(info_string);

                ui.horizontal(|ui| match self.diffusion.map(|m| m.density.shape()) {
                    Some(diffusion_shape) => {
                        if ui.small_button("+").clicked() {
                            if self.canvas.selected_layer + 1 < diffusion_shape[1] {
                                self.canvas.selected_layer += 1;
                            } else {
                                self.canvas.selected_layer = 0;
                            }
                        }
                        ui.label(format!("{}", self.canvas.selected_layer));
                        if ui.small_button("-").clicked() {
                            if self.canvas.selected_layer > 0 {
                                self.canvas.selected_layer -= 1;
                            } else {
                                self.canvas.selected_layer = diffusion_shape[1] - 1;
                            }
                        }
                    }
                    None => {}
                });

                egui::Frame::canvas(ui.style())
                    .show(ui, |ui| {
                        self.canvas.render_canvas(
                            ui,
                            self.cells,
                            self.diffusion,
                            self.legalized_cells,
                        )
                    })
                    .inner
            },
        )
        .inner
    }
}
