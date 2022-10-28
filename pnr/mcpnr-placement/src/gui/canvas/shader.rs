use bytemuck::{Pod, Zeroable};

/// Uniform buffer layout for the rectangle renderer
#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, Pod, Zeroable)]
pub struct Uniforms {
    pub projection_view: [f32; 16],
    pub color: [f32; 4],
}

/// Vertex type for the rectangle renderer
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct Vertex {
    pub pos: egui::Vec2,
}

/// WGSL source code of the shader
pub const SOURCE: &'static str = include_str!("./canvas_shaders.wgsl");

