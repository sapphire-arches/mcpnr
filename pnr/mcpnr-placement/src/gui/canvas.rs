use std::{
    collections::HashMap,
    sync::{atomic::AtomicU64, Arc},
};

use bytemuck::{Pod, Zeroable};
use eframe::wgpu::{self, Device};
use egui::{Vec2, Widget, WidgetInfo};
use nalgebra as na;

use crate::core::PlaceableCells;

/// Global render state used to cache pipelines
pub struct CanvasGlobalResources {
    /// Pipeline used to render the rectangles
    rects_pipeline: wgpu::RenderPipeline,
    /// Bind group layout for rectangle pipeline
    rects_bind_group_layout: wgpu::BindGroupLayout,
    // TODO: render lines for connections
    // lines_pipeline: wgpu::RenderPipeline
    /// Storage for per-canvas resources
    canvases: HashMap<CanvasId, CanvasRenderResources>,
}

#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, Pod, Zeroable)]
struct RectangleUniforms {
    projection_view: [f32; 16],
    color: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
struct RectVertex {
    pos: Vec2,
}

/// 1 vertex per corner
const VERTEX_PER_RECT: u64 = 4;
/// 6 indicies per rectangle, 1 for the provoking vertex of the strip, 4 to draw each line, and
/// then 1 for the reset
const INDEX_PER_RECT: u64 = 6;

type RectIndexType = u16;
const RECT_INDEX_FORMAT: wgpu::IndexFormat = wgpu::IndexFormat::Uint16;

/// Per-canvas render resources
struct CanvasRenderResources {
    rect_uniform_buffer: wgpu::Buffer,
    rect_bind_group: wgpu::BindGroup,

    rect_vertex_buffer: wgpu::Buffer,
    rect_index_buffer: wgpu::Buffer,
    /// Number of rectangle slots we have allocated in `Self::rect_vertex_buffer` and
    /// `Self::rect_index_buffer`
    rect_count: u64,
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
    cells: &'a PlaceableCells,
}

fn initialize_rects_pipeline(
    device: &Device,
    rs_target_format: wgpu::ColorTargetState,
) -> (wgpu::RenderPipeline, wgpu::BindGroupLayout) {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("canvas.rects.shader"),
        source: wgpu::ShaderSource::Wgsl(include_str!("./canvas_shaders.wgsl").into()),
    });

    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("canvas.rects.bind_group_layout"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }],
    });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("canvas.rects.pipeline_layout"),
        bind_group_layouts: &[&bind_group_layout],
        push_constant_ranges: &[],
    });

    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("canvas.rects.pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: "vs_main",
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<Vec2>() as wgpu::BufferAddress,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &wgpu::vertex_attr_array![
                    0 => Float32x2
                ],
            }],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: "fs_main",
            targets: &[Some(rs_target_format)],
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::LineStrip,
            strip_index_format: Some(RECT_INDEX_FORMAT),
            ..wgpu::PrimitiveState::default()
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview: None,
    });

    (pipeline, bind_group_layout)
}

impl CanvasGlobalResources {
    pub fn register(cc: &eframe::CreationContext) {
        let render_state = cc.render_state.as_ref().expect("WGPU enabled");

        let device = &render_state.device;

        let (rects_pipeline, rects_bind_group_layout) =
            initialize_rects_pipeline(device, render_state.target_format.into());

        render_state
            .egui_rpass
            .write()
            .paint_callback_resources
            .insert(Self {
                rects_pipeline,
                rects_bind_group_layout,
                canvases: Default::default(),
            });
    }
}

impl Canvas {
    pub fn new(cc: &eframe::CreationContext) -> Self {
        let render_state = cc.render_state.as_ref().expect("WGPU enabled");
        let mut rpass = render_state.egui_rpass.write();

        let global_resources: &mut CanvasGlobalResources =
            rpass.paint_callback_resources.get_mut().unwrap();

        let device = &render_state.device;

        let rect_uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("canvas.rects.uniforms"),
            size: std::mem::size_of::<RectangleUniforms>() as _,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let rect_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("canvas.rects.bind_group"),
            layout: &global_resources.rects_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: rect_uniform_buffer.as_entire_binding(),
            }],
        });

        let (rect_vertex_buffer, rect_index_buffer) = alloc_rect_buffers(device, 16);

        let render_resources = CanvasRenderResources {
            rect_uniform_buffer,
            rect_bind_group,
            rect_index_buffer,
            rect_vertex_buffer,
            rect_count: 16,
        };

        let id = CanvasId::allocate();

        global_resources.canvases.insert(id, render_resources);

        Self {
            id,
            pixels_per_unit: 16.0,
            center: Vec2::splat(0.0),
        }
    }

    fn render_canvas(&mut self, ui: &mut egui::Ui, cells: &PlaceableCells) -> egui::Response {
        let (rect, response) =
            ui.allocate_at_least(ui.available_size(), egui::Sense::click_and_drag());

        // Accessiblity properties (mostly just a stub, this is a purely visual component...)
        response.widget_info(|| {
            let mut info = WidgetInfo::new(egui::WidgetType::Other);
            info.label = Some("Canvas".into());
            info
        });

        if response.hovered() {
            let delta = ui.input().scroll_delta.y;

            const SCALE: f32 = 0.5;

            let factor = if delta > 0.0 {
                SCALE
            } else if delta < 0.0 {
                1.0 / SCALE
            } else {
                1.0
            };

            self.pixels_per_unit = f32::min(128.0, f32::max(1.0, self.pixels_per_unit * factor));
        }

        if response.dragged() {
            self.center += response.drag_delta() / self.pixels_per_unit;
        }

        // Compute the size in pixels
        let pixel_width = rect.width() * ui.ctx().pixels_per_point();
        let pixel_height = rect.height() * ui.ctx().pixels_per_point();

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

        let mut nrects: u32 = 0;
        let mut rect_vertex_data: Vec<RectVertex> = Vec::new();
        let mut rect_indicies: Vec<RectIndexType> = Vec::new();

        for cell in &cells.cells {
            let x = cell.x as f32;
            let y = cell.y as f32;
            let sx = cell.sx as f32;
            let sy = cell.sy as f32;

            let cell_rect = egui::Rect {
                min: (x - sx / 2.0, y - sy / 2.0).into(),
                max: (x + sx / 2.0, y + sy / 2.0).into(),
            };

            if cell_rect.intersects(clip_rect) {
                let base_idx: u16 = rect_vertex_data.len().try_into().unwrap();
                rect_vertex_data.push(RectVertex {
                    pos: Vec2::new(cell_rect.min.x, cell_rect.min.y),
                });
                rect_vertex_data.push(RectVertex {
                    pos: Vec2::new(cell_rect.min.x, cell_rect.max.y),
                });
                rect_vertex_data.push(RectVertex {
                    pos: Vec2::new(cell_rect.max.x, cell_rect.max.y),
                });
                rect_vertex_data.push(RectVertex {
                    pos: Vec2::new(cell_rect.max.x, cell_rect.min.y),
                });

                rect_indicies.push(base_idx + 0);
                rect_indicies.push(base_idx + 1);
                rect_indicies.push(base_idx + 2);
                rect_indicies.push(base_idx + 3);
                rect_indicies.push(base_idx + 0);
                rect_indicies.push(0xffff);

                nrects += 1;
            }
        }

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

        let mut rect_uniforms = RectangleUniforms {
            projection_view: [0.0; 16],
            color: [1.0, 0.0, 1.0, 1.0],
        };

        assert_eq!(projection_view.as_slice().len(), 16);
        for (i, f) in projection_view.as_slice().iter().enumerate() {
            rect_uniforms.projection_view[i] = *f;
        }

        let id = self.id;

        let cb = egui_wgpu::CallbackFn::new()
            .prepare(move |device, queue, paint_callback_resources| {
                let global_resources: &mut CanvasGlobalResources =
                    paint_callback_resources.get_mut().unwrap();

                let mut local_resources = global_resources.canvases.get_mut(&id).unwrap();

                let rect_count: u64 = nrects.into();
                if rect_count > local_resources.rect_count {
                    // Need to grow the buffers
                    // This is a cursed way of computing the next largest power of 2
                    // If we have a 16-bit number like this:
                    //     0000000000001001 = 9
                    // then leading_zeros will return 12, and new_log_2 becomes (16) - 12 + 2 = 5
                    // and (1 << 4) = 16 (which is the smallest power of two > 9)
                    //
                    // This overestimates for exact powers of two but that shouldn't matter much
                    // in practice
                    //
                    // It would be much clearer if u64::log2 was stable but it's not
                    let rect_count_log2 = (std::mem::size_of_val(&rect_count) as u32 * 8u32)
                        - rect_count.leading_zeros();

                    let new_rect_count = 1 << rect_count_log2;

                    let (vtx, idx) = alloc_rect_buffers(device, new_rect_count);

                    local_resources.rect_index_buffer = idx;
                    local_resources.rect_vertex_buffer = vtx;
                    local_resources.rect_count = new_rect_count;
                }

                queue.write_buffer(
                    &local_resources.rect_vertex_buffer,
                    0,
                    bytemuck::cast_slice(&rect_vertex_data),
                );

                queue.write_buffer(
                    &local_resources.rect_index_buffer,
                    0,
                    bytemuck::cast_slice(&rect_indicies),
                );

                queue.write_buffer(
                    &local_resources.rect_uniform_buffer,
                    0,
                    bytemuck::cast_slice(&[rect_uniforms]),
                );
            })
            .paint(move |_info, rpass, paint_callback_resources| {
                let global_resources: &CanvasGlobalResources =
                    paint_callback_resources.get().unwrap();

                let local_resources = global_resources.canvases.get(&id).unwrap();

                rpass.set_pipeline(&global_resources.rects_pipeline);
                rpass.set_bind_group(0, &local_resources.rect_bind_group, &[]);
                rpass.set_vertex_buffer(
                    0,
                    local_resources.rect_vertex_buffer.slice(
                        ..(nrects as u64
                            * VERTEX_PER_RECT
                            * std::mem::size_of::<RectVertex>() as u64),
                    ),
                );
                rpass.set_index_buffer(
                    local_resources.rect_index_buffer.slice(
                        ..(nrects as u64
                            * INDEX_PER_RECT
                            * std::mem::size_of::<RectIndexType>() as u64),
                    ),
                    RECT_INDEX_FORMAT,
                );
                rpass.draw_indexed(0..(nrects * INDEX_PER_RECT as u32), 0, 0..1);
            });

        ui.painter().add(egui::PaintCallback {
            rect,
            callback: Arc::new(cb),
        });

        response
    }
}

fn alloc_rect_buffers(device: &wgpu::Device, count: u64) -> (wgpu::Buffer, wgpu::Buffer) {
    let vertex = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("canvas.rects.vertex_buffer"),
        size: count * VERTEX_PER_RECT * std::mem::size_of::<RectVertex>() as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let index = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("canvas.rects.vertex_buffer"),
        size: count * INDEX_PER_RECT * std::mem::size_of::<RectIndexType>() as u64,
        usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    (vertex, index)
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
    pub fn new(canvas: &'a mut Canvas, cells: &'a PlaceableCells) -> Self {
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
