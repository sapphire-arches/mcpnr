use eframe::wgpu::{self, Device};
use nalgebra as na;
use std::sync::Arc;

use crate::gui::canvas::CanvasGlobalResources;

use super::{shader, Canvas};

/********************************************************************************
 * Rendering types and constants
********************************************************************************/
type Uniforms = shader::Uniforms;
type Vertex = shader::Vertex;

/********************************************************************************
 * Implementation
********************************************************************************/

/// wgpu resources shared by all line renderers
pub struct GlobalResources {
    /// Pipeline used to render the lines
    pipeline: wgpu::RenderPipeline,
    /// Bind group layout for line pipeline
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
            label: Some("canvas.lines.bind_group_layout"),
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
            label: Some("canvas.lines.pipeline_layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("canvas.lines.pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: shader,
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
                module: shader,
                entry_point: "fs_main",
                targets: &[Some(rs_target_format)],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::LineList,
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

    /// Allocate storage for an individual line renderer based on this suite of global resources.
    pub fn create_local(&self, device: &Device) -> RenderResources {
        const INITIAL_COUNT: u64 = 16;

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("canvas.lines.uniforms"),
            size: std::mem::size_of::<Uniforms>() as _,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("canvas.lines.bind_group"),
            layout: &self.bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("canvas.lines.vertex_buffer"),
            size: INITIAL_COUNT * 2 * std::mem::size_of::<Vertex>() as u64,
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        RenderResources {
            uniform_buffer,
            bind_group,
            vertex_buffer,
            count: INITIAL_COUNT,
        }
    }
}

/// Collection of resources used to render lines, per instance.
pub struct RenderResources {
    /// Buffer for uniforms
    uniform_buffer: wgpu::Buffer,
    /// Bind group
    bind_group: wgpu::BindGroup,

    /// Buffer for verticies
    vertex_buffer: wgpu::Buffer,
    /// Number of slots we have allocated in `Self::vertex_buffer`
    count: u64,
}

impl Canvas {
    pub(super) fn render_lines(
        &self,
        ui: &mut egui::Ui,
        projection_view: na::Matrix4<f32>,
        render_rect: egui::Rect,
        clip_rect: egui::Rect,
        lines: impl Iterator<Item = ((f32, f32), (f32, f32))>,
    ) {
        let mut count: u64 = 0;
        let mut verticies: Vec<Vertex> = Vec::new();

        for (s, e) in lines {
            if !clip_rect.contains(s.into()) && !clip_rect.contains(e.into()) {
                continue;
            }

            verticies.push(Vertex { pos: s.into() });
            verticies.push(Vertex { pos: e.into() });

            count += 1;
        }

        if count == 0 {
            return;
        }

        let mut uniforms = Uniforms {
            projection_view: [0.0; 16],
            color: [1.0, 0.0, 0.0, 1.0],
        };

        assert_eq!(projection_view.as_slice().len(), 16);
        for (i, f) in projection_view.as_slice().iter().enumerate() {
            uniforms.projection_view[i] = *f;
        }

        let id = self.id;

        let cb = egui_wgpu::CallbackFn::new()
            .prepare(move |device, queue, paint_callback_resources| {
                let global_resources: &mut CanvasGlobalResources =
                    paint_callback_resources.get_mut().unwrap();

                let local_resources = global_resources.canvases.get_mut(&id).unwrap();

                let local_resources = &mut local_resources.line;

                if count > local_resources.count {
                    let new_line_count = count + 16;

                    let vtx = device.create_buffer(&wgpu::BufferDescriptor {
                        label: Some("canvas.line.vertex"),
                        size: std::mem::size_of::<Vertex>() as u64 * 2 * new_line_count,
                        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                        mapped_at_creation: false,
                    });

                    local_resources.vertex_buffer = vtx;
                    local_resources.count = new_line_count;
                }

                queue.write_buffer(
                    &local_resources.vertex_buffer,
                    0,
                    bytemuck::cast_slice(&verticies),
                );

                queue.write_buffer(
                    &local_resources.uniform_buffer,
                    0,
                    bytemuck::cast_slice(&[uniforms]),
                );
            })
            .paint(move |_info, rpass, paint_callback_resources| {
                let global_resources: &CanvasGlobalResources =
                    paint_callback_resources.get().unwrap();

                let local_resources = global_resources.canvases.get(&id).unwrap();

                let global_resources = &global_resources.line;
                let local_resources = &local_resources.line;

                rpass.set_pipeline(&global_resources.pipeline);
                rpass.set_bind_group(0, &local_resources.bind_group, &[]);
                rpass.set_vertex_buffer(
                    0,
                    local_resources
                        .vertex_buffer
                        .slice(..(count * 2 * std::mem::size_of::<Vertex>() as u64)),
                );
                rpass.draw(0..((count as u32) * 2), 0..1);
            });

        ui.painter().add(egui::PaintCallback {
            rect: render_rect,
            callback: Arc::new(cb),
        });
    }
}
