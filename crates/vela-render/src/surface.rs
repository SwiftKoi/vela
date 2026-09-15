//! Rendering into a window.
//!
//! The other half of the target story: `Capture` renders to a texture and reads it back,
//! this renders to a swapchain and presents it. Both go through the same `RenderGraph` and
//! the same `DrawList`, so what a capture shows and what a window shows are produced by the
//! same code — which is what makes the capture evidence rather than a re-implementation.
//!
//! A surface has three states that are not errors and must not be treated as such: *lost*
//! (recreate the surface), *outdated* (reconfigure), and *timeout* (try again). Collapsing
//! them into a failure is how a renderer that works on one machine crashes on another.

use std::sync::Arc;

use winit::window::Window as WinitWindow;

use crate::draw::DrawList;
use crate::graph::{Frame, RenderGraph};
use crate::renderer::Renderer;

/// What happened to a presented frame.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Presented {
    /// It was drawn.
    Drawn,
    /// The surface is gone and must be rebuilt.
    Lost,
    /// The swapchain no longer matches the window.
    Outdated,
}

/// A window to draw into.
pub struct Surface {
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    renderer: Renderer,
    window: Arc<WinitWindow>,
}

impl Surface {
    /// Attaches a renderer to `window`.
    ///
    /// `None` when no adapter can draw to this window — a machine with no GPU, or a headless
    /// environment. The caller decides what that means; here it is not a panic, because a
    /// story that cannot open a window should still be able to say so.
    #[must_use]
    pub fn new(
        window: Arc<WinitWindow>,
        size: (u32, u32),
        format_hint: Option<wgpu::TextureFormat>,
    ) -> Option<Self> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let surface = instance.create_surface(window.clone()).ok()?;
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::default(),
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        }))
        .ok()?;

        let capabilities = surface.get_capabilities(&adapter);
        // Prefer an sRGB format: the shader works in linear space and the swapchain has to
        // convert, or everything renders too dark.
        let format = format_hint
            .filter(|format| capabilities.formats.contains(format))
            .or_else(|| {
                capabilities
                    .formats
                    .iter()
                    .copied()
                    .find(wgpu::TextureFormat::is_srgb)
            })
            .or_else(|| capabilities.formats.first().copied())?;

        let (device, queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).ok()?;

        let mut config = surface
            .get_default_config(&adapter, size.0.max(1), size.1.max(1))
            .or_else(|| {
                Some(wgpu::SurfaceConfiguration {
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                    format,
                    width: size.0.max(1),
                    height: size.1.max(1),
                    present_mode: wgpu::PresentMode::Fifo,
                    desired_maximum_frame_latency: 2,
                    alpha_mode: capabilities.alpha_modes.first().copied()?,
                    view_formats: vec![],
                })
            })?;
        config.format = format;
        // Fifo rather than Mailbox: a visual novel has nothing to gain from an uncapped frame
        // rate and everything to lose on a laptop's battery.
        config.present_mode = wgpu::PresentMode::Fifo;
        surface.configure(&device, &config);

        let renderer = Renderer::from_device(device, queue, format, size);
        Some(Self {
            surface,
            config,
            renderer,
            window,
        })
    }

    /// The size the swapchain is configured for.
    #[must_use]
    pub fn size(&self) -> (u32, u32) {
        (self.config.width, self.config.height)
    }

    /// The renderer, for uploading an atlas.
    pub fn renderer_mut(&mut self) -> &mut Renderer {
        &mut self.renderer
    }

    /// Reconfigures for a new window size.
    pub fn resize(&mut self, size: (u32, u32)) {
        if size.0 == 0 || size.1 == 0 || size == self.size() {
            return;
        }
        self.config.width = size.0;
        self.config.height = size.1;
        self.surface.configure(self.renderer.device(), &self.config);
        self.renderer.set_viewport(size);
    }

    /// Rebuilds the surface after the platform invalidated it.
    pub fn rebuild(&mut self) {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        if let Ok(surface) = instance.create_surface(self.window.clone()) {
            self.surface = surface;
            self.surface.configure(self.renderer.device(), &self.config);
        }
    }

    /// Draws a frame and presents it.
    #[must_use]
    pub fn render(&mut self, graph: &RenderGraph, draw: &DrawList) -> Presented {
        let frame = match self.surface.get_current_texture() {
            Ok(frame) => frame,
            Err(wgpu::SurfaceError::Lost) => return Presented::Lost,
            Err(wgpu::SurfaceError::Outdated) => return Presented::Outdated,
            // A timeout, or a compositor that was busy, is not a reason to end a story: the
            // next frame will very likely get one. Out of memory is reported as `Lost` so the
            // caller rebuilds, which is the closest thing to a recovery this layer has.
            Err(wgpu::SurfaceError::Timeout | wgpu::SurfaceError::Other) => {
                return Presented::Drawn;
            }
            Err(wgpu::SurfaceError::OutOfMemory) => return Presented::Lost,
        };

        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder =
            self.renderer
                .device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("vela.window_frame"),
                });
        {
            let mut target = Frame {
                encoder: &mut encoder,
                target: &view,
                size: self.size(),
                draw,
                renderer: &self.renderer,
            };
            graph.run(&mut target);
        }
        self.renderer.queue().submit(Some(encoder.finish()));
        frame.present();
        Presented::Drawn
    }
}
