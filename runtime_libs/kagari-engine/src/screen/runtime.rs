//! A step's screen at run time: the screen its render returned, the logical screen at step
//! entry, and the timeline of its scheduled items with their resolved start times.

use std::collections::HashMap;

use kagari_vm::{HostObj, Trap, Value};

use super::expect_screen;
use super::model::{
    CursorAnchor, LayerContent, LogicalScreen, PropTarget, PropValue, ScheduledKind, ScreenObj,
    StepState, Target,
};
use crate::Platform;
use crate::anim::decimal_f32;
use crate::assets::AssetStore;
use crate::audio::AudioModel;
use crate::values::TransitionKind;

/// Records every image on the logical screen as seen; a new one invalidates `SeenImage`.
pub(crate) fn mark_seen_images(
    logical: &LogicalScreen,
    system: &mut crate::system::SystemState,
    ui: &mut crate::ui::UiState,
) {
    let parts = logical
        .entities
        .values()
        .flat_map(|e| e.parts.values().copied());
    for id in parts.chain(logical.hud.side_image) {
        if system.mark_seen_image(id) {
            ui.invalidate(crate::ui::SourceKey::SeenImage(id));
        }
    }
}

/// When the text reveal reaches each mark.
pub(crate) trait MarkTimes {
    /// Story ms (absolute, like `entry_ms`) at which the reveal reaches `Mark(mark)`; a mark the
    /// text lacks → the reveal end; `None` while the reveal waits at an unreleased click pause
    /// before it.
    fn mark_ms(&self, mark: u8) -> Option<u64>;
}

/// Every mark at one time.
#[cfg(test)]
pub(crate) struct NoReveal(pub u64);

#[cfg(test)]
impl MarkTimes for NoReveal {
    fn mark_ms(&self, _mark: u8) -> Option<u64> {
        Some(self.0)
    }
}

/// No mark resolves yet.
struct Unresolved;

impl MarkTimes for Unresolved {
    fn mark_ms(&self, _mark: u8) -> Option<u64> {
        None
    }
}

pub(crate) struct ScreenRuntime {
    /// The screen the step's render returned.
    pub(super) screen: ScreenObj,
    /// The logical screen at step entry.
    pub(super) base: LogicalScreen,
    pub(super) entry_ms: u64,
    pub(super) animations: bool,
    text_start_ms: u32,
    /// One per scheduled item, in schedule order.
    pub(super) items: Vec<ItemState>,
    /// Item indices per target, in commit order.
    pub(super) by_target: HashMap<Target, Vec<usize>>,
}

pub(super) struct ItemState {
    /// Absolute story ms; `None` until resolved.
    pub start: Option<u64>,
    pub length_ms: u32,
    pub fired: bool,
}

fn screen_obj(screen: &HostObj) -> ScreenObj {
    screen
        .downcast::<ScreenObj>()
        .cloned()
        .unwrap_or_else(ScreenObj::empty)
}

/// The length of a scheduled item; animations count as 0 when they are off, audio never does.
fn item_length(kind: &ScheduledKind, animations: bool) -> u32 {
    match kind {
        ScheduledKind::Tween { tween, .. } => {
            if animations {
                tween.duration_ms
            } else {
                0
            }
        }
        ScheduledKind::Transition { transition, .. } => {
            if !animations {
                return 0;
            }
            match transition.kind {
                TransitionKind::ThroughColor { hold_ms, .. } => {
                    transition.duration_ms.saturating_add(hold_ms)
                }
                _ => transition.duration_ms,
            }
        }
        ScheduledKind::Motion(motion) => {
            if !animations {
                return 0;
            }
            motion
                .keys
                .last()
                .map_or(0, |last| last.at_ms.saturating_mul(motion.repeat))
        }
        ScheduledKind::AudioFade { fade, .. } => {
            if fade.overlap {
                fade.fade_out_ms.max(fade.fade_in_ms)
            } else {
                fade.fade_out_ms.saturating_add(fade.fade_in_ms)
            }
        }
        ScheduledKind::Sound { length_ms, .. } => *length_ms,
        ScheduledKind::StopSounds { fade_out_ms } => *fade_out_ms,
        ScheduledKind::ReplayMusic => 0,
    }
}

impl ScreenRuntime {
    /// A runtime before any step: the base is the screen's logical screen, with no items.
    pub fn new(screen: HostObj) -> ScreenRuntime {
        let screen = screen_obj(&screen);
        ScreenRuntime {
            base: screen.logical.clone(),
            screen,
            entry_ms: 0,
            animations: true,
            text_start_ms: 0,
            items: Vec::new(),
            by_target: HashMap::new(),
        }
    }

    /// The runtime of a step entered at `entry_ms` whose render returned `screen`, over the
    /// logical screen `base` of the step before.
    pub fn from_step(
        base: LogicalScreen,
        screen: HostObj,
        entry_ms: u64,
        animations: bool,
    ) -> ScreenRuntime {
        let screen = screen_obj(&screen);
        let scheduled = &screen.step.scheduled;
        let items: Vec<ItemState> = scheduled
            .iter()
            .map(|item| ItemState {
                start: None,
                length_ms: item_length(&item.kind, animations),
                fired: false,
            })
            .collect();
        let mut by_target: HashMap<Target, Vec<usize>> = HashMap::new();
        for (index, item) in scheduled.iter().enumerate() {
            if let Some(target) = item.target {
                by_target.entry(target).or_default().push(index);
            }
        }
        let mut runtime = ScreenRuntime {
            base,
            entry_ms,
            animations,
            text_start_ms: 0,
            items,
            by_target,
            screen,
        };
        runtime.text_start_ms = runtime.compute_text_start();
        runtime.store_starts(&Unresolved);
        runtime
    }

    /// Step entry: `out` is what the step's render returned. The previous step completes, the
    /// images the step shows are loaded, and its timeline starts at `entry_ms`.
    pub fn enter_step(
        &mut self,
        out: Value,
        entry_ms: u64,
        animations: bool,
        assets: &mut AssetStore,
        platform: &mut dyn Platform,
    ) -> Result<(), Trap> {
        let screen = expect_screen(&out)?;
        let base = self.logical().clone();
        if let Some(obj) = screen.downcast::<ScreenObj>() {
            for image in images_of(&obj.logical).chain(images_of(&base)) {
                assets.ensure_image(image, platform)?;
            }
            for item in &obj.step.scheduled {
                if let ScheduledKind::Transition {
                    transition,
                    content,
                } = &item.kind
                {
                    if let Some(LayerContent::Image(image)) = content {
                        assets.ensure_image(*image, platform)?;
                    }
                    if let TransitionKind::Rule { mask, .. } = transition.kind {
                        assets.ensure_image(mask, platform)?;
                    }
                }
            }
        }
        *self = ScreenRuntime::from_step(base, screen, entry_ms, animations);
        Ok(())
    }

    /// The screen the next step's render receives.
    pub fn render_input(&self) -> Value {
        let mut obj = self.screen.clone();
        obj.step_entry();
        Value::Host(HostObj::new(obj))
    }

    pub fn logical(&self) -> &LogicalScreen {
        &self.screen.logical
    }

    pub fn step(&self) -> &StepState {
        &self.screen.step
    }

    /// When the text reveal starts, in ms after step entry.
    pub fn text_start_ms(&self) -> u32 {
        self.text_start_ms
    }

    /// Whether a resolved item is still running at `now_ms`; items waiting on a mark do not
    /// count.
    pub fn timeline_running(&self, now_ms: u64, marks: &dyn MarkTimes) -> bool {
        self.resolve_starts(marks)
            .iter()
            .zip(&self.items)
            .any(|(start, item)| {
                start.is_some_and(|s| s.saturating_add(u64::from(item.length_ms)) > now_ms)
            })
    }

    /// Fires the audio of every item that has started by `now_ms`.
    pub fn tick(&mut self, now_ms: u64, marks: &dyn MarkTimes, audio: &mut AudioModel) {
        self.store_starts(marks);
        for (index, item) in self.screen.step.scheduled.iter().enumerate() {
            let Some(state) = self.items.get_mut(index) else {
                continue;
            };
            if state.fired || state.start.is_none_or(|start| start > now_ms) {
                continue;
            }
            state.fired = true;
            match (&item.kind, item.target) {
                (
                    ScheduledKind::Tween {
                        value: PropValue::Decimal(gain),
                        ..
                    },
                    Some(Target::Prop(PropTarget::ChannelVolume(channel))),
                ) => audio.set_channel_gain(channel, decimal_f32(*gain), state.length_ms),
                (ScheduledKind::AudioFade { fade, track }, Some(Target::Channel(channel))) => {
                    let gain = decimal_f32(self.screen.logical.channel(channel).volume);
                    audio.change_channel(channel, track.as_ref(), *fade, gain);
                }
                (ScheduledKind::Sound { sound, length_ms }, _) => {
                    audio.play_one_shot(sound, Some(*length_ms).filter(|l| *l > 0), now_ms);
                }
                (ScheduledKind::StopSounds { fade_out_ms }, _) => {
                    audio.stop_sounds(*fade_out_ms, now_ms);
                }
                (ScheduledKind::ReplayMusic, _) => audio.replay_music(),
                _ => {}
            }
        }
    }

    /// A click or skip: one-shots not started yet are dropped, every resolved item ends now,
    /// and the remaining audio fires. Items waiting on a mark are kept.
    pub fn complete_all(&mut self, now_ms: u64, marks: &dyn MarkTimes, audio: &mut AudioModel) {
        self.store_starts(marks);
        for (state, item) in self.items.iter_mut().zip(&self.screen.step.scheduled) {
            let Some(start) = state.start else {
                continue;
            };
            if !state.fired && start > now_ms && matches!(item.kind, ScheduledKind::Sound { .. }) {
                state.fired = true;
            }
            state.start = Some(start.min(now_ms));
            state.length_ms = 0;
        }
        self.tick(now_ms, marks, audio);
    }

    /// The absolute start of every item: the stored ones, plus those resolvable now.
    pub(super) fn resolve_starts(&self, marks: &dyn MarkTimes) -> Vec<Option<u64>> {
        let mut starts: Vec<Option<u64>> = self.items.iter().map(|item| item.start).collect();
        for (index, item) in self.screen.step.scheduled.iter().enumerate() {
            if starts.get(index).copied().flatten().is_some() {
                continue;
            }
            let anchor = match item.start.anchor {
                CursorAnchor::Entry => Some(self.entry_ms),
                CursorAnchor::Mark(mark) => marks.mark_ms(mark).map(|t| t.max(self.entry_ms)),
                CursorAnchor::Barrier(n) => self.barrier_end(&starts, n),
            };
            let offset = if self.animations {
                u64::from(item.start.offset_ms)
            } else {
                0
            };
            if let Some(slot) = starts.get_mut(index) {
                *slot = anchor.map(|a| a.saturating_add(offset));
            }
        }
        starts
    }

    /// The end of the first `n` items; `None` while one of them is unresolved.
    fn barrier_end(&self, starts: &[Option<u64>], n: u32) -> Option<u64> {
        let n = usize::try_from(n).unwrap_or(usize::MAX);
        let mut end = self.entry_ms;
        for (start, item) in starts.iter().zip(&self.items).take(n) {
            end = end.max(start.as_ref()?.saturating_add(u64::from(item.length_ms)));
        }
        Some(end)
    }

    fn store_starts(&mut self, marks: &dyn MarkTimes) {
        let starts = self.resolve_starts(marks);
        for (item, start) in self.items.iter_mut().zip(starts) {
            if item.start.is_none() {
                item.start = start;
            }
        }
    }

    /// The text start: barriers skip items whose start depends on a mark, directly or through
    /// an earlier barrier, since a mark is never before the text start.
    fn compute_text_start(&self) -> u32 {
        let mut free: Vec<Option<u64>> = Vec::with_capacity(self.items.len());
        // The end of the first `n` mark-free items; the mark-dependent ones are skipped.
        let barrier = |free: &[Option<u64>], n: u32| -> u64 {
            let n = usize::try_from(n).unwrap_or(usize::MAX);
            free.iter()
                .zip(&self.items)
                .take(n)
                .filter_map(|(start, item)| {
                    start.map(|s| s.saturating_add(u64::from(item.length_ms)))
                })
                .max()
                .unwrap_or(0)
        };
        let offset = |ms: u32| if self.animations { u64::from(ms) } else { 0 };
        for item in &self.screen.step.scheduled {
            let anchor = match item.start.anchor {
                CursorAnchor::Entry => Some(0),
                CursorAnchor::Mark(_) => None,
                // An item behind a barrier waits for every item before it: it depends on a
                // mark as soon as one of those does.
                CursorAnchor::Barrier(n) => {
                    let before = usize::try_from(n).unwrap_or(usize::MAX);
                    let on_mark = free.iter().take(before).any(Option::is_none);
                    (!on_mark).then(|| barrier(&free, n))
                }
            };
            free.push(anchor.map(|a| a.saturating_add(offset(item.start.offset_ms))));
        }
        let text_start = self.screen.step.text_start;
        let anchor = match text_start.anchor {
            CursorAnchor::Barrier(n) => barrier(&free, n),
            CursorAnchor::Entry | CursorAnchor::Mark(_) => 0,
        };
        u32::try_from(anchor.saturating_add(offset(text_start.offset_ms))).unwrap_or(u32::MAX)
    }
}

/// Every image a logical screen shows.
fn images_of(logical: &LogicalScreen) -> impl Iterator<Item = kagari_vm::AssetId> + '_ {
    logical
        .entities
        .values()
        .flat_map(|entity| entity.parts.values().copied())
        .chain(logical.hud.side_image)
}
