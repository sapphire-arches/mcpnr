use std::{
    collections::HashMap,
    sync::{atomic::AtomicU64, Arc},
};

use bytemuck::{Pod, Zeroable};
use eframe::wgpu::{self, Device};
use egui::{Vec2, WidgetInfo};

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

        Self { id }
    }

    pub fn ui(&self, ui: &mut egui::Ui, size: Vec2) -> egui::Response {
        let (rect, response) = ui.allocate_at_least(size, egui::Sense::click_and_drag());

        // Accessiblity properties (mostly just a stub, this is a purely visual component...)
        response.widget_info(|| {
            let mut info = WidgetInfo::new(egui::WidgetType::Other);
            info.label = Some("Canvas".into());
            info
        });

        let nrects: u32 = 2;

        let rect_uniforms = RectangleUniforms {
            projection_view: [
                1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
            ],
            color: [1.0, 0.0, 1.0, 1.0],
        };

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
                    // then leading_zeros will return 12, and new_log_2 becomes (16) - 12 = 4
                    // and (1 << 4) = 16 (which is the smallest power of two > 9)
                    //
                    // This overestimates for exact powers of two but that shouldn't matter much
                    // in practice
                    //
                    // It would be much clearer if u64::log2 was stable but it's not
                    let rect_count_log2 = (std::mem::size_of_val(&rect_count) as u32 * 8u32)
                        - rect_count.leading_zeros();

                    let new_rect_count = 1 << rect_count_log2;

                    let (idx, vtx) = alloc_rect_buffers(device, new_rect_count);

                    local_resources.rect_index_buffer = idx;
                    local_resources.rect_vertex_buffer = vtx;
                }

                queue.write_buffer(
                    &local_resources.rect_vertex_buffer,
                    0,
                    bytemuck::cast_slice(&[
                        RectVertex {
                            pos: Vec2::new(-0.5, 0.5),
                        },
                        RectVertex {
                            pos: Vec2::new(0.5, 0.5),
                        },
                        RectVertex {
                            pos: Vec2::new(0.5, -0.5),
                        },
                        RectVertex {
                            pos: Vec2::new(-0.5, -0.5),
                        },
                        // rectangle 2
                        RectVertex {
                            pos: Vec2::new(-0.25, 0.25),
                        },
                        RectVertex {
                            pos: Vec2::new(0.25, 0.25),
                        },
                        RectVertex {
                            pos: Vec2::new(0.25, -0.25),
                        },
                        RectVertex {
                            pos: Vec2::new(-0.25, -0.25),
                        },
                    ]),
                );

                queue.write_buffer(
                    &local_resources.rect_index_buffer,
                    0,
                    bytemuck::cast_slice(&[
                        // rect 0
                        0u16, 1, 2, 3, 0, 0xffff,
                        // rect 1
                        4u16, 5, 6, 7, 4, 0xffff,
                    ]),
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
