//! The window and the event loop.
//!
//! This is the only place in the engine that knows a windowing library exists. Everything
//! above it sees a [`Window`] that reports its size and a stream of [`Action`]s; everything
//! below is `winit`. A second backend — a test harness, a mobile shell — implements the same
//! [`App`] trait and nothing else changes.
//!
//! Input is translated here and nowhere else. `SCREENS.md §11` wants actions rather than
//! keys, and the translation is a table in [`Bindings`], so a project's own profile is a
//! value rather than a code path.

use std::sync::Arc;
use std::time::{Duration, Instant};

use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key as WinitKey, NamedKey};
use winit::window::{Window as WinitWindow, WindowId};

use crate::action::{Action, Bindings, Key};

/// How often the app is asked for a frame while nothing is happening.
///
/// A redraw-on-event loop draws only when a key arrives, which is enough for a still scene and
/// not enough for what comes later: a title-screen animation, a scripted auto-advance, and — now
/// — hot reload, which must notice a file change without the player touching anything. Five a
/// second is slow enough to cost nothing and fast enough that an edit is seen immediately.
const TICK: Duration = Duration::from_millis(200);

/// What a window is asked for.
#[derive(Clone, PartialEq, Debug)]
pub struct Config {
    /// The title.
    pub title: String,
    /// The initial size in pixels.
    pub size: (u32, u32),
    /// The input profile.
    pub bindings: Bindings,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            title: "Vela".to_string(),
            size: (1280, 720),
            bindings: Bindings::new(),
        }
    }
}

/// A window, as the rest of the engine sees one.
#[derive(Clone)]
pub struct Window {
    inner: Arc<WinitWindow>,
}

impl Window {
    /// The size in pixels.
    #[must_use]
    pub fn size(&self) -> (u32, u32) {
        let size = self.inner.inner_size();
        (size.width.max(1), size.height.max(1))
    }

    /// Asks for a redraw.
    pub fn request_redraw(&self) {
        self.inner.request_redraw();
    }

    /// The windowing library's window.
    ///
    /// Deliberately not the public shape of this type: it exists so `vela-render` can hand
    /// the handle to `wgpu`, which is a GPU need rather than a windowing one. Every other
    /// caller should use [`Self::size`] and the [`Action`] stream.
    #[must_use]
    pub fn raw(&self) -> Arc<WinitWindow> {
        self.inner.clone()
    }
}

/// What an application does with a window.
pub trait App {
    /// The window exists and can be drawn into.
    fn opened(&mut self, window: &Window);

    /// One semantic action arrived.
    fn action(&mut self, action: Action, window: &Window);

    /// Draw a frame.
    fn frame(&mut self, window: &Window);

    /// Whether the application wants the window closed.
    ///
    /// The host answers its own `Quit` binding by exiting, but a story can also quit itself —
    /// a screen's `quit()` action — and the app has no event loop to exit. Asking after each
    /// action and frame is how that reaches the same door without the app holding the loop.
    fn should_close(&self) -> bool {
        false
    }
}

/// Runs `app` until it asks to quit.
///
/// # Errors
///
/// Returns the platform's failure if the event loop cannot be created or run, because a
/// window that silently fails to open is worse than a program that says it could not.
pub fn run(config: Config, app: &mut dyn App) -> Result<(), Box<dyn std::error::Error>> {
    let event_loop = EventLoop::new()?;
    let mut runner = Runner {
        config,
        app,
        window: None,
        next_tick: Instant::now() + TICK,
    };
    event_loop.run_app(&mut runner)?;
    Ok(())
}

/// The `winit` handler that forwards to an [`App`].
struct Runner<'a> {
    config: Config,
    app: &'a mut dyn App,
    window: Option<Window>,
    /// When the next idle frame is due.
    next_tick: Instant,
}

impl ApplicationHandler for Runner<'_> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let attributes = WinitWindow::default_attributes()
            .with_title(self.config.title.clone())
            .with_inner_size(winit::dpi::PhysicalSize::new(
                self.config.size.0,
                self.config.size.1,
            ));
        let Ok(inner) = event_loop.create_window(attributes) else {
            event_loop.exit();
            return;
        };
        let window = Window {
            inner: Arc::new(inner),
        };
        self.app.opened(&window);
        window.request_redraw();
        self.window = Some(window);
        self.next_tick = Instant::now() + TICK;
        event_loop.set_control_flow(ControlFlow::WaitUntil(self.next_tick));
    }

    /// Wakes the app on the tick so it can do what only time can prompt — hot reload now,
    /// animation later.
    ///
    /// `WaitUntil` rather than `Poll`: the loop must sleep when there is nothing to do, and
    /// asking for a redraw on every wakeup would spin a core to draw the same frame.
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let now = Instant::now();
        if now >= self.next_tick {
            if let Some(window) = &self.window {
                window.request_redraw();
            }
            self.next_tick = now + TICK;
        }
        event_loop.set_control_flow(ControlFlow::WaitUntil(self.next_tick));
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(window) = self.window.clone() else {
            return;
        };
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(_) => window.request_redraw(),
            WindowEvent::RedrawRequested => {
                self.app.frame(&window);
                if self.app.should_close() {
                    event_loop.exit();
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if std::env::var_os("VELA_INPUT_TRACE").is_some() {
                    eprintln!("input: key {:?} state {:?}", event.logical_key, event.state);
                }
                // Presses only. A held key repeats, and `Repeat` would advance twice.
                if event.state != ElementState::Pressed {
                    return;
                }
                let Some(key) = translate(&event.logical_key) else {
                    return;
                };
                if let Some(action) = self.config.bindings.action(key) {
                    if action == Action::Quit {
                        event_loop.exit();
                        return;
                    }
                    self.app.action(action, &window);
                    if self.app.should_close() {
                        event_loop.exit();
                        return;
                    }
                    window.request_redraw();
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                if std::env::var_os("VELA_INPUT_TRACE").is_some() {
                    eprintln!("input: mouse {button:?} {state:?}");
                }
                if state != ElementState::Pressed {
                    return;
                }
                let number: u8 = match button {
                    MouseButton::Left => 1,
                    MouseButton::Right => 2,
                    MouseButton::Middle => 3,
                    MouseButton::Back => 4,
                    MouseButton::Forward => 5,
                    // Only the low byte is kept: a profile names a button, and button 300 does
                    // not exist.
                    MouseButton::Other(n) => n as u8,
                };
                if let Some(action) = self.config.bindings.action(Key::Mouse(number)) {
                    self.app.action(action, &window);
                    if self.app.should_close() {
                        event_loop.exit();
                        return;
                    }
                    window.request_redraw();
                }
            }
            _ => {}
        }
    }
}

/// A `winit` key as a binding-profile key.
///
/// Returns `None` for anything the profile cannot name, which is most keys — a story that
/// bound every key would be a story whose author did not choose.
#[must_use]
pub fn translate(key: &WinitKey) -> Option<Key> {
    match key {
        WinitKey::Character(text) => {
            let mut characters = text.chars();
            match (characters.next(), characters.next()) {
                (Some(c), None) => Some(Key::Char(c.to_ascii_lowercase())),
                _ => None,
            }
        }
        WinitKey::Named(named) => match named {
            NamedKey::Enter => Some(Key::Enter),
            NamedKey::Space => Some(Key::Space),
            NamedKey::Escape => Some(Key::Escape),
            NamedKey::Shift => Some(Key::Shift),
            NamedKey::Control => Some(Key::Control),
            NamedKey::Tab => Some(Key::Tab),
            NamedKey::Backspace => Some(Key::Backspace),
            NamedKey::ArrowUp => Some(Key::Up),
            NamedKey::ArrowDown => Some(Key::Down),
            NamedKey::ArrowLeft => Some(Key::Left),
            NamedKey::ArrowRight => Some(Key::Right),
            _ => None,
        },
        _ => None,
    }
}
