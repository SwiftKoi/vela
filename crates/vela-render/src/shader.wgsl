// Pixel position to clip space, and a coverage mask onto a colour.
//
// The same shader draws a filled rectangle and a glyph: a rectangle samples the one-texel
// white texture, so its mask is 1.0 and the fragment is the vertex colour; a glyph samples
// the atlas, so its mask is coverage and the fragment is the colour tinted by it. One
// pipeline, two bindings.

struct Uniforms {
    viewport: vec2<f32>,
    _padding: vec2<f32>,
};

@group(0) @binding(0) var<uniform> uniforms: Uniforms;

@group(1) @binding(0) var mask_texture: texture_2d<f32>;
@group(1) @binding(1) var mask_sampler: sampler;

struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
};

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    // Pixels to normalised device coordinates: (0,0) at the top-left of the frame, y
    // increasing downwards, which is why the y term is inverted.
    let ndc = vec2<f32>(
        input.position.x / uniforms.viewport.x * 2.0 - 1.0,
        1.0 - input.position.y / uniforms.viewport.y * 2.0,
    );
    output.clip = vec4<f32>(ndc, 0.0, 1.0);
    output.uv = input.uv;
    output.color = input.color;
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let coverage = textureSample(mask_texture, mask_sampler, input.uv).r;
    // The colour is straight (not premultiplied), which is what `ALPHA_BLENDING` expects.
    return vec4<f32>(input.color.rgb, input.color.a * coverage);
}
