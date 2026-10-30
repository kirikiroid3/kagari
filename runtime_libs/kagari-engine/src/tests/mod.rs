//! Engine tests on compiled fixtures: the story, saves, settings, reveal timing, and the reactive
//! graph and focus through a booted engine.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod debug;
mod focus;
mod reactive;
mod reveal;
mod saves;
mod settings;
mod story;

use std::collections::HashMap;

use kagari_vm::{HostObj, StaticId, Value};

use crate::frame::{Frame, Node, Reveal};
use crate::saves::Timestamp;
use crate::story::narrative::{NarrativeImport, NarrativeObj};
use crate::story::{Current, text_plain};
use crate::test_support::compile;
use crate::{AssetStore, Engine, InputEvent, Key, MouseButton, Platform};

/// The font every font asset reads as.
const TEST_FONT: &[u8] = include_bytes!("../../../kagari-text/tests/fonts/kagari-test-cjk.otf");

/// The settings statics, the font, `main` and a bare game view; a fixture adds `Title`.
pub(crate) const SYSTEM: &str = r#"
use std::narrative::text::text_style;
use std::screen::size;
use std::system::default_bindings;

static TEXT_SPEED: Setting<Duration> = setting(ms(30));
static AUTO_DELAY: Setting<Duration> = setting(ms(1300));
static AUTO_DELAY_PER_CHAR: Setting<Duration> = setting(ms(60));
static SKIP_UNREAD: Setting<bool> = setting(false);
static MASTER: Setting<Volume> = setting(volume(100));
static MUSIC: Setting<Volume> = setting(volume(60));
static SOUND: Setting<Volume> = setting(volume(100));
static VOICE: Setting<Volume> = setting(volume(100));
static VOICE_SUSTAIN: Setting<bool> = setting(false);
static WAIT_FOR_VOICE: Setting<bool> = setting(true);
static WINDOW_OPACITY: Setting<Int<0, 100>> = setting(80);
static FULLSCREEN: Setting<bool> = setting(false);
static ANIMATIONS: Setting<bool> = setting(true);
const FONT: FontAsset = asset("font.ttf");

fn main() -> System {
    System {
        title: "Story",
        resolution: size(1280, 720),
        start: Title,
        game: GameView,
        save_slots: 20,
        auto_slots: 3,
        backlog: 200,
        font: FONT,
        text: text_style(26, WHITE),
        settings: Settings {
            text_speed: TEXT_SPEED,
            auto_delay: AUTO_DELAY,
            auto_delay_per_char: AUTO_DELAY_PER_CHAR,
            skip_unread: SKIP_UNREAD,
            master: MASTER,
            music: MUSIC,
            sound: SOUND,
            voice: VOICE,
            voice_sustain: VOICE_SUSTAIN,
            wait_for_voice: WAIT_FOR_VOICE,
            window_opacity: WINDOW_OPACITY,
            fullscreen: FULLSCREEN,
            animations: ANIMATIONS,
        },
        bindings: default_bindings(),
    }
}

fn GameView(game: Narrative) -> Element {
    game_screen!(game = game)
}
"#;

/// The story of §3.13: three lines, a choice, two more lines.
pub(crate) const STORY_SCENE: &str = r#"
use std::ui::spacer;

static mut COUNT: Int<0, 9> = 0;
enum Pick { A, B }

#[scene]
#[entry]
pushdown fn Start() {
    s!(t!["One."]);
    s!(t!["Two."]);
    s!(t!["Three."]);
    match choice!{id = "pick", t!["A"] => Pick::A, t!["B"] => Pick::B} {
        Pick::A => COUNT += 2,
        Pick::B => COUNT += 5,
    }
    s!(t!["Count " {COUNT}]);
    s!(t!["End."]);
}

fn Title() -> RootPage {
    nav::start(Start, (), |_end| ());
    RootPage(spacer())
}
"#;

pub(crate) fn story() -> String {
    format!("{SYSTEM}{STORY_SCENE}")
}

pub(crate) fn story2() -> String {
    story().replace(r#"id = "pick""#, r#"id = "pick2""#)
}

/// A platform with in-memory stores.
#[derive(Default)]
pub(crate) struct TestPlatform {
    pub stores: HashMap<String, Vec<u8>>,
    /// The reveal of every text node of each rendered thumbnail, in order.
    pub thumbnail_reveals: Vec<Vec<Reveal>>,
}

fn collect_reveals(nodes: &[Node], out: &mut Vec<Reveal>) {
    for node in nodes {
        match node {
            Node::Text { reveal, .. } => out.push(*reveal),
            Node::Group { children, .. } => collect_reveals(children, out),
            Node::Transition { from, to, .. } => {
                collect_reveals(from, out);
                collect_reveals(to, out);
            }
            _ => {}
        }
    }
}

impl Platform for TestPlatform {
    fn read_store(&mut self, name: &str) -> std::io::Result<Option<Vec<u8>>> {
        Ok(self.stores.get(name).cloned())
    }

    fn write_store(&mut self, name: &str, bytes: &[u8]) -> std::io::Result<()> {
        self.stores.insert(name.to_owned(), bytes.to_vec());
        Ok(())
    }

    fn delete_store(&mut self, name: &str) -> std::io::Result<()> {
        self.stores.remove(name);
        Ok(())
    }

    fn read_asset(&mut self, path: &str) -> std::io::Result<Option<Vec<u8>>> {
        if path.ends_with(".ttf") || path.ends_with(".otf") {
            return Ok(Some(TEST_FONT.to_vec()));
        }
        Ok(Some(Vec::new()))
    }

    fn audio_duration_ms(&mut self, _path: &str, _bytes: &[u8]) -> Option<u32> {
        Some(1000)
    }

    fn render_thumbnail(
        &mut self,
        frame: &Frame,
        _assets: &AssetStore,
        width: u32,
        height: u32,
    ) -> Option<Vec<u8>> {
        let mut reveals = Vec::new();
        collect_reveals(&frame.nodes, &mut reveals);
        self.thumbnail_reveals.push(reveals);
        let len = width.saturating_mul(height).saturating_mul(4);
        Some(vec![0; len as usize])
    }

    fn local_time(&self) -> Timestamp {
        Timestamp {
            year: 2026,
            month: 1,
            day: 2,
            hour: 3,
            minute: 4,
            second: 5,
        }
    }

    fn random_seed(&mut self) -> u64 {
        7
    }

    fn set_title(&mut self, _title: &str) {}
}

pub(crate) fn boot(source: &str, p: &mut TestPlatform) -> Engine {
    let engine = Engine::boot(&compile(source), p, 0).unwrap();
    assert_eq!(engine.crash(), None);
    engine
}

/// Presses and releases `key` at `now`.
pub(crate) fn key(e: &mut Engine, p: &mut TestPlatform, key: Key, now: u64) {
    e.input(InputEvent::Key { key, pressed: true }, p, now);
    e.input(
        InputEvent::Key {
            key,
            pressed: false,
        },
        p,
        now,
    );
}

pub(crate) fn mouse(e: &mut Engine, p: &mut TestPlatform, button: MouseButton, now: u64) {
    e.input(
        InputEvent::MouseButton {
            button,
            pressed: true,
        },
        p,
        now,
    );
    e.input(
        InputEvent::MouseButton {
            button,
            pressed: false,
        },
        p,
        now,
    );
}

/// The text of the current line.
pub(crate) fn line(e: &Engine) -> Option<String> {
    match &e.state.game.as_ref()?.current {
        Current::Say(say) => say.text.as_ref().map(text_plain),
        _ => None,
    }
}

pub(crate) fn at_choice(e: &Engine) -> bool {
    matches!(
        e.state.game.as_ref().map(|g| &g.current),
        Some(Current::Choice(_))
    )
}

/// Calls a `Narrative` import of the running game.
pub(crate) fn narrative(
    e: &mut Engine,
    p: &mut TestPlatform,
    import: NarrativeImport,
    extra: Vec<Value>,
    now: u64,
) -> Value {
    let generation = e.state.game.as_ref().unwrap().generation;
    let mut args = vec![Value::Host(HostObj::new(NarrativeObj { generation }))];
    args.extend(extra);
    let mut out = None;
    e.with_ctx(p, now, |ctx, vm| {
        out = Some(import.call(ctx, vm, args)?);
        Ok(())
    });
    out.unwrap()
}

/// The static at `path`.
pub(crate) fn static_id(e: &Engine, path: &str) -> StaticId {
    let program = &e.state.program;
    let index = program
        .module()
        .statics
        .iter()
        .position(|s| program.string(s.path) == path)
        .unwrap_or_else(|| panic!("no static `{path}`"));
    StaticId(index as u32)
}

/// Advances `n` lines from a fully revealed state: each takes two presses (complete, next).
pub(crate) fn advance_lines(e: &mut Engine, p: &mut TestPlatform, n: usize, now: &mut u64) {
    for _ in 0..n {
        for _ in 0..2 {
            *now = now.saturating_add(10);
            key(e, p, Key::Space, *now);
            e.update(p, *now);
        }
    }
}
