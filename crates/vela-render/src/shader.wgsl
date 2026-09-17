// Pixel position to clip space, and a texture onto a colour.
//
// The same shader draws all three of a frame's quads, and `mode` is what tells it which it is
// looking at. A filled rectangle samples the one-texel white texture, so its mask is 1.0 and
// the fragment is the vertex colour. A glyph samples the atlas, so its mask is coverage and the
// fragment is the colour tinted by it. An *image* is the third case and the opposite one: its
// texture is colour, and the vertex colour tints *it* — which is what a background needs and
// what the coverage path cannot express, because that path throws the texture's colour away.

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
    // 0 = the texture is a coverage mask; 1 = the texture is colour.
    @location(3) mode: f32,
    // The rectangle this vertex may be drawn inside, as [left, top, right, bottom] in pixels.
    @location(4) clip: vec4<f32>,
};

struct VertexOutput {
    // The vertex writes the clip-space position here, and the fragment reads the *pixel* position out
    // of the same builtin — which is exactly what a clip test needs. Passing `@builtin(position)` as a
    // fragment parameter as well would declare it twice and fail validation.
    @builtin(position) at: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) mode: f32,
    @location(3) bounds: vec4<f32>,
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
    output.at = vec4<f32>(ndc, 0.0, 1.0);
    output.uv = input.uv;
    output.color = input.color;
    output.mode = input.mode;
    output.bounds = input.clip;
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    // What falls outside the clip is not drawn. A viewport is the reason: its content is laid out
    // whole and only a window of it is shown, so the part outside has to go somewhere — and the
    // honest place is nowhere, rather than under whatever is drawn next.
    if (input.at.x < input.bounds.x || input.at.y < input.bounds.y ||
        input.at.x >= input.bounds.z || input.at.y >= input.bounds.w) {
        discard;
    }
    let texel = textureSample(mask_texture, mask_sampler, input.uv);
    // A mask contributes coverage and takes its colour from the vertex. A picture contributes
    // colour and is tinted by the vertex — white meaning "draw it as it is".
    let masked = vec4<f32>(input.color.rgb, input.color.a * texel.r);
    let tinted = input.color * texel;
    // Straight alpha either way, which is what `ALPHA_BLENDING` expects.
    return select(masked, tinted, input.mode > 0.5);
}
