#![deny(
    clippy::unwrap_used,
    clippy::panic,
    clippy::expect_used,
    clippy::arithmetic_side_effects
)]

//! The Kagari engine: the std host imports over `kagari-vm`, the screen model with its
//! timeline, animation, the audio model, and the `Frame` draw list a renderer consumes. It has
//! no platform dependencies: a [`Platform`] supplies storage, assets, time and thumbnails.

/// An import enum: one variant per std path, with `from_path` and `path`.
macro_rules! import_table {
    ($(#[$meta:meta])* $name:ident { $($variant:ident = $path:literal),* $(,)? }) => {
        $(#[$meta])*
        #[derive(Clone, Copy, PartialEq, Eq, Debug)]
        pub(crate) enum $name { $($variant),* }

        impl $name {
            const ALL: &'static [($name, &'static str)] = &[$(($name::$variant, $path)),*];

            /// The import whose std path is `path`.
            pub fn from_path(path: &str) -> Option<$name> {
                Self::ALL.iter().find(|(_, p)| *p == path).map(|(import, _)| *import)
            }

            /// The std path of this import.
            pub fn path(self) -> &'static str {
                Self::ALL.iter().find(|(import, _)| *import == self).map_or("", |(_, p)| p)
            }
        }
    };
}

mod anim;
mod assets;
mod audio;
pub mod debug;
mod frame;
mod host;
mod input;
mod saves;
mod screen;
mod store;
mod story;
mod swap;
mod system;
#[cfg(test)]
mod test_support;
#[cfg(test)]
mod tests;
mod ui;
mod values;

use std::fmt;
use std::rc::Rc;
use std::sync::Arc;

use kagari_vm::{Program, Trap, Vm};
use sha2::Digest;

pub use assets::{AssetStore, ImageData, Thumbnail};
pub use audio::{AudioCommand, Bus};
pub use frame::{
    Affine, Dir, Frame, MotionStage, Node, Pose, Rect, Reveal, ShaderEffect, ThumbKey,
    TransitionDraw,
};
pub use input::{InputEvent, Key, MouseButton, PadButton};
pub use saves::Timestamp;

/// What the engine needs from the machine it runs on.
pub trait Platform {
    fn read_store(&mut self, name: &str) -> std::io::Result<Option<Vec<u8>>>;
    fn write_store(&mut self, name: &str, bytes: &[u8]) -> std::io::Result<()>;
    fn delete_store(&mut self, name: &str) -> std::io::Result<()>;
    /// The bytes of the asset at a bytecode asset path.
    fn read_asset(&mut self, path: &str) -> std::io::Result<Option<Vec<u8>>>;
    fn audio_duration_ms(&mut self, path: &str, bytes: &[u8]) -> Option<u32>;
    /// `frame` rendered offscreen at `width`×`height`: straight-alpha sRGB RGBA8, row-major;
    /// `None` on failure. `assets` supplies the images and, through `assets.fonts()`, the glyphs.
    fn render_thumbnail(
        &mut self,
        frame: &Frame,
        assets: &AssetStore,
        width: u32,
        height: u32,
    ) -> Option<Vec<u8>>;
    /// The local time, as the fields of `std::system::saves::Timestamp`.
    fn local_time(&self) -> Timestamp;
    /// The seed of a new story's PRNG (`nav::start`).
    fn random_seed(&mut self) -> u64;
    /// Called once by the boot, after `main` returned and before the first store call: the
    /// desktop's store directory depends on the title.
    fn set_title(&mut self, title: &str);
    /// A debugger swapped in a rebuilt program ([`Engine::debug_swap`]), before it boots: drop
    /// whatever was read or decoded from the old build's assets (open archives, decoded
    /// sounds, textures). Nothing by default.
    fn program_changed(&mut self) {}
}

/// The window the game asks for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WindowState {
    pub title: String,
    pub resolution: (u32, u32),
    pub fullscreen: bool,
    pub quit: bool,
}

/// Why the engine could not start.
#[derive(Debug)]
pub enum BootError {
    Load(kagari_vm::LoadError),
    /// A trap during boot, rendered with `Trap::render`.
    Trap(String),
    /// The path of an import no std module provides.
    UnsupportedImport(String),
    Asset(String),
}

impl fmt::Display for BootError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BootError::Load(error) => write!(f, "{error}"),
            BootError::Trap(message) | BootError::Asset(message) => f.write_str(message),
            BootError::UnsupportedImport(path) => write!(
                f,
                "the bytecode imports `{path}`, which this runtime does not provide"
            ),
        }
    }
}

impl std::error::Error for BootError {}

/// Everything the host imports read and change. The `Vm` stays outside, so imports can borrow
/// both.
pub(crate) struct EngineState {
    pub program: Rc<Program>,
    pub std: values::StdTypes,
    pub assets: assets::AssetStore,
    /// The system font and style and the layout cache; the fonts live in `assets`.
    pub text: ui::text::TextSystem,
    pub audio: audio::AudioModel,
    pub system: system::SystemState,
    /// The reactive graph and the element trees of the mounted modes.
    pub ui: ui::UiState,
    /// The running game.
    pub game: Option<story::GameFrame>,
    /// Navigation deferred until the VM returns (`system::modes::settle`).
    pub pending_nav: Vec<system::NavOp>,
    /// The debugger's hold on the running game and its step breakpoints.
    pub debug: debug::DebugState,
}

/// The receiver of every host import.
pub(crate) struct HostCtx<'a> {
    pub state: &'a mut EngineState,
    pub platform: &'a mut dyn Platform,
    pub now_ms: u64,
}

/// Writes the dirty stores.
pub(crate) fn flush_stores(ctx: &mut HostCtx, vm: &Vm) {
    let program = ctx.state.program.clone();
    let names = store::value::Names {
        program: &program,
        types: vm.types(),
    };
    ctx.state.system.stores.flush(&names, &mut *ctx.platform);
}

/// The script hash of a `game.kagari`: the lowercase hex SHA-256 of its bytes. Saves record it;
/// a debug artifact names the bytecode it belongs to by it.
pub fn script_hash(program_bytes: &[u8]) -> String {
    sha2::Sha256::digest(program_bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// A running game program.
pub struct Engine {
    vm: Vm,
    state: EngineState,
    /// The latest time `boot`, `input` or `update` received; `quit` autosaves at it.
    now_ms: u64,
}

impl EngineState {
    /// A state over `program` before `main` ran.
    pub(crate) fn new(vm: &mut Vm, program: Rc<Program>, bindings: host::Bindings) -> EngineState {
        let std = values::StdTypes::resolve(vm.types_mut());
        EngineState {
            assets: assets::AssetStore::new(program.clone()),
            std,
            text: ui::text::TextSystem::new(),
            audio: audio::AudioModel::new(),
            system: system::SystemState::new(bindings, store::Stores::initial(&program)),
            ui: ui::UiState::new(),
            game: None,
            pending_nav: Vec::new(),
            debug: debug::DebugState::default(),
            program,
        }
    }
}

impl Engine {
    /// Loads the bytecode, runs `main`, loads the stores and fonts, and opens `System::start`.
    pub fn boot(
        program_bytes: &[u8],
        platform: &mut dyn Platform,
        now_ms: u64,
    ) -> Result<Engine, BootError> {
        let program = Program::load(program_bytes).map_err(BootError::Load)?;
        let script_hash = script_hash(program_bytes);
        let mut vm = Vm::new(program.clone());
        let bindings = host::Bindings::new(&program)?;
        let mut state = EngineState::new(&mut vm, program.clone(), bindings);
        state.system.script_hash = script_hash;
        let mut ctx = HostCtx {
            state: &mut state,
            platform,
            now_ms,
        };
        let result = vm
            .call(&mut ctx, program.main(), Vec::new())
            .map_err(|trap| BootError::Trap(trap.render(&program)))?;
        if !ctx.state.pending_nav.is_empty() {
            ctx.state.pending_nav.clear();
            log::warn!("navigation during `main` ignored");
        }
        let config = system::SystemConfig::decode(&vm, &ctx.state.std, &result)?;
        let title = story::text_plain(&config.title);
        ctx.platform.set_title(&title);
        ctx.state.system.stores.load(&program, &mut *ctx.platform);
        let asset_error = |trap: Trap| {
            BootError::Asset(match trap.kind {
                kagari_vm::TrapKind::Host(message) => message,
                kind => Trap::new(kind).to_string(),
            })
        };
        ctx.state
            .assets
            .load_fonts(&mut *ctx.platform)
            .map_err(asset_error)?;
        let font = ctx.state.assets.font(config.font).ok_or_else(|| {
            BootError::Asset(format!(
                "the system font `{}` is not loaded",
                ctx.state.assets.path(config.font)
            ))
        })?;
        let style = ctx
            .state
            .text
            .decode_style(&ctx.state.assets, &config.text)
            .map_err(asset_error)?;
        ctx.state.text.set_system(font, style);
        ctx.state.system.window = WindowState {
            title,
            resolution: config.resolution,
            fullscreen: false,
            quit: false,
        };
        let start = config.start.clone();
        ctx.state.system.config = Some(config);
        system::imports::apply_engine_settings(ctx.state)
            .map_err(|trap| BootError::Trap(trap.render(&program)))?;
        saves::load_all(&mut ctx);
        ctx.state.pending_nav.push(system::NavOp::Goto(start));
        if let Err(trap) = system::modes::settle(&mut ctx, &mut vm) {
            system::crash::crash(&mut ctx, trap);
        }
        Ok(Engine { vm, state, now_ms })
    }

    fn running(&self) -> bool {
        self.state.system.crashed.is_none() && !self.state.system.window.quit
    }

    /// Runs `f` with a host context; a trap stops the game.
    fn with_ctx(
        &mut self,
        platform: &mut dyn Platform,
        now_ms: u64,
        f: impl FnOnce(&mut HostCtx, &mut Vm) -> Result<(), Trap>,
    ) {
        let mut ctx = HostCtx {
            state: &mut self.state,
            platform,
            now_ms,
        };
        if let Err(trap) = f(&mut ctx, &mut self.vm) {
            system::crash::crash(&mut ctx, trap);
        }
    }

    pub fn input(&mut self, event: InputEvent, platform: &mut dyn Platform, now_ms: u64) {
        if !self.running() {
            return;
        }
        self.now_ms = now_ms;
        self.with_ctx(platform, now_ms, |ctx, vm| {
            input::dispatch(ctx, vm, event)?;
            system::modes::settle(ctx, vm)?;
            let state = &mut *ctx.state;
            audio::flush_heard(&mut state.audio, &mut state.system, &mut state.ui);
            Ok(())
        });
    }

    pub fn update(&mut self, platform: &mut dyn Platform, now_ms: u64) {
        if !self.running() {
            return;
        }
        self.now_ms = now_ms;
        self.with_ctx(platform, now_ms, |ctx, vm| {
            story::driver::tick(ctx, vm)?;
            if let Some(game) = ctx.state.game.as_mut() {
                let shown = game.compute_hud_shown(now_ms);
                if shown != game.hud_shown {
                    game.hud_shown = shown;
                    ctx.state.ui.invalidate(ui::SourceKey::NarrativeHud);
                }
            }
            system::modes::settle(ctx, vm)?;
            ui::flush(ctx, vm)?;
            system::modes::settle(ctx, vm)?;
            let state = &mut *ctx.state;
            if let Some(game) = state.game.as_mut() {
                let t = game.clock.story_ms(now_ms);
                game.screen.tick(t, &game.reveal, &mut state.audio);
            }
            audio::flush_heard(&mut state.audio, &mut state.system, &mut state.ui);
            flush_stores(ctx, vm);
            Ok(())
        });
    }

    pub fn frame(&mut self, now_ms: u64) -> Frame {
        let size = self.state.system.window.resolution;
        if let Some(crash) = &self.state.system.crashed {
            return system::crash::crash_frame(crash, size);
        }
        let (nodes, capture_still) = ui::draw::frame_nodes(&mut self.state, now_ms);
        Frame {
            size,
            nodes,
            capture_still,
        }
    }

    pub fn take_audio(&mut self) -> Vec<AudioCommand> {
        self.state.audio.take_commands()
    }

    pub fn window(&self) -> WindowState {
        self.state.system.window.clone()
    }

    /// Decoded images, the fonts (`fonts()`) and save thumbnails, for the renderer.
    pub fn assets(&self) -> &AssetStore {
        &self.state.assets
    }

    /// The rendered trap message once the game has stopped.
    pub fn crash(&self) -> Option<&str> {
        self.state
            .system
            .crashed
            .as_ref()
            .map(|c| c.message.as_str())
    }

    /// A save thumbnail: (generation, straight-alpha sRGB RGBA8, width, height).
    pub fn thumbnail(&self, slot: ThumbKey) -> Option<(u64, Arc<[u8]>, u32, u32)> {
        self.state
            .assets
            .thumbnail(slot)
            .map(|t| (t.generation, t.rgba.clone(), t.width, t.height))
    }

    /// Autosaves a game at a step, writes the stores and sets `WindowState::quit`.
    pub fn quit(&mut self, platform: &mut dyn Platform) {
        if self.state.system.window.quit {
            return;
        }
        let mut ctx = HostCtx {
            state: &mut self.state,
            platform,
            now_ms: self.now_ms,
        };
        system::modes::quit_now(&mut ctx, &mut self.vm);
    }
}
