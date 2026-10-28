//! A step's screen at run time: the screen its render returned, the logical screen at step
//! entry, the timeline of its scheduled items with their resolved start times, and the effect
//! clocks of its shader effects.

use std::collections::HashMap;

use kagari_vm::{AssetId, HostObj, Trap, Value};

use super::expect_screen;
use super::model::{
    CursorAnchor, EntityProp, LayerContent, LayerTarget, LogicalScreen, PartContent, PropTarget,
    PropValue, ScheduledKind, ScreenObj, StepState, Target,
};
use crate::Platform;
use crate::anim::decimal_f32;
use crate::assets::AssetStore;
use crate::audio::AudioModel;
use crate::values::{EffectSpec, LocalMotionBehavior, PaintSpec, TransitionKind};

/// Records every image on the logical screen as seen; a new one invalidates `SeenImage`.
pub(crate) fn mark_seen_images(
    logical: &LogicalScreen,
    system: &mut crate::system::SystemState,
    ui: &mut crate::ui::UiState,
) {
    for id in images_of(logical) {
        if system.mark_seen_image(id) {
            ui.invalidate(crate::ui::SourceKey::SeenImage(id));
        }
    }
}

/// What an effect clock belongs to: the screen filter, an entity's filter, mix, mask or local
/// motion, or the paint of an image layer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum EffectSlot {
    ScreenFilter,
    Entity(u32, EntityProp),
    Layer(LayerTarget),
}

impl EffectSlot {
    fn target(self) -> Target {
        match self {
            EffectSlot::ScreenFilter => Target::Prop(PropTarget::ScreenFilter),
            EffectSlot::Entity(id, prop) => Target::Prop(PropTarget::Entity(id, prop)),
            EffectSlot::Layer(layer) => Target::Layer(layer),
        }
    }
}

/// What keeps an effect clock running: changing only the arguments keeps it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct EffectKey {
    pub shader: AssetId,
    pub mask: Option<AssetId>,
    pub behavior: Option<LocalMotionBehavior>,
}

impl EffectKey {
    pub fn of_effect(e: &EffectSpec) -> EffectKey {
        EffectKey {
            shader: e.shader,
            mask: None,
            behavior: e.behavior,
        }
    }

    pub fn of_paint(p: &PaintSpec) -> EffectKey {
        EffectKey {
            shader: p.texture.shader,
            mask: p.mask.as_ref().map(|m| m.shader),
            behavior: None,
        }
    }

    fn of_prop(value: &PropValue) -> Option<EffectKey> {
        match value {
            PropValue::Effect(Some(e)) => Some(EffectKey::of_effect(e)),
            _ => None,
        }
    }

    fn of_layer(content: Option<&LayerContent>) -> Option<EffectKey> {
        match content {
            Some(LayerContent::Paint(p)) => Some(EffectKey::of_paint(p)),
            _ => None,
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
    /// The effect clocks of the previous step at its end: slot → (key, origin).
    carried: HashMap<EffectSlot, (EffectKey, u64)>,
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
        ScheduledKind::Travel { motion, .. } => {
            if animations {
                motion.duration_ms
            } else {
                0
            }
        }
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
            carried: HashMap::new(),
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
            carried: HashMap::new(),
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
            for shader in shaders_of(&obj.logical).chain(shaders_of(&base)) {
                assets.ensure_shader(shader, platform)?;
            }
            for item in &obj.step.scheduled {
                match &item.kind {
                    ScheduledKind::Transition {
                        transition,
                        content,
                    } => {
                        match content {
                            Some(LayerContent::Image(image)) => {
                                assets.ensure_image(*image, platform)?;
                            }
                            Some(LayerContent::Paint(paint)) => {
                                for shader in paint_shaders(paint) {
                                    assets.ensure_shader(shader, platform)?;
                                }
                            }
                            _ => {}
                        }
                        match &transition.kind {
                            TransitionKind::Rule { mask, .. } => {
                                assets.ensure_image(*mask, platform)?;
                            }
                            TransitionKind::Masked { mask, .. } => {
                                assets.ensure_shader(mask.shader, platform)?;
                            }
                            _ => {}
                        }
                    }
                    ScheduledKind::Tween {
                        value: PropValue::Effect(Some(effect)),
                        ..
                    } => assets.ensure_shader(effect.shader, platform)?,
                    ScheduledKind::Travel { motion, .. } => {
                        assets.ensure_shader(motion.effect.shader, platform)?;
                    }
                    _ => {}
                }
            }
        }
        let carried = self.final_origins();
        *self = ScreenRuntime::from_step(base, screen, entry_ms, animations);
        self.carried = carried;
        Ok(())
    }

    /// The key of item `index`'s effect value, if it sets one.
    fn item_key(&self, index: usize) -> Option<EffectKey> {
        match &self.screen.step.scheduled.get(index)?.kind {
            ScheduledKind::Tween { value, .. } => EffectKey::of_prop(value),
            ScheduledKind::Transition { content, .. } => EffectKey::of_layer(content.as_ref()),
            _ => None,
        }
    }

    /// The key of a slot's effect on the base screen.
    fn base_key(&self, slot: EffectSlot) -> Option<EffectKey> {
        match slot {
            EffectSlot::ScreenFilter => {
                EffectKey::of_prop(&self.base.read_prop(PropTarget::ScreenFilter))
            }
            EffectSlot::Entity(id, prop) => {
                EffectKey::of_prop(&self.base.read_prop(PropTarget::Entity(id, prop)))
            }
            EffectSlot::Layer(layer) => EffectKey::of_layer(self.base.read_layer(layer).as_ref()),
        }
    }

    /// The origin (absolute story ms) of the clock of the effect `key` in `slot` at `t`: the
    /// earliest start of the trailing run of started items carrying `key`; when that run is
    /// every started item and the base carries `key` too, the origin carried over from the
    /// previous step; else the step entry.
    pub(super) fn effect_origin(&self, slot: EffectSlot, key: EffectKey, t: u64) -> u64 {
        let mut earliest: Option<u64> = None;
        let mut whole = true;
        for &index in self
            .by_target
            .get(&slot.target())
            .into_iter()
            .flatten()
            .rev()
        {
            let Some(start) = self.items.get(index).and_then(|i| i.start) else {
                continue;
            };
            if start > t {
                continue;
            }
            if self.item_key(index) != Some(key) {
                whole = false;
                break;
            }
            earliest = Some(earliest.map_or(start, |e| e.min(start)));
        }
        if whole && self.base_key(slot) == Some(key) {
            return match self.carried.get(&slot) {
                Some((carried, origin)) if *carried == key => *origin,
                _ => self.entry_ms,
            };
        }
        earliest.unwrap_or(self.entry_ms)
    }

    /// The final origin of every effect on the logical screen, carried into the next step.
    fn final_origins(&self) -> HashMap<EffectSlot, (EffectKey, u64)> {
        let logical = self.logical();
        let mut slots: Vec<(EffectSlot, EffectKey)> = Vec::new();
        if let Some(filter) = &logical.filter {
            slots.push((EffectSlot::ScreenFilter, EffectKey::of_effect(filter)));
        }
        for (id, entity) in &logical.entities {
            for (prop, effect) in [
                (EntityProp::Filter, &entity.filter),
                (EntityProp::Mix, &entity.mix),
                (EntityProp::Mask, &entity.mask),
                (EntityProp::LocalMotion, &entity.local_motion),
            ] {
                if let Some(effect) = effect {
                    slots.push((EffectSlot::Entity(*id, prop), EffectKey::of_effect(effect)));
                }
            }
            for (part, content) in &entity.parts {
                if let PartContent::Paint(paint) = content {
                    let slot = EffectSlot::Layer(LayerTarget::Part(*id, *part));
                    slots.push((slot, EffectKey::of_paint(paint)));
                }
            }
        }
        if let Some(PartContent::Paint(paint)) = &logical.hud.side_image {
            let slot = EffectSlot::Layer(LayerTarget::SideImage);
            slots.push((slot, EffectKey::of_paint(paint)));
        }
        slots
            .into_iter()
            .map(|(slot, key)| (slot, (key, self.effect_origin(slot, key, u64::MAX))))
            .collect()
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

/// The image and paint contents of a logical screen's image layers.
fn contents_of(logical: &LogicalScreen) -> impl Iterator<Item = &PartContent> + '_ {
    logical
        .entities
        .values()
        .flat_map(|entity| entity.parts.values())
        .chain(&logical.hud.side_image)
}

/// Every image a logical screen shows.
fn images_of(logical: &LogicalScreen) -> impl Iterator<Item = AssetId> + '_ {
    contents_of(logical).filter_map(|content| match content {
        PartContent::Image(image) => Some(*image),
        PartContent::Paint(_) => None,
    })
}

/// The texture and mask shader of a paint.
fn paint_shaders(paint: &PaintSpec) -> impl Iterator<Item = AssetId> + '_ {
    std::iter::once(paint.texture.shader).chain(paint.mask.as_ref().map(|m| m.shader))
}

/// Every shader a logical screen uses: filters, mixes, masks, local motions and paints.
fn shaders_of(logical: &LogicalScreen) -> impl Iterator<Item = AssetId> + '_ {
    let effects = logical
        .filter
        .iter()
        .chain(logical.entities.values().flat_map(|e| {
            e.filter
                .iter()
                .chain(&e.mix)
                .chain(&e.mask)
                .chain(&e.local_motion)
        }));
    let paints = contents_of(logical).flat_map(|content| match content {
        PartContent::Paint(paint) => Some(paint_shaders(paint)),
        PartContent::Image(_) => None,
    });
    effects.map(|effect| effect.shader).chain(paints.flatten())
}
