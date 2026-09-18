//! Rendering a frame to a file.
//!
//! This is the milestone's proof, and it is also the CI artifact the exit criterion asks
//! for — *"text layout golden: a fixed string lays out byte-identically across the CI
//! matrix"*. Reading the framebuffer back needs no display, no window, and no compositor, so
//! the same code path runs on a laptop and in CI, which is the only way the two agree.
//!
//! See `tools/capture.sh` for the wrapper future milestones use.

use std::path::Path;

use crate::draw::DrawList;
use crate::graph::{Frame, RenderGraph};
use crate::renderer::Renderer;

/// Renders a frame offscreen and reads the pixels back.
pub struct Capture {
    renderer: Renderer,
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    width: u32,
    height: u32,
}

/// The capture format.
///
/// sRGB, because the numbers a PNG holds are sRGB and the renderer's blend is designed for a
/// display — capturing in linear and saving it raw would produce a file that is far too dark
/// and a golden that pins the wrong values.
pub const CAPTURE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

impl Capture {
    /// A capture target of `width` × `height`.
    ///
    /// `None` when no adapter is available. Everything that can be verified without a GPU —
    /// layout, draw lists, the atlas — is tested without one; this is the one thing that
    /// cannot be.
    #[must_use]
    pub fn new(width: u32, height: u32) -> Option<Self> {
        let renderer = Renderer::headless(CAPTURE_FORMAT, (width, height))?;
        let texture = renderer.device().create_texture(&wgpu::TextureDescriptor {
            label: Some("vela.capture"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: CAPTURE_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        Some(Self {
            renderer,
            texture,
            view,
            width,
            height,
        })
    }

    /// The renderer, for uploading an atlas before rendering.
    pub fn renderer_mut(&mut self) -> &mut Renderer {
        &mut self.renderer
    }

    /// The frame size.
    #[must_use]
    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Runs a graph and returns the frame's pixels, top-left first, as RGBA8.
    #[must_use]
    pub fn render(&mut self, graph: &RenderGraph, draw: &DrawList) -> Vec<u8> {
        let mut encoder =
            self.renderer
                .device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("vela.frame"),
                });

        {
            let mut frame = Frame {
                encoder: &mut encoder,
                target: &self.view,
                size: (self.width, self.height),
                draw,
                renderer: &self.renderer,
            };
            graph.run(&mut frame);
        }

        // Rows in a copy buffer must be a multiple of 256 bytes, so a frame whose width is
        // not a multiple of 64 has to be padded and then stripped.
        let padded = (self.width * 4).div_ceil(256) * 256;
        let buffer = self
            .renderer
            .device()
            .create_buffer(&wgpu::BufferDescriptor {
                label: Some("vela.readback"),
                size: u64::from(padded) * u64::from(self.height),
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded),
                    rows_per_image: Some(self.height),
                },
            },
            wgpu::Extent3d {
                width: self.width,
                height: self.height,
                depth_or_array_layers: 1,
            },
        );
        self.renderer.queue().submit(Some(encoder.finish()));

        read_back(&self.renderer, &buffer, padded, self.width, self.height)
    }

    /// Renders a frame, writes it to `path` as a PNG, and hands the pixels back.
    ///
    /// The pixels come back because a *check* needs them and a picture does not: a frame that is one
    /// flat colour, or one identical to the frame before it, is a bug a person sees immediately and
    /// a written PNG reports to nobody. Returning them also keeps the frame from being rendered
    /// twice, which is what a separate `render` call would do.
    ///
    /// # Errors
    ///
    /// Returns the I/O or encoding failure, because a capture that silently produced no file
    /// is worse than one that failed: the caller is usually a comparison.
    pub fn save(
        &mut self,
        graph: &RenderGraph,
        draw: &DrawList,
        path: &Path,
    ) -> std::io::Result<Vec<u8>> {
        let pixels = self.render(graph, draw);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let file = std::fs::File::create(path)?;
        let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), self.width, self.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .and_then(|mut writer| writer.write_image_data(&pixels))
            .map_err(|error| std::io::Error::other(error.to_string()))?;
        Ok(pixels)
    }
}

/// Blocks until the copy has landed, then strips the row padding.
fn read_back(
    renderer: &Renderer,
    buffer: &wgpu::Buffer,
    padded: u32,
    width: u32,
    height: u32,
) -> Vec<u8> {
    let slice = buffer.slice(..);
    let (sender, receiver) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = sender.send(result);
    });
    // The device must be polled for the mapping to complete. `wait_indefinitely` because a
    // copy this small always finishes; a timeout would only add a way to fail spuriously.
    let _ = renderer.device().poll(wgpu::PollType::wait_indefinitely());
    let _ = receiver.recv();

    let mapped = slice.get_mapped_range();
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    for row in 0..height {
        let start = (row * padded) as usize;
        pixels.extend_from_slice(&mapped[start..start + (width * 4) as usize]);
    }
    drop(mapped);
    buffer.unmap();
    pixels
}
