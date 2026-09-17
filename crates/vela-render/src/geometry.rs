//! The geometry batch: a draw list in, vertices and draws out.
//!
//! The vertex building is a free function so it can be tested without a device — which is
//! how the bowtie in the first triangulation was found exactly, rather than guessed at from
//! a screenshot.

use crate::draw::{Clip, Color, DrawList, GlyphQuad, ImageQuad, Op, Quad, RectQuad, Source};
use crate::renderer::{Renderer, Vertex};

impl Renderer {
    /// Draws a draw list into `frame`, in one pass.
    pub fn draw(&self, frame: &mut crate::graph::Frame<'_>, draw: &DrawList) {
        let vertices = build_vertices(draw);
        if vertices.is_empty() {
            return;
        }
        let quad_count = draw.len();
        let quads: Vec<&Quad> = draw.quads().collect();
        let indices: Vec<u32> = (0..quad_count as u32)
            .flat_map(|quad| {
                let base = quad * 4;
                // TL,TR,BL and TR,BR,BL — two triangles sharing the anti-diagonal.
                //
                // The obvious-looking TL,BL,BR for the second triangle is *wrong*, and
                // wrong in a way that renders: it shares the left edge instead, leaving the
                // right-hand wedge uncovered and drawing a bowtie. Both orders triangulate
                // the same four corners; only one tiles them.
                [base, base + 1, base + 2, base + 1, base + 3, base + 2]
            })
            .collect();

        let vertex_buffer = self.create_buffer(
            "vela.vertices",
            bytemuck::cast_slice(&vertices),
            wgpu::BufferUsages::VERTEX,
        );
        let index_buffer = self.create_buffer(
            "vela.indices",
            bytemuck::cast_slice(&indices),
            wgpu::BufferUsages::INDEX,
        );

        let mut pass = frame
            .encoder
            .begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("vela.geometry"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: frame.target,
                    resolve_target: None,
                    // Load rather than clear: the clear is a stage of its own, and a
                    // geometry stage that also cleared would make the two inseparable.
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                ..Default::default()
            });

        pass.set_pipeline(&self.pipeline);
        pass.set_vertex_buffer(0, vertex_buffer.slice(..));
        pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint32);

        emit_runs(
            &mut pass,
            &quads,
            self.atlas.as_ref().map(|(_, atlas)| atlas),
            &self.white,
            &self.images,
            &self.uniforms_bind,
        );
    }
}

/// Emits one draw call per run of consecutive quads from the same texture.
///
/// Runs rather than fixed batches, because submission order is the picture and a frame may
/// interleave all three kinds — a background, a panel, a portrait, the dialogue over them.
/// Quads of one source share a bind group; a change of source is a new draw call.
///
/// A clip between two quads does not end a run: the clip is a vertex attribute, so the quads either
/// side of it still share a bind group and a draw call covers both.
fn emit_runs(
    pass: &mut wgpu::RenderPass<'_>,
    quads: &[&Quad],
    atlas: Option<&wgpu::BindGroup>,
    white: &wgpu::BindGroup,
    images: &[(wgpu::Texture, wgpu::BindGroup)],
    uniforms: &wgpu::BindGroup,
) {
    let mut first = 0usize;
    while first < quads.len() {
        let source = quads[first].source();
        let mut last = first;
        while last < quads.len() && quads[last].source() == source {
            last += 1;
        }

        // A run whose texture is not there has nothing to sample: a glyph with no atlas built,
        // or an image that was never uploaded. Skipping it is the honest failure — drawing it
        // against the white texel would paint opaque boxes where the letters or the picture
        // should be, which reads as a rendering bug rather than as a missing asset.
        let bound = match source {
            Source::White => Some(white),
            Source::Atlas => atlas,
            Source::Image(index) => images.get(index as usize).map(|(_, bind)| bind),
        };
        if let Some(bound) = bound {
            pass.set_bind_group(0, uniforms, &[]);
            pass.set_bind_group(1, bound, &[]);
            pass.draw_indexed((first as u32 * 6)..(last as u32 * 6), 0, 0..1);
        }
        first = last;
    }
}

/// Turns a draw list into a vertex array: four vertices per quad, in submission order.
///
/// The public form of this exists for tests, which can then assert on geometry without a GPU.
///
/// Clips are replayed rather than stored per quad: a clip is a *region* of the submission order, so the
/// walk keeps a stack and stamps each quad with the clip in force when it is reached. A pop with nothing
/// pushed leaves the clip alone, matching the draw list's own tolerance.
#[must_use]
pub fn build_vertices(draw: &DrawList) -> Vec<Vertex> {
    let mut vertices = Vec::with_capacity(draw.len() * 4);
    let mut clip = Clip::UNBOUNDED;
    let mut enclosing: Vec<Clip> = Vec::new();

    for op in draw.ops() {
        match op {
            Op::Clip(narrowed) => {
                enclosing.push(clip);
                clip = *narrowed;
            }
            Op::Unclip => {
                if let Some(outer) = enclosing.pop() {
                    clip = outer;
                }
            }
            Op::Quad(quad) => {
                let bounds = clip.bounds();
                vertices.extend(match quad {
                    Quad::Rect(rect) => rect_vertices(rect, bounds),
                    Quad::Glyph(glyph) => glyph_vertices(glyph, bounds),
                    Quad::Image(image) => image_vertices(image, bounds),
                });
            }
        }
    }
    vertices
}

/// The four corners of a filled rectangle, in triangle-strip-free order (TL, TR, BL, BR as
/// the index buffer expects: 0,1,2, 0,2,3).
fn rect_vertices(rect: &RectQuad, clip: [f32; 4]) -> [Vertex; 4] {
    let (l, t) = (rect.x, rect.y);
    let (r, b) = (rect.x + rect.width, rect.y + rect.height);
    let color = [rect.color.r, rect.color.g, rect.color.b, rect.color.a];
    [
        Vertex {
            position: [l, t],
            uv: [0.5, 0.5],
            color,
            mode: MASK,
            clip,
        },
        Vertex {
            position: [r, t],
            uv: [0.5, 0.5],
            color,
            mode: MASK,
            clip,
        },
        Vertex {
            position: [l, b],
            uv: [0.5, 0.5],
            color,
            mode: MASK,
            clip,
        },
        Vertex {
            position: [r, b],
            uv: [0.5, 0.5],
            color,
            mode: MASK,
            clip,
        },
    ]
}

/// The four corners of a glyph quad, carrying the atlas rectangle.
fn glyph_vertices(glyph: &GlyphQuad, clip: [f32; 4]) -> [Vertex; 4] {
    textured_vertices(
        [glyph.x, glyph.y, glyph.width, glyph.height],
        glyph.uv,
        glyph.color,
        MASK,
        clip,
    )
}

/// The four corners of an image quad, carrying the part of the image it shows.
fn image_vertices(image: &ImageQuad, clip: [f32; 4]) -> [Vertex; 4] {
    textured_vertices(
        [image.x, image.y, image.width, image.height],
        image.uv,
        image.color,
        COLOUR,
        clip,
    )
}

/// A coverage mask: the texture's red channel is alpha and the vertex carries the colour.
const MASK: f32 = 0.0;

/// Colour: the texture is the picture and the vertex tints it.
const COLOUR: f32 = 1.0;

/// The four corners of a textured quad.
///
/// One function for both texturers: a glyph and a picture differ in which texture they sample
/// and in nothing else, and two copies of this would be two places for a corner order to drift.
///
/// `rect` is `[x, y, width, height]`, bundled because a corner list is five things and a signature that
/// spelled them all out would be a signature nobody reads.
fn textured_vertices(
    rect: [f32; 4],
    uv: [f32; 4],
    color: Color,
    mode: f32,
    clip: [f32; 4],
) -> [Vertex; 4] {
    let [x, y, width, height] = rect;
    let (l, t) = (x, y);
    let (r, b) = (x + width, y + height);
    let [u0, v0, u1, v1] = uv;
    let color = [color.r, color.g, color.b, color.a];
    [
        Vertex {
            position: [l, t],
            uv: [u0, v0],
            color,
            mode,
            clip,
        },
        Vertex {
            position: [r, t],
            uv: [u1, v0],
            color,
            mode,
            clip,
        },
        Vertex {
            position: [l, b],
            uv: [u0, v1],
            color,
            mode,
            clip,
        },
        Vertex {
            position: [r, b],
            uv: [u1, v1],
            color,
            mode,
            clip,
        },
    ]
}
