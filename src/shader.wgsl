@group(0) @binding(0)
var t: texture_2d<f32>;
@group(0) @binding(1)
var s: sampler;

struct TextureUniforms {
    size: vec2<f32>,
    is_mask: u32,
}

@group(0) @binding(2)
var<uniform> texture_uniforms: TextureUniforms;

struct TargetUniforms {
    size: vec2<f32>,
}

@group(1) @binding(0)
var<uniform> target_uniforms: TargetUniforms;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) tex_coords: vec2<f32>,
    @location(2) tint: vec4<f32>,
}

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
    @location(1) tint: vec4<f32>,
};

@vertex
fn vs_main(model: VertexInput) -> VertexOutput {
    var out: VertexOutput;

    out.tint = model.tint;

    // Normalize screen position to NDC position.
    var pos = (model.position.xy / target_uniforms.size - 0.5) * 2.0;
    pos.y = -pos.y;

    out.tex_coords = model.tex_coords;
    out.position = vec4<f32>(pos, model.position.z, 1.0);
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    var sample = textureSample(t, s, in.tex_coords / texture_uniforms.size);
    if texture_uniforms.is_mask == 1 {
        sample = vec4(1.0, 1.0, 1.0, sample.r);
    }
    return sample * in.tint;
}
