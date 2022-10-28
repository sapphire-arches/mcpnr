use eframe::wgpu::{self, Device};
use nalgebra as na;
use std::sync::Arc;

use crate::gui::canvas::CanvasGlobalResources;

use super::{shader, Canvas};

/********************************************************************************
 * Rendering types and constants
********************************************************************************/

/// Type for rectangle indicies
type IndexType = u16;
/// Matching index format for [`IndexType`]
const RECT_INDEX_FORMAT: wgpu::IndexFormat = wgpu::IndexFormat::Uint16;

type Uniforms = shader::Uniforms;
type Vertex = shader::Vertex;

/// 1 vertex per corner
const VERTEX_PER_RECT: u64 = 4;
/// 6 indicies per rectangle, 1 for the provoking vertex of the strip, 4 to draw each line, and
/// then 1 for the reset
const INDEX_PER_RECT: u64 = 6;

/********************************************************************************
 * Implementation
********************************************************************************/

/// wgpu resources shared by all rectangle renderers
pub struct GlobalResources {
    /// Pipeline used to render the rectangles
    pipeline: wgpu::RenderPipeline,
    /// Bind group layout for rectangle pipeline
    bind_group_layout: wgpu::BindGroupLayout,
}

impl GlobalResources {
    /// Allocate all the globally shareable render resources
    pub fn new(
        device: &Device,
        shader: &wgpu::ShaderModule,
        rs_target_format: wgpu::ColorTargetState,
    ) -> Self {
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
                    array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
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

        Self {
            pipeline,
            bind_group_layout,
        }
    }

    /// Allocate storage for an individual rectangle renderer based on this suite of global
    /// resources.
    pub fn create_local(&self, device: &Device) -> RenderResources {
        const INITIAL_COUNT: u64 = 16;

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("canvas.rects.uniforms"),
            size: std::mem::size_of::<Uniforms>() as _,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("canvas.rects.bind_group"),
            layout: &self.bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        let (vertex_buffer, index_buffer) = alloc_buffers(device, INITIAL_COUNT);

        RenderResources {
            uniform_buffer,
            bind_group,
            vertex_buffer,
            index_buffer,
            count: INITIAL_COUNT,
        }
    }
}

/// Collection of resources used to render rectangles, per instance.
pub struct RenderResources {
    /// Buffer for uniforms
    uniform_buffer: wgpu::Buffer,
    /// Bind group
    bind_group: wgpu::BindGroup,

    /// Buffer for verticies
    vertex_buffer: wgpu::Buffer,
    /// Buffer for indicies
    index_buffer: wgpu::Buffer,
    /// Number of slots we have allocated in `Self::vertex_buffer` and `Self::index_buffer`
    count: u64,
}

impl Canvas {
    /// Set the rectangles to be rendered for this canvas this frame.
    pub fn render_rectangles(
        &self,
        ui: &mut egui::Ui,
        projection_view: na::Matrix4<f32>,
        render_rect: egui::Rect,
        clip_rect: egui::Rect,
        rectangles: impl Iterator<Item = egui::Rect>,
    ) {
        let mut count: IndexType = 0;
        let mut verticies: Vec<Vertex> = Vec::new();
        let mut indicies: Vec<IndexType> = Vec::new();

        for rect in rectangles {
            if rect.intersects(clip_rect) {
                let base_idx: u16 = verticies.len().try_into().unwrap();
                verticies.push(Vertex {
                    pos: egui::Vec2::new(rect.min.x, rect.min.y),
                });
                verticies.push(Vertex {
                    pos: egui::Vec2::new(rect.min.x, rect.max.y),
                });
                verticies.push(Vertex {
                    pos: egui::Vec2::new(rect.max.x, rect.max.y),
                });
                verticies.push(Vertex {
                    pos: egui::Vec2::new(rect.max.x, rect.min.y),
                });

                indicies.push(base_idx + 0);
                indicies.push(base_idx + 1);
                indicies.push(base_idx + 2);
                indicies.push(base_idx + 3);
                indicies.push(base_idx + 0);
                indicies.push(IndexType::MAX);

                count += 1;
            }
        }

        if count == 0 {
            return;
        }

        let mut uniforms = Uniforms {
            projection_view: [0.0; 16],
            color: [1.0, 0.0, 1.0, 1.0],
        };

        assert_eq!(projection_view.as_slice().len(), 16);
        for (i, f) in projection_view.as_slice().iter().enumerate() {
            uniforms.projection_view[i] = *f;
        }

        let id = self.id;

        let cb = egui_wgpu::CallbackFn::new()
            .prepare(move |device, queue, paint_callback_resources| {
                let global_resources = &mut paint_callback_resources
                    .get_mut::<CanvasGlobalResources>()
                    .unwrap();

                let mut local_resources =
                    &mut global_resources.canvases.get_mut(&id).unwrap().rectangle;

                let count: u64 = count.into();
                if count > local_resources.count {
                    let new_count = count + 16;

                    let (vtx, idx) = alloc_buffers(device, new_count);

                    local_resources.index_buffer = idx;
                    local_resources.vertex_buffer = vtx;
                    local_resources.count = count;
                }

                queue.write_buffer(
                    &local_resources.vertex_buffer,
                    0,
                    bytemuck::cast_slice(&verticies),
                );

                queue.write_buffer(
                    &local_resources.index_buffer,
                    0,
                    bytemuck::cast_slice(&indicies),
                );

                queue.write_buffer(
                    &local_resources.uniform_buffer,
                    0,
                    bytemuck::cast_slice(&[uniforms]),
                );
            })
            .paint(move |_info, rpass, paint_callback_resources| {
                let global_resources = paint_callback_resources
                    .get::<CanvasGlobalResources>()
                    .unwrap();
                let local_resources = global_resources.canvases.get(&id).unwrap();

                let global_resources = &global_resources.rectangle;
                let local_resources = &local_resources.rectangle;

                rpass.set_pipeline(&global_resources.pipeline);
                rpass.set_bind_group(0, &local_resources.bind_group, &[]);
                rpass.set_vertex_buffer(
                    0,
                    local_resources.vertex_buffer.slice(
                        ..(count as u64 * VERTEX_PER_RECT * std::mem::size_of::<Vertex>() as u64),
                    ),
                );
                rpass.set_index_buffer(
                    local_resources.index_buffer.slice(
                        ..(count as u64 * INDEX_PER_RECT * std::mem::size_of::<IndexType>() as u64),
                    ),
                    RECT_INDEX_FORMAT,
                );
                rpass.draw_indexed(0..((count as u32) * INDEX_PER_RECT as u32), 0, 0..1);
            });

        ui.painter().add(egui::PaintCallback {
            rect: render_rect,
            callback: Arc::new(cb),
        });
    }
}

fn alloc_buffers(device: &wgpu::Device, count: u64) -> (wgpu::Buffer, wgpu::Buffer) {
    let vertex = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("canvas.rects.vertex_buffer"),
        size: count * VERTEX_PER_RECT * std::mem::size_of::<Vertex>() as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let index = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("canvas.rects.vertex_buffer"),
        size: count * INDEX_PER_RECT * std::mem::size_of::<IndexType>() as u64,
        usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    (vertex, index)
}
