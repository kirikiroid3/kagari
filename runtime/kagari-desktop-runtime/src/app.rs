//! The windowed runtime: a winit event loop driving the engine, the renderer and the audio.

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use kagari_engine::{Engine, InputEvent};
use kagari_render::Viewport;
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, StartCause, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Fullscreen, Window, WindowId};

use crate::audio::AudioOut;
use crate::cli::{BackendArg, Cli};
use crate::debug::DebugPort;
use crate::exit::{Failure, STOPPED, WINDOW};
use crate::game;
use crate::gpu;
use crate::input::{self, HeldInputs};
use crate::pads::Pads;
use crate::platform::{DesktopPlatform, SeedSource, TimeSource};
use crate::store::Store;

/// The wake-up interval while nothing can be drawn (occluded or minimized).
const IDLE_WAIT: Duration = Duration::from_millis(16);

/// Boots the game, opens the window and runs until it closes; returns the exit status.
pub fn run(cli: &Cli, dir: &Path, bytes: &[u8]) -> Result<u8, Failure> {
    let store = match &cli.data_dir {
        Some(dir) => Store::fixed(dir.clone()),
        None => Store::by_title(),
    };
    let mut platform = DesktopPlatform::new(dir, store, TimeSource::Local, SeedSource::System);
    let mut engine = game::boot(bytes, &mut platform)?;
    let debug = DebugPort::open(cli, dir, &mut engine)?;

    let event_loop = build_event_loop(cli.backend)?;
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_with_display_handle(
        Box::new(event_loop.owned_display_handle()),
    ));
    let ws = engine.window();
    let mut app = App {
        engine,
        platform,
        audio: AudioOut::new(),
        pads: Pads::new(),
        input: HeldInputs::default(),
        start: None,
        instance: Some(instance),
        win: None,
        applied: (ws.title, ws.fullscreen),
        design: ws.resolution,
        exit: None,
        idle: false,
        debug,
    };
    event_loop
        .run_app(&mut app)
        .map_err(|e| Failure::new(WINDOW, format!("cannot open a window: {e}")))?;
    Ok(app.exit.unwrap_or(0))
}

#[cfg(target_os = "linux")]
fn build_event_loop(backend: Option<BackendArg>) -> Result<EventLoop<()>, Failure> {
    use winit::platform::wayland::EventLoopBuilderExtWayland;
    use winit::platform::x11::EventLoopBuilderExtX11;
    let wayland = match backend {
        Some(BackendArg::Wayland) => true,
        Some(BackendArg::X11) => false,
        None => std::env::var_os("WAYLAND_DISPLAY").is_some_and(|v| !v.is_empty()),
    };
    let mut builder = EventLoop::builder();
    if wayland {
        builder.with_wayland();
    } else {
        builder.with_x11();
    }
    log::info!(
        target: "kagari::desktop",
        "window backend: {}",
        if wayland { "wayland" } else { "x11" }
    );
    builder
        .build()
        .map_err(|e| Failure::new(WINDOW, format!("cannot open a window: {e}")))
}

#[cfg(not(target_os = "linux"))]
fn build_event_loop(backend: Option<BackendArg>) -> Result<EventLoop<()>, Failure> {
    if backend.is_some() {
        log::warn!("`--backend` only applies on Linux; ignored");
    }
    EventLoop::builder()
        .build()
        .map_err(|e| Failure::new(WINDOW, format!("cannot open a window: {e}")))
}

struct WindowCtx {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    viewport: Viewport,
    /// The window has no area (minimized); nothing is drawn.
    empty: bool,
}

struct App {
    engine: Engine,
    platform: DesktopPlatform,
    audio: AudioOut,
    pads: Pads,
    input: HeldInputs,
    /// Engine time 0. The engine boots at 0; the clock starts once the window and the GPU are
    /// ready, just before the first frame, so their setup time does not eat into the first
    /// animations.
    start: Option<Instant>,
    /// Kept until the first `resumed` creates the GPU from it.
    instance: Option<wgpu::Instance>,
    win: Option<WindowCtx>,
    /// The title and fullscreen state the window has.
    applied: (String, bool),
    /// The design resolution (`System::resolution`).
    design: (u32, u32),
    exit: Option<u8>,
    /// Nothing could be drawn this frame: wait instead of spinning.
    idle: bool,
    /// The debug channel, when `--debug-port` or `--debug-stdio` opened one.
    debug: Option<DebugPort>,
}

impl App {
    fn now(&self) -> u64 {
        self.start.map_or(0, |start| {
            u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX)
        })
    }

    /// Prints the failure and stops the event loop with its code.
    fn fail(&mut self, event_loop: &ActiveEventLoop, failure: Failure) {
        eprintln!("kagari-desktop-runtime: {}", failure.message);
        self.exit = Some(failure.code);
        event_loop.exit();
    }

    fn send(&mut self, event: InputEvent) {
        let now = self.now();
        self.engine.input(event, &mut self.platform, now);
    }

    /// Creates the window at its design size (shrunk to 90 % of the monitor), the surface and
    /// the GPU.
    fn open(&mut self, event_loop: &ActiveEventLoop) -> Result<(), Failure> {
        let ws = self.engine.window();
        let (dw, dh) = (f64::from(ws.resolution.0), f64::from(ws.resolution.1));
        let monitor = event_loop
            .primary_monitor()
            .or_else(|| event_loop.available_monitors().next());
        let scale = monitor.map_or(1.0, |m| {
            let factor = m.scale_factor();
            let size = m.size();
            let (mw, mh) = (
                f64::from(size.width) / factor,
                f64::from(size.height) / factor,
            );
            1.0_f64.min(0.9 * mw / dw).min(0.9 * mh / dh)
        });
        let attributes = Window::default_attributes()
            .with_title(&ws.title)
            .with_inner_size(LogicalSize::new(dw * scale, dh * scale))
            .with_fullscreen(ws.fullscreen.then_some(Fullscreen::Borderless(None)));
        let window = Arc::new(
            event_loop
                .create_window(attributes)
                .map_err(|e| Failure::new(WINDOW, format!("cannot create the window: {e}")))?,
        );
        let instance = self.instance.take().unwrap_or_else(|| {
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_with_display_handle(Box::new(
                event_loop.owned_display_handle(),
            )))
        });
        let surface = instance
            .create_surface(window.clone())
            .map_err(|e| Failure::new(WINDOW, format!("cannot create the window surface: {e}")))?;
        let gpu = gpu::create(instance, Some(&surface))?;
        let size = window.inner_size();
        let config = gpu.surface_config(size.width, size.height);
        let empty = size.width == 0 || size.height == 0;
        if !empty {
            surface.configure(&gpu.device, &config);
        }
        self.platform.gpu = Some(gpu);
        self.design = ws.resolution;
        self.applied = (ws.title, ws.fullscreen);
        window.request_redraw();
        self.start.get_or_insert_with(Instant::now);
        self.win = Some(WindowCtx {
            window,
            surface,
            viewport: Viewport::letterbox(self.design, (config.width, config.height)),
            config,
            empty,
        });
        Ok(())
    }

    fn resize(&mut self, width: u32, height: u32) {
        let (Some(win), Some(gpu)) = (self.win.as_mut(), self.platform.gpu.as_ref()) else {
            return;
        };
        win.empty = width == 0 || height == 0;
        if win.empty {
            self.idle = true;
            return;
        }
        win.config.width = width;
        win.config.height = height;
        win.surface.configure(&gpu.device, &win.config);
        win.viewport = Viewport::letterbox(self.design, (width, height));
    }

    /// One frame: input from pads, update, audio, window state, then draw and present.
    fn frame(&mut self, event_loop: &ActiveEventLoop) {
        let now = self.now();
        for event in self.pads.poll() {
            self.engine.input(event, &mut self.platform, now);
        }
        if let Some(debug) = self.debug.as_mut() {
            debug.poll(&mut self.engine, &mut self.platform, now);
        }
        self.engine.update(&mut self.platform, now);
        if self.exit.is_none()
            && let Some(message) = self.engine.crash()
        {
            eprintln!("kagari-desktop-runtime: game stopped: {message}");
            self.exit = Some(STOPPED);
        }
        if let Some(Err(failure)) = self.platform.gpu.as_ref().map(gpu::Gpu::check) {
            self.fail(event_loop, failure);
            return;
        }
        let commands = self.engine.take_audio();
        self.audio.apply(commands, &mut self.platform);

        let ws = self.engine.window();
        if ws.quit {
            event_loop.exit();
            return;
        }
        self.design = ws.resolution;
        if let Some(win) = &self.win {
            if ws.title != self.applied.0 {
                win.window.set_title(&ws.title);
            }
            if ws.fullscreen != self.applied.1 {
                win.window
                    .set_fullscreen(ws.fullscreen.then_some(Fullscreen::Borderless(None)));
            }
        }
        self.applied = (ws.title, ws.fullscreen);

        let frame = self.engine.frame(now);
        if let Some(gpu) = self.platform.gpu.as_mut() {
            game::upload_thumbnails(&mut gpu.renderer, &self.engine, &frame);
        }
        if !self.present(&frame, event_loop)
            && frame.capture_still
            && let Some(gpu) = self.platform.gpu.as_mut()
        {
            // The engine took the still from this frame and draws `Node::Still` from now on:
            // capture it even though nothing reached the screen.
            gpu.renderer.render_offscreen(&frame, self.engine.assets());
        }
    }

    /// Draws `frame` into the window and presents it; false when nothing was drawn.
    fn present(&mut self, frame: &kagari_engine::Frame, event_loop: &ActiveEventLoop) -> bool {
        let (Some(win), Some(gpu)) = (self.win.as_mut(), self.platform.gpu.as_mut()) else {
            return false;
        };
        if win.empty {
            self.idle = true;
            return false;
        }
        let (texture, suboptimal) = match win.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t) => (t, false),
            wgpu::CurrentSurfaceTexture::Suboptimal(t) => (t, true),
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                self.idle = true;
                return false;
            }
            wgpu::CurrentSurfaceTexture::Outdated => {
                win.surface.configure(&gpu.device, &win.config);
                return false;
            }
            wgpu::CurrentSurfaceTexture::Lost => {
                self.recreate_surface(event_loop);
                return false;
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                log::error!("surface validation error; frame skipped");
                return false;
            }
        };
        let view = texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        gpu.renderer
            .render(frame, self.engine.assets(), &view, win.viewport);
        gpu.queue.present(texture);
        if suboptimal {
            win.surface.configure(&gpu.device, &win.config);
        }
        true
    }

    /// Replaces a lost surface. The old one is dropped first: a window may hold only one
    /// surface at a time (Vulkan `NATIVE_WINDOW_IN_USE`).
    fn recreate_surface(&mut self, event_loop: &ActiveEventLoop) {
        let Some(gpu) = self.platform.gpu.as_ref() else {
            return;
        };
        let Some(WindowCtx {
            window,
            surface,
            config,
            viewport,
            empty,
        }) = self.win.take()
        else {
            return;
        };
        drop(surface);
        match gpu.instance.create_surface(window.clone()) {
            Ok(surface) => {
                surface.configure(&gpu.device, &config);
                self.win = Some(WindowCtx {
                    window,
                    surface,
                    config,
                    viewport,
                    empty,
                });
            }
            Err(e) => {
                let failure =
                    Failure::new(WINDOW, format!("cannot create the window surface: {e}"));
                self.fail(event_loop, failure);
            }
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.win.is_some() {
            return;
        }
        if let Err(failure) = self.open(event_loop) {
            self.fail(event_loop, failure);
        }
    }

    fn new_events(&mut self, _event_loop: &ActiveEventLoop, cause: StartCause) {
        if let StartCause::ResumeTimeReached { .. } = cause
            && let Some(win) = &self.win
        {
            win.window.request_redraw();
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                self.engine.quit(&mut self.platform);
                event_loop.exit();
            }
            WindowEvent::Resized(size) => self.resize(size.width, size.height),
            WindowEvent::KeyboardInput { event, .. } => {
                if let Some(e) = self.input.key(&event) {
                    self.send(e);
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                if let Some(button) = input::map_mouse(button) {
                    self.send(InputEvent::MouseButton {
                        button,
                        pressed: state == ElementState::Pressed,
                    });
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                if let Some(e) = input::wheel(delta) {
                    self.send(e);
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                if let Some(win) = &self.win {
                    let (x, y) = win.viewport.to_design(self.design, position.x, position.y);
                    self.send(InputEvent::MouseMove { x, y });
                }
            }
            WindowEvent::CursorLeft { .. } => self.send(InputEvent::MouseMove { x: -1.0, y: -1.0 }),
            WindowEvent::Focused(focused) => {
                if !focused {
                    self.input.clear();
                }
                self.send(InputEvent::Focus(focused));
            }
            WindowEvent::RedrawRequested => self.frame(event_loop),
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self.idle {
            self.idle = false;
            let until = Instant::now().checked_add(IDLE_WAIT);
            event_loop.set_control_flow(until.map_or(ControlFlow::Wait, ControlFlow::WaitUntil));
        } else {
            event_loop.set_control_flow(ControlFlow::Wait);
            if let Some(win) = &self.win {
                win.window.request_redraw();
            }
        }
    }
}
