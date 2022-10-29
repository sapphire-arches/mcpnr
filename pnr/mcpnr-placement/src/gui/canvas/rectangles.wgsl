struct VertexOut {
  @builtin(position) position: vec4<f32>,
};

struct Uniforms {
  // transform matrix
  projection_view: mat4x4<f32>,
  // Color for all lines
  color: vec4<f32>,
};

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

@vertex
fn vs_main(
  @location(0) position: vec2<f32>,
) -> VertexOut {
  var out: VertexOut;

  out.position =  uniforms.projection_view * vec4(position, 0.0, 1.0);

  return out;
}

@fragment
fn fs_main(
) -> @location(0) vec4<f32> {
  return uniforms.color;
}
