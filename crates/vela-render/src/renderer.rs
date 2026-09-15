//! The wgpu device, the pipelines, and the atlas texture.

use bytemuck::{Pod, Zeroable};

/// One vertex: a pixel position, an atlas coordinate, and a colour.
///
/// Solid rectangles and glyphs share this layout so they share a pipeline. A solid rectangle
/// samples a one-texel white texture, which makes "filled" and "textured" the same draw with
/// a different binding rather than two pipelines and two shader pairs.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Debug)]
pub struct Vertex {
    /// Position in pixels, from the frame's top-left.
    pub position: [f32; 2],
    /// Atlas coordinates, normalised; `(0, 0)` for a solid rectangle.
    pub uv: [f32; 2],
    /// Linear colour, premultiplied by nothing — the shader multiplies by coverage.
    pub color: [f32; 4],
    /// What this quad's texture is: `0.0` a coverage mask (the atlas, the white texel), `1.0`
    /// colour to be tinted.
    ///
    /// A vertex attribute rather than a second pipeline, because it is a property of the quad
    /// and not of the draw: the frame interleaves backgrounds, panels, and text, and a pipeline
    /// switch per run would be state to keep in step for no gain.
    pub mode: f32,
}

/// The per-frame uniform block.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Uniforms {
    viewport: [f32; 2],
    _padding: [f32; 2],
}

/// The device-side renderer.
pub struct Renderer {
    pub(crate) device: wgpu::Device,
    pub(crate) queue: wgpu::Queue,
    pub(crate) pipeline: wgpu::RenderPipeline,
    pub(crate) uniforms: wgpu::Buffer,
    pub(crate) uniforms_bind: wgpu::BindGroup,
    pub(crate) bind_group_layout: wgpu::BindGroupLayout,
    /// The atlas, bound for glyphs.
    pub(crate) atlas: Option<(wgpu::Texture, wgpu::BindGroup)>,
    /// A single white texel, bound for solid rectangles.
    pub(crate) white: wgpu::BindGroup,
    /// Uploaded images, by the index a quad names.
    ///
    /// A `Vec` keyed by position, not a map: an index is what a quad carries, and an image's
    /// *name* is the caller's business. The renderer is told pixels.
    pub(crate) images: Vec<(wgpu::Texture, wgpu::BindGroup)>,
    pub(crate) format: wgpu::TextureFormat,
}

/// The atlas is one channel of coverage, not colour.
///
/// R8 because a glyph mask *is* coverage: an RGBA atlas would store the same byte four times
/// and cost four times the memory and bandwidth. The colour comes from the vertex, which is
/// what lets one glyph be drawn in two colours without two rasterisations.
pub const ATLAS_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::R8Unorm;

/// An image is colour, and its bytes are sRGB — which is what a PNG holds and what a display
/// expects, so the sampler converts to linear and the shader can stay in one space.
pub const IMAGE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

impl Renderer {
    /// A renderer with no surface — for offscreen frames and for tests.
    ///
    /// No display is required, which is not a convenience: the layout golden and the capture
    /// artifact both have to work in CI, and CI has no display.
    #[must_use]
    pub fn headless(format: wgpu::TextureFormat, size: (u32, u32)) -> Option<Self> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::default(),
            compatible_surface: None,
            force_fallback_adapter: false,
        }))
        .ok()?;
        let (device, queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).ok()?;
        Some(Self::new(device, queue, format, size))
    }

    /// A renderer over an existing device, for a caller that created one for a surface.
    #[must_use]
    pub(crate) fn from_device(
        device: wgpu::Device,
        queue: wgpu::Queue,
        format: wgpu::TextureFormat,
        size: (u32, u32),
    ) -> Self {
        Self::new(device, queue, format, size)
    }

    /// Builds the pipeline and the shared resources.
    fn new(
        device: wgpu::Device,
        queue: wgpu::Queue,
        format: wgpu::TextureFormat,
        size: (u32, u32),
    ) -> Self {
        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("vela.uniforms"),
            size: std::mem::size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group_layout = crate::pipeline::texture_layout(&device);
        let uniform_layout = crate::pipeline::uniform_layout(&device);
        let pipeline =
            crate::pipeline::create(&device, format, &uniform_layout, &bind_group_layout);

        let uniforms_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("vela.uniforms_bind"),
            layout: &uniform_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniforms.as_entire_binding(),
            }],
        });
        let white = Self::create_white_texel(&device, &queue, &bind_group_layout);

        let mut renderer = Self {
            device,
            queue,
            pipeline,
            uniforms,
            uniforms_bind,
            bind_group_layout,
            atlas: None,
            white,
            images: Vec::new(),
            format,
        };
        renderer.set_viewport(size);
        renderer
    }

    /// The target format this renderer was built for.
    #[must_use]
    pub fn format(&self) -> wgpu::TextureFormat {
        self.format
    }

    /// The device, for a caller that needs to make its own resources.
    #[must_use]
    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    /// The queue.
    #[must_use]
    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }

    /// Records the frame size into the uniform that maps pixels to clip space.
    pub fn set_viewport(&mut self, size: (u32, u32)) {
        let uniforms = Uniforms {
            viewport: [size.0.max(1) as f32, size.1.max(1) as f32],
            _padding: [0.0; 2],
        };
        self.queue
            .write_buffer(&self.uniforms, 0, bytemuck::bytes_of(&uniforms));
    }

    /// Records a full-screen clear.
    pub fn clear(&self, frame: &mut crate::graph::Frame<'_>, color: crate::draw::Color) {
        frame
            .encoder
            .begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("vela.clear"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: frame.target,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: f64::from(color.r),
                            g: f64::from(color.g),
                            b: f64::from(color.b),
                            a: f64::from(color.a),
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                ..Default::default()
            });
    }

    /// Uploads bytes into a new buffer.
    pub(crate) fn create_buffer(
        &self,
        label: &str,
        contents: &[u8],
        usage: wgpu::BufferUsages,
    ) -> wgpu::Buffer {
        use wgpu::util::DeviceExt as _;
        self.device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(label),
                contents,
                usage,
            })
    }
}
