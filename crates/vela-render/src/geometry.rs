//! The geometry batch: a draw list in, vertices and draws out.
//!
//! The vertex building is a free function so it can be tested without a device — which is
//! how the bowtie in the first triangulation was found exactly, rather than guessed at from
//! a screenshot.

use crate::draw::{DrawList, GlyphQuad, Quad, RectQuad};
use crate::renderer::{Renderer, Vertex};

impl Renderer {
    /// Draws a draw list into `frame`, in one pass.
    pub fn draw(&self, frame: &mut crate::graph::Frame<'_>, draw: &DrawList) {
        let vertices = build_vertices(draw);
        if vertices.is_empty() {
            return;
        }
        let indices: Vec<u32> = (0..(vertices.len() / 4) as u32)
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
            draw.quads(),
            self.atlas.as_ref().map(|(_, atlas)| atlas),
            &self.white,
            &self.uniforms_bind,
        );
    }
}

/// Emits one draw call per run of consecutive quads of the same kind.
///
/// Runs rather than two fixed batches, because submission order is the picture and a frame may
/// interleave rectangles and glyphs — a menu drawn over a dialogue is text, then a panel, then
/// more text. Rectangles sample the white texel, glyphs the atlas; a glyph run with no atlas
/// has nothing to sample and is skipped, since drawing it against the white texel would paint
/// opaque boxes where the letters should be.
fn emit_runs(
    pass: &mut wgpu::RenderPass<'_>,
    quads: &[Quad],
    atlas: Option<&wgpu::BindGroup>,
    white: &wgpu::BindGroup,
    uniforms: &wgpu::BindGroup,
) {
    let mut first = 0usize;
    while first < quads.len() {
        let glyphs = quads[first].is_glyph();
        let mut last = first;
        while last < quads.len() && quads[last].is_glyph() == glyphs {
            last += 1;
        }
        let range = (first as u32 * 6)..(last as u32 * 6);
        match (atlas, glyphs) {
            (Some(atlas), true) => {
                pass.set_bind_group(0, uniforms, &[]);
                pass.set_bind_group(1, atlas, &[]);
                pass.draw_indexed(range, 0, 0..1);
            }
            (None, true) => {}
            (_, false) => {
                pass.set_bind_group(0, uniforms, &[]);
                pass.set_bind_group(1, white, &[]);
                pass.draw_indexed(range, 0, 0..1);
            }
        }
        first = last;
    }
}

/// Turns a draw list into a vertex array: four vertices per quad, in submission order.
///
/// The public form of this exists for tests, which can then assert on geometry without a GPU.
#[must_use]
pub fn build_vertices(draw: &DrawList) -> Vec<Vertex> {
    let mut vertices = Vec::with_capacity(draw.len() * 4);
    for quad in draw.quads() {
        match quad {
            Quad::Rect(rect) => vertices.extend(rect_vertices(rect)),
            Quad::Glyph(glyph) => vertices.extend(glyph_vertices(glyph)),
        }
    }
    vertices
}

/// The four corners of a filled rectangle, in triangle-strip-free order (TL, TR, BL, BR as
/// the index buffer expects: 0,1,2, 0,2,3).
fn rect_vertices(rect: &RectQuad) -> [Vertex; 4] {
    let (l, t) = (rect.x, rect.y);
    let (r, b) = (rect.x + rect.width, rect.y + rect.height);
    let color = [rect.color.r, rect.color.g, rect.color.b, rect.color.a];
    [
        Vertex {
            position: [l, t],
            uv: [0.5, 0.5],
            color,
        },
        Vertex {
            position: [r, t],
            uv: [0.5, 0.5],
            color,
        },
        Vertex {
            position: [l, b],
            uv: [0.5, 0.5],
            color,
        },
        Vertex {
            position: [r, b],
            uv: [0.5, 0.5],
            color,
        },
    ]
}

/// The four corners of a glyph quad, carrying the atlas rectangle.
fn glyph_vertices(glyph: &GlyphQuad) -> [Vertex; 4] {
    let (l, t) = (glyph.x, glyph.y);
    let (r, b) = (glyph.x + glyph.width, glyph.y + glyph.height);
    let [u0, v0, u1, v1] = glyph.uv;
    let color = [glyph.color.r, glyph.color.g, glyph.color.b, glyph.color.a];
    [
        Vertex {
            position: [l, t],
            uv: [u0, v0],
            color,
        },
        Vertex {
            position: [r, t],
            uv: [u1, v0],
            color,
        },
        Vertex {
            position: [l, b],
            uv: [u0, v1],
            color,
        },
        Vertex {
            position: [r, b],
            uv: [u1, v1],
            color,
        },
    ]
}
