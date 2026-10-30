//! The running game: the story fiber, its screen, the current step, the trace a save records,
//! the backlog and the play mode.

pub(crate) mod driver;
pub(crate) mod narrative;
pub(crate) mod reveal;

use std::collections::VecDeque;

use kagari_vm::{AssetId, FuncId, StepId, StoryFiber, Text, Value};

use crate::screen::runtime::ScreenRuntime;
use crate::values::{VAlign, Visibility};
use reveal::{ClickWait, MessageView, RevealRun, RevealState};

/// The running game.
pub(crate) struct GameFrame {
    pub generation: u64,
    pub entry: FuncId,
    /// Taken out while the fiber runs.
    pub fiber: Option<StoryFiber>,
    pub screen: ScreenRuntime,
    pub current: Current,
    /// The render function of the current step, as its record named it.
    pub render: Option<Value>,
    pub step_serial: u64,
    /// Story ms of the current step's entry.
    pub step_entered: u64,
    pub reveal: RevealRun,
    pub trace: Trace,
    pub backlog: VecDeque<BacklogEntry>,
    pub mode: PlayMode,
    pub hide_window: bool,
    pub skip_ready_at: Option<u64>,
    pub clock: StoryClock,
    pub on_exit: Option<Value>,
    pub pending_input: Option<StoryInput>,
    /// Whether the text window and quick menu count as shown (logical, or still fading out).
    pub hud_shown: (bool, bool),
}

impl GameFrame {
    pub fn new(
        generation: u64,
        entry: FuncId,
        fiber: StoryFiber,
        screen: ScreenRuntime,
        trace: Trace,
        on_exit: Option<Value>,
        now_ms: u64,
    ) -> GameFrame {
        let mut game = GameFrame {
            generation,
            entry,
            fiber: Some(fiber),
            screen,
            current: Current::None,
            render: None,
            step_serial: 0,
            step_entered: 0,
            reveal: RevealRun::empty(0),
            trace,
            backlog: VecDeque::new(),
            mode: PlayMode::Normal,
            hide_window: false,
            skip_ready_at: None,
            clock: StoryClock::new(now_ms),
            on_exit,
            pending_input: None,
            hud_shown: (true, true),
        };
        game.hud_shown = game.compute_hud_shown(now_ms);
        game
    }

    /// The hud visibilities the game view reads: shown while logical Shown or fading out.
    pub fn compute_hud_shown(&self, now_ms: u64) -> (bool, bool) {
        let h = self.screen.sample(self.clock.story_ms(now_ms)).hud;
        let l = &self.screen.logical().hud;
        (
            h.text_window > 0.0 || l.text_window == Visibility::Shown,
            h.quick_menu > 0.0 || l.quick_menu == Visibility::Shown,
        )
    }

    /// The current line for `message()`.
    pub fn message(&self, now_ms: u64) -> Option<MessageView> {
        let Current::Say(say) = &self.current else {
            return None;
        };
        let text = say.text.clone()?;
        let t = self.clock.story_ms(now_ms);
        let state = self.reveal.state(t);
        let click_wait = match state {
            RevealState::WaitingClick { .. } => ClickWait::Pause,
            RevealState::Done { .. }
                if !self.screen.timeline_running(t, &self.reveal)
                    && self.mode != PlayMode::Skip =>
            {
                ClickWait::Line
            }
            _ => ClickWait::None,
        };
        Some(MessageView {
            text,
            reveal: self.reveal.reveal(t),
            click_wait,
        })
    }

    /// Where the trace stands now.
    pub fn trace_pos(&self) -> TracePos {
        TracePos {
            choices: u32::try_from(self.trace.choices.len()).unwrap_or(u32::MAX),
            steps: self.trace.steps_after_last_choice,
        }
    }
}

/// The step the game is at.
pub(crate) enum Current {
    None,
    Say(SayStep),
    Choice(ChoiceStep),
}

pub(crate) struct SayStep {
    pub who: Option<Text>,
    pub text: Option<Text>,
    pub voice: Option<AssetId>,
    /// The step ID, which keys the read flag.
    pub step: StepId,
    pub read: bool,
}

pub(crate) struct ChoiceStep {
    /// The step ID; `step.func` is the function the choice ID names.
    pub step: StepId,
    pub id: String,
    pub arms: Vec<ChoiceArmStep>,
    pub disabled: Vec<(u32, Text)>,
    pub align: VAlign,
}

pub(crate) struct ChoiceArmStep {
    pub text: Text,
    pub shown: bool,
    pub value: Value,
    pub arm_id: String,
}

/// What a save records: how to reach the current step again. A debugger moves the story by
/// replaying a changed trace (see [`crate::debug`]).
#[derive(Clone, Debug)]
pub struct Trace {
    /// The path of the `#[entry]` scene.
    pub entry: String,
    pub args: Vec<Value>,
    pub seed: u64,
    /// The choice ID of every selection, in order.
    pub choices: Vec<String>,
    /// `Say` steps advanced since the last selection (or the start).
    pub steps_after_last_choice: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TracePos {
    pub choices: u32,
    pub steps: u32,
}

pub(crate) struct BacklogEntry {
    pub entry: Entry,
    pub at: TracePos,
}

pub(crate) enum Entry {
    Line {
        who: Option<Text>,
        text: Option<Text>,
        voice: Option<AssetId>,
        read: bool,
    },
    Chosen(Text),
}

/// The play mode; the variant index is the std order.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum PlayMode {
    Normal,
    Auto,
    Skip,
}

impl PlayMode {
    pub fn index(self) -> u32 {
        match self {
            PlayMode::Normal => 0,
            PlayMode::Auto => 1,
            PlayMode::Skip => 2,
        }
    }

    pub fn from_index(i: u32) -> Option<PlayMode> {
        Some(match i {
            0 => PlayMode::Normal,
            1 => PlayMode::Auto,
            2 => PlayMode::Skip,
            _ => return None,
        })
    }
}

/// Input for the story, applied by `settle` while the game is on top.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum StoryInput {
    Advance,
    Select(u32),
}

/// Story time: engine time minus the time the game spent paused.
#[derive(Clone, Copy, Debug)]
pub(crate) struct StoryClock {
    origin: u64,
    paused_at: Option<u64>,
    paused_total: u64,
}

impl StoryClock {
    pub fn new(now_ms: u64) -> StoryClock {
        StoryClock {
            origin: now_ms,
            paused_at: None,
            paused_total: 0,
        }
    }

    pub fn story_ms(&self, now_ms: u64) -> u64 {
        self.paused_at
            .unwrap_or(now_ms)
            .saturating_sub(self.origin)
            .saturating_sub(self.paused_total)
    }

    pub fn pause(&mut self, now_ms: u64) {
        if self.paused_at.is_none() {
            self.paused_at = Some(now_ms);
        }
    }

    pub fn resume(&mut self, now_ms: u64) {
        if let Some(at) = self.paused_at.take() {
            self.paused_total = self.paused_total.saturating_add(now_ms.saturating_sub(at));
        }
    }
}

/// The plain characters of a text: the base of rubies and effects, in order.
pub(crate) fn text_plain(text: &Text) -> String {
    fn walk(text: &Text, out: &mut String) {
        match text {
            Text::Plain(s) => out.push_str(s),
            Text::Ruby(p) => walk(&p.0, out),
            Text::MonoRuby(p) => walk(&p.0, out),
            Text::Effect(p) => walk(&p.0, out),
            Text::Concat(items) => items.iter().for_each(|t| walk(t, out)),
        }
    }
    let mut out = String::new();
    walk(text, &mut out);
    out
}
