//! Headless mode: no window, no audio device, no gamepads; a script drives the input and the
//! clock, and `capture` writes PNGs.

pub mod script;

use std::fs;
use std::io::BufWriter;
use std::path::{Path, PathBuf};

use kagari_engine::{Engine, InputEvent, Key, MouseButton};

use crate::cli::Cli;
use crate::exit::{Failure, SCRIPT, WINDOW};
use crate::game;
use crate::platform::{DesktopPlatform, HEADLESS_TIME, SeedSource, TimeSource};
use crate::store::Store;
use script::{Command, ScriptError};

/// The clock step of `wait`.
const TICK_MS: u64 = 16;

/// Runs the script; `Ok` is exit 0.
pub fn run(cli: &Cli, dir: &Path, bytes: &[u8]) -> Result<(), Failure> {
    let (Some(script_path), Some(out)) = (&cli.script, &cli.out) else {
        return Err(Failure::new(SCRIPT, "--headless needs --script and --out"));
    };
    let text = fs::read_to_string(script_path)
        .map_err(|e| Failure::new(SCRIPT, format!("{}: {e}", script_path.display())))?;
    let commands = script::parse(&text)?;
    fs::create_dir_all(out).map_err(|e| Failure::new(SCRIPT, format!("{}: {e}", out.display())))?;

    let data = cli.data_dir.clone().unwrap_or_else(|| out.join("data"));
    let mut platform = DesktopPlatform::new(
        dir,
        Store::fixed(data),
        TimeSource::Fixed(HEADLESS_TIME),
        SeedSource::Fixed(cli.seed.unwrap_or(1)),
    );
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    platform.gpu = Some(crate::gpu::create(instance, None)?);
    let engine = game::boot(bytes, &mut platform)?;
    Session {
        engine,
        platform,
        now: 0,
        expected: false,
        out: out.clone(),
    }
    .run(commands)
}

struct Session {
    engine: Engine,
    platform: DesktopPlatform,
    now: u64,
    /// `expect-crash` saw the crash.
    expected: bool,
    out: PathBuf,
}

impl Session {
    fn run(mut self, commands: Vec<(u32, Command)>) -> Result<(), Failure> {
        for (line, command) in commands {
            if self.engine.window().quit {
                break;
            }
            let crash = self.engine.crash().map(str::to_owned);
            match (&command, crash) {
                (Command::ExpectCrash, Some(m)) => {
                    if !self.expected {
                        log::info!("expected crash: {m}");
                    }
                    self.expected = true;
                }
                (Command::ExpectCrash, None) => {
                    return Err(ScriptError {
                        line,
                        message: "expected a crash, but the game is running".to_owned(),
                    }
                    .into());
                }
                (Command::AssertNoCrash, Some(m)) => return Err(Failure::stopped(&m)),
                (Command::Capture(name), _) => self.capture(name)?,
                (_, Some(m)) if !self.expected => return Err(Failure::stopped(&m)),
                (_, Some(_)) => {}
                (_, None) => self.command(&command),
            }
            self.check_gpu()?;
        }
        if let Some(m) = self.engine.crash() {
            if !self.expected {
                return Err(Failure::stopped(m));
            }
        } else if !self.engine.window().quit {
            self.engine.quit(&mut self.platform);
        }
        Ok(())
    }

    /// An input or wait command of a running game.
    fn command(&mut self, command: &Command) {
        match *command {
            Command::Wait(ms) => self.wait(ms),
            Command::Key(key) => {
                self.key(key, true);
                self.key(key, false);
            }
            Command::Down(key) => self.key(key, true),
            Command::Up(key) => self.key(key, false),
            Command::Click(x, y) => {
                self.input(InputEvent::MouseMove { x, y });
                self.input(InputEvent::MouseButton {
                    button: MouseButton::Left,
                    pressed: true,
                });
                self.input(InputEvent::MouseButton {
                    button: MouseButton::Left,
                    pressed: false,
                });
            }
            Command::Move(x, y) => self.input(InputEvent::MouseMove { x, y }),
            Command::Wheel(delta_y) => self.input(InputEvent::Wheel { delta_y }),
            Command::Capture(_) | Command::ExpectCrash | Command::AssertNoCrash => return,
        }
        if !matches!(command, Command::Wait(_)) {
            self.engine.update(&mut self.platform, self.now);
            drop(self.engine.take_audio());
        }
    }

    fn key(&mut self, key: Key, pressed: bool) {
        self.input(InputEvent::Key { key, pressed });
    }

    fn input(&mut self, event: InputEvent) {
        self.engine.input(event, &mut self.platform, self.now);
    }

    /// Advances the clock in ticks of at most [`TICK_MS`], updating and drawing each one.
    fn wait(&mut self, ms: u64) {
        let mut left = ms;
        while left > 0 {
            let step = left.min(TICK_MS);
            left = left.saturating_sub(step);
            self.now = self.now.saturating_add(step);
            self.engine.update(&mut self.platform, self.now);
            drop(self.engine.take_audio());
            let frame = self.engine.frame(self.now);
            if frame.capture_still
                && let Some(gpu) = self.platform.gpu.as_mut()
            {
                gpu.renderer.render_offscreen(&frame, self.engine.assets());
            }
            if self.engine.window().quit {
                break;
            }
        }
    }

    /// Writes the current frame to `OUT/{name}.png`.
    fn capture(&mut self, name: &str) -> Result<(), Failure> {
        let path = self.out.join(format!("{name}.png"));
        let frame = self.engine.frame(self.now);
        let Some(gpu) = self.platform.gpu.as_mut() else {
            return Err(Failure::new(WINDOW, "no GPU"));
        };
        game::upload_thumbnails(&mut gpu.renderer, &self.engine, &frame);
        let (width, height) = frame.size;
        let rgba = gpu
            .renderer
            .render_to_rgba(&frame, self.engine.assets(), width, height);
        gpu.check()?;
        if rgba.is_empty() {
            return Err(Failure::new(
                SCRIPT,
                format!("{}: the frame could not be read back", path.display()),
            ));
        }
        write_png(&path, width, height, &rgba)
            .map_err(|e| Failure::new(SCRIPT, format!("{}: {e}", path.display())))?;
        println!("wrote {}", path.display());
        Ok(())
    }

    fn check_gpu(&self) -> Result<(), Failure> {
        self.platform
            .gpu
            .as_ref()
            .map_or(Ok(()), crate::gpu::Gpu::check)
    }
}

fn write_png(path: &Path, width: u32, height: u32, rgba: &[u8]) -> Result<(), png::EncodingError> {
    let file = fs::File::create(path)?;
    let mut encoder = png::Encoder::new(BufWriter::new(file), width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header()?;
    writer.write_image_data(rgba)?;
    writer.finish()
}
