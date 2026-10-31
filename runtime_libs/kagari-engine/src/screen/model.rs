//! The model of `std::screen::Screen`: the logical screen (entities, Hud, camera, filter,
//! channels) and the step-only state (timeline cursor, scheduled animations, one-shots, choice
//! menu, text start, despawning entities). A port of the compiler's
//! `eval/machine/screen.rs` with typed fields.

use std::any::Any;
use std::collections::BTreeMap;
use std::rc::Rc;

use kagari_vm::{
    AssetId, Decimal, HostObj, HostObject, Remap, RemapError, StaticId, Text, Trap, Value, value_eq,
};

use crate::values::{
    AudioFade, EffectSpec, Flip, LoopSpec, Motion, MovieSpec, PaintSpec, Plane, PoseSpec,
    SoundSpec, Transition, TransitionalMotionSpec, Tween, VAlign, Visibility, check_easing,
    check_fraction, check_motion, host_trap,
};

/// A screen. Equality compares the logical screen only.
#[derive(Clone, Debug)]
pub(crate) struct ScreenObj {
    pub logical: LogicalScreen,
    pub step: StepState,
}

/// The persistent state after a render.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LogicalScreen {
    pub entities: BTreeMap<u32, Entity>,
    pub hud: Hud,
    pub camera: Camera,
    pub filter: Option<Rc<EffectSpec>>,
    pub music: Channel,
    pub ambience: [Channel; 4],
}

#[derive(Clone, Debug)]
pub(crate) struct Entity {
    pub plane: Plane,
    pub z: i32,
    pub position: [i32; 2],
    pub anchor: [Decimal; 2],
    pub scale: Decimal,
    pub rotation: Decimal,
    pub opacity: Decimal,
    pub filter: Option<Rc<EffectSpec>>,
    pub mix: Option<Rc<EffectSpec>>,
    pub mask: Option<Rc<EffectSpec>>,
    pub local_motion: Option<Rc<EffectSpec>>,
    pub fill: [u8; 4],
    pub fill_size: Option<[u32; 2]>,
    pub parts: BTreeMap<u8, PartContent>,
    pub label: Option<Text>,
    pub label_style: Option<Value>,
    pub label_width: Option<u32>,
    pub movie: Option<MovieSpec>,
    pub flip: Flip,
    pub gate: Option<StaticId>,
}

/// What an image layer (a part or the side image) shows.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum PartContent {
    Image(AssetId),
    Paint(Rc<PaintSpec>),
}

impl From<PartContent> for LayerContent {
    fn from(content: PartContent) -> LayerContent {
        match content {
            PartContent::Image(image) => LayerContent::Image(image),
            PartContent::Paint(paint) => LayerContent::Paint(paint),
        }
    }
}

fn values_eq(a: Option<Value>, b: Option<Value>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => value_eq(&a, &b).unwrap_or(false),
        _ => false,
    }
}

impl PartialEq for Entity {
    fn eq(&self, other: &Entity) -> bool {
        self.plane == other.plane
            && self.z == other.z
            && self.position == other.position
            && self.anchor == other.anchor
            && self.scale == other.scale
            && self.rotation == other.rotation
            && self.opacity == other.opacity
            && self.filter == other.filter
            && self.mix == other.mix
            && self.mask == other.mask
            && self.local_motion == other.local_motion
            && self.fill == other.fill
            && self.fill_size == other.fill_size
            && self.parts == other.parts
            && self.label_width == other.label_width
            && self.movie == other.movie
            && self.flip == other.flip
            && self.gate == other.gate
            && values_eq(
                self.label.clone().map(Value::Text),
                other.label.clone().map(Value::Text),
            )
            && values_eq(self.label_style.clone(), other.label_style.clone())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Hud {
    pub text_window: Visibility,
    pub quick_menu: Visibility,
    pub side_image: Option<PartContent>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Camera {
    pub offset: [i32; 2],
    pub zoom: Decimal,
    pub rotation: Decimal,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Channel {
    pub track: Option<LoopSpec>,
    pub volume: Decimal,
}

/// The parts of a screen that belong to the step only.
#[derive(Clone, Debug)]
pub(crate) struct StepState {
    pub cursor: Cursor,
    /// Where the text reveal and the voice start.
    pub text_start: Cursor,
    pub scheduled: Vec<Scheduled>,
    /// The choice menu's display-only arms: `(at, text)`.
    pub disabled_arms: Vec<(u32, Text)>,
    pub menu_align: VAlign,
    /// Entities removed from the logical screen, drawn until their `Despawn` item and their
    /// animations end.
    pub despawning: BTreeMap<u32, Entity>,
}

/// A point of the step's timeline.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct Cursor {
    pub anchor: CursorAnchor,
    pub offset_ms: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum CursorAnchor {
    Entry,
    /// The time the text reveal reaches `Mark(n)`.
    Mark(u8),
    /// The end of the first `n` scheduled entries (`barrier()`).
    Barrier(u32),
}

impl Cursor {
    pub const ENTRY: Cursor = Cursor {
        anchor: CursorAnchor::Entry,
        offset_ms: 0,
    };
}

/// Something scheduled on the timeline.
#[derive(Clone, Debug)]
pub(crate) struct Scheduled {
    /// The cursor plus the animation's delay.
    pub start: Cursor,
    pub target: Option<Target>,
    pub kind: ScheduledKind,
}

#[derive(Clone, Debug)]
pub(crate) enum ScheduledKind {
    /// A property change, interpolated to `value`.
    Tween {
        tween: Tween,
        value: PropValue,
    },
    /// A layer swap to `content`.
    Transition {
        transition: Transition,
        content: Option<LayerContent>,
    },
    Motion(Motion),
    /// A channel change to `track`.
    AudioFade {
        fade: AudioFade,
        track: Option<LoopSpec>,
    },
    Sound {
        sound: SoundSpec,
        length_ms: u32,
    },
    StopSounds {
        fade_out_ms: u32,
    },
    /// `replay_bgm()`.
    ReplayMusic,
    /// `Entity::travel`: the entity's path from `from` to `to`, drawn by the motion's shader.
    Travel {
        motion: TransitionalMotionSpec,
        from: PoseSpec,
        to: PoseSpec,
    },
    /// `Screen::despawn` at the cursor, on `Target::Entity`: the despawning entity is drawn
    /// until this time and the end of its animations, whichever is later.
    Despawn,
}

/// What an animation applies to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Target {
    Prop(PropTarget),
    Layer(LayerTarget),
    Channel(ChannelId),
    Entity(u32),
    Camera,
    /// The travels of an entity.
    Travel(u32),
}

/// An interpolated property (`Prop<T>`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum PropTarget {
    Entity(u32, EntityProp),
    HudTextWindow,
    HudQuickMenu,
    CameraOffset,
    CameraZoom,
    CameraRotation,
    ScreenFilter,
    ChannelVolume(ChannelId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum EntityProp {
    Position,
    Scale,
    Rotation,
    Opacity,
    Filter,
    Mix,
    Mask,
    LocalMotion,
    Fill,
}

/// A layer swapped with a `Transition`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum LayerTarget {
    Part(u32, u8),
    Label(u32),
    Movie(u32),
    SideImage,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum ChannelId {
    Music,
    Ambience(u8),
}

/// A part of the screen an import returns a `&mut` to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum PartTarget {
    Entity(u32),
    Hud,
    ChoiceMenu,
    Camera,
    Prop(PropTarget),
    Layer(LayerTarget),
    Channel(ChannelId),
}

/// The value of a property.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum PropValue {
    Point([i32; 2]),
    Offset([i32; 2]),
    Decimal(Decimal),
    Angle(Decimal),
    Color([u8; 4]),
    /// A filter, mix, mask or local motion; `None` is no effect.
    Effect(Option<Rc<EffectSpec>>),
    Visibility(Visibility),
}

/// The content of a layer.
#[derive(Clone, Debug)]
pub(crate) enum LayerContent {
    Image(AssetId),
    Paint(Rc<PaintSpec>),
    Label(Text),
    Movie(MovieSpec),
}

impl LayerContent {
    /// The content of an image layer; `None` for a label or a movie.
    pub fn part(self) -> Option<PartContent> {
        match self {
            LayerContent::Image(image) => Some(PartContent::Image(image)),
            LayerContent::Paint(paint) => Some(PartContent::Paint(paint)),
            LayerContent::Label(_) | LayerContent::Movie(_) => None,
        }
    }
}

/// A change that happens only when [`ScreenObj::commit`] gives it an animation.
#[derive(Clone, Debug)]
pub(crate) enum PendingChange {
    Prop(PropTarget, PropValue),
    /// `None` clears the layer.
    Layer(LayerTarget, Option<LayerContent>),
    /// `None` stops the channel.
    Channel(ChannelId, Option<LoopSpec>),
}

/// The animation `apply_animation` commits a pending change with.
#[derive(Clone, Debug)]
pub(crate) enum Animation {
    Tween(Tween),
    Transition(Transition),
    Fade(AudioFade),
}

impl Entity {
    /// A newly spawned entity: invisible, on `Stage` at z 0.
    pub fn spawned() -> Entity {
        Entity {
            plane: Plane::Stage,
            z: 0,
            position: [0, 0],
            anchor: [Decimal::ZERO, Decimal::ZERO],
            scale: Decimal::ONE,
            rotation: Decimal::ZERO,
            opacity: Decimal::ZERO,
            filter: None,
            mix: None,
            mask: None,
            local_motion: None,
            fill: [0, 0, 0, 0],
            fill_size: None,
            parts: BTreeMap::new(),
            label: None,
            label_style: None,
            label_width: None,
            movie: None,
            flip: Flip::None,
            gate: None,
        }
    }

    pub fn prop(&self, prop: EntityProp) -> PropValue {
        match prop {
            EntityProp::Position => PropValue::Point(self.position),
            EntityProp::Scale => PropValue::Decimal(self.scale),
            EntityProp::Rotation => PropValue::Angle(self.rotation),
            EntityProp::Opacity => PropValue::Decimal(self.opacity),
            EntityProp::Filter => PropValue::Effect(self.filter.clone()),
            EntityProp::Mix => PropValue::Effect(self.mix.clone()),
            EntityProp::Mask => PropValue::Effect(self.mask.clone()),
            EntityProp::LocalMotion => PropValue::Effect(self.local_motion.clone()),
            EntityProp::Fill => PropValue::Color(self.fill),
        }
    }

    pub fn layer(&self, target: LayerTarget) -> Option<LayerContent> {
        match target {
            LayerTarget::Part(_, part) => self.parts.get(&part).cloned().map(LayerContent::from),
            LayerTarget::Label(_) => self.label.clone().map(LayerContent::Label),
            LayerTarget::Movie(_) => self.movie.clone().map(LayerContent::Movie),
            LayerTarget::SideImage => None,
        }
    }
}

impl Channel {
    fn silent() -> Channel {
        Channel {
            track: None,
            volume: Decimal::ONE,
        }
    }
}

fn mismatch() -> Trap {
    host_trap("a screen value does not match the part it is set on")
}

impl LogicalScreen {
    pub fn channel(&self, channel: ChannelId) -> &Channel {
        match channel {
            ChannelId::Music => &self.music,
            ChannelId::Ambience(index) => &self.ambience[usize::from(index).min(3)],
        }
    }

    /// The logical value of a property; an absent entity reads its spawn defaults.
    pub fn read_prop(&self, target: PropTarget) -> PropValue {
        match target {
            PropTarget::Entity(id, prop) => match self.entities.get(&id) {
                Some(entity) => entity.prop(prop),
                None => Entity::spawned().prop(prop),
            },
            PropTarget::HudTextWindow => PropValue::Visibility(self.hud.text_window),
            PropTarget::HudQuickMenu => PropValue::Visibility(self.hud.quick_menu),
            PropTarget::CameraOffset => PropValue::Offset(self.camera.offset),
            PropTarget::CameraZoom => PropValue::Decimal(self.camera.zoom),
            PropTarget::CameraRotation => PropValue::Angle(self.camera.rotation),
            PropTarget::ScreenFilter => PropValue::Effect(self.filter.clone()),
            PropTarget::ChannelVolume(channel) => PropValue::Decimal(self.channel(channel).volume),
        }
    }

    /// The content of a layer; an absent entity has none.
    pub fn read_layer(&self, target: LayerTarget) -> Option<LayerContent> {
        match target {
            LayerTarget::Part(id, _) | LayerTarget::Label(id) | LayerTarget::Movie(id) => {
                self.entities.get(&id)?.layer(target)
            }
            LayerTarget::SideImage => self.hud.side_image.clone().map(LayerContent::from),
        }
    }
}

impl ScreenObj {
    /// `Screen::empty()`: only the opaque `BACKGROUND` entity on the `Background` plane, the
    /// default Hud and camera, no filter and silent channels.
    pub fn empty() -> ScreenObj {
        let mut background = Entity::spawned();
        background.plane = Plane::Background;
        background.opacity = Decimal::ONE;
        let mut entities = BTreeMap::new();
        entities.insert(0, background);
        ScreenObj {
            logical: LogicalScreen {
                entities,
                hud: Hud {
                    text_window: Visibility::Shown,
                    quick_menu: Visibility::Shown,
                    side_image: None,
                },
                camera: Camera {
                    offset: [0, 0],
                    zoom: Decimal::ONE,
                    rotation: Decimal::ZERO,
                },
                filter: None,
                music: Channel::silent(),
                ambience: [
                    Channel::silent(),
                    Channel::silent(),
                    Channel::silent(),
                    Channel::silent(),
                ],
            },
            step: StepState::entry(),
        }
    }

    /// Step entry: the timeline and the text start go back to the entry, scheduled animations
    /// complete (despawning entities disappear), and the choice menu is emptied and centred.
    pub fn step_entry(&mut self) {
        self.step = StepState::entry();
    }

    /// The entity, spawned invisible if absent; a pending despawn is cancelled and the entity
    /// keeps its values.
    pub fn entity_mut(&mut self, id: u32) -> &mut Entity {
        let despawning = &mut self.step.despawning;
        self.logical
            .entities
            .entry(id)
            .or_insert_with(|| despawning.remove(&id).unwrap_or_else(Entity::spawned))
    }

    pub fn contains(&self, id: u32) -> bool {
        self.logical.entities.contains_key(&id)
    }

    /// The ids of the live entities on `plane`, ascending.
    pub fn entities_on(&self, plane: Plane) -> Vec<u32> {
        self.logical
            .entities
            .iter()
            .filter(|(_, entity)| entity.plane == plane)
            .map(|(id, _)| *id)
            .collect()
    }

    /// Removes entity `id` from the logical screen and schedules its `Despawn` at the cursor;
    /// nothing when it is absent.
    pub fn despawn(&mut self, id: u32) {
        if let Some(entity) = self.logical.entities.remove(&id) {
            self.step.despawning.insert(id, entity);
            self.step.scheduled.push(Scheduled {
                start: self.step.cursor,
                target: Some(Target::Entity(id)),
                kind: ScheduledKind::Despawn,
            });
        }
    }

    fn channel_mut(&mut self, channel: ChannelId) -> &mut Channel {
        match channel {
            ChannelId::Music => &mut self.logical.music,
            ChannelId::Ambience(index) => &mut self.logical.ambience[usize::from(index).min(3)],
        }
    }

    /// `Prop::get`: the logical value.
    pub fn prop(&mut self, target: PropTarget) -> PropValue {
        if let PropTarget::Entity(id, _) = target {
            self.entity_mut(id);
        }
        self.logical.read_prop(target)
    }

    fn set_prop(&mut self, target: PropTarget, value: PropValue) -> Result<(), Trap> {
        match (target, value) {
            (PropTarget::Entity(id, prop), value) => {
                let entity = self.entity_mut(id);
                match (prop, value) {
                    (EntityProp::Position, PropValue::Point(p)) => entity.position = p,
                    (EntityProp::Scale, PropValue::Decimal(d)) => entity.scale = d,
                    (EntityProp::Rotation, PropValue::Angle(d)) => entity.rotation = d,
                    (EntityProp::Opacity, PropValue::Decimal(d)) => entity.opacity = d,
                    (EntityProp::Filter, PropValue::Effect(e)) => entity.filter = e,
                    (EntityProp::Mix, PropValue::Effect(e)) => entity.mix = e,
                    (EntityProp::Mask, PropValue::Effect(e)) => entity.mask = e,
                    (EntityProp::LocalMotion, PropValue::Effect(e)) => entity.local_motion = e,
                    (EntityProp::Fill, PropValue::Color(c)) => entity.fill = c,
                    _ => return Err(mismatch()),
                }
            }
            (PropTarget::HudTextWindow, PropValue::Visibility(v)) => {
                self.logical.hud.text_window = v;
            }
            (PropTarget::HudQuickMenu, PropValue::Visibility(v)) => {
                self.logical.hud.quick_menu = v;
            }
            (PropTarget::CameraOffset, PropValue::Offset(o)) => self.logical.camera.offset = o,
            (PropTarget::CameraZoom, PropValue::Decimal(d)) => self.logical.camera.zoom = d,
            (PropTarget::CameraRotation, PropValue::Angle(d)) => self.logical.camera.rotation = d,
            (PropTarget::ScreenFilter, PropValue::Effect(e)) => self.logical.filter = e,
            (PropTarget::ChannelVolume(channel), PropValue::Decimal(d)) => {
                self.channel_mut(channel).volume = d;
            }
            _ => return Err(mismatch()),
        }
        Ok(())
    }

    /// The content of a layer.
    pub fn layer(&mut self, target: LayerTarget) -> Option<LayerContent> {
        match target {
            LayerTarget::Part(id, _) | LayerTarget::Label(id) | LayerTarget::Movie(id) => {
                self.entity_mut(id).layer(target)
            }
            LayerTarget::SideImage => self.logical.hud.side_image.clone().map(LayerContent::from),
        }
    }

    fn set_layer(
        &mut self,
        target: LayerTarget,
        content: Option<LayerContent>,
    ) -> Result<(), Trap> {
        match (target, content) {
            (LayerTarget::Part(id, part), Some(content)) => {
                let content = content.part().ok_or_else(mismatch)?;
                self.entity_mut(id).parts.insert(part, content);
            }
            (LayerTarget::Part(id, part), None) => {
                self.entity_mut(id).parts.remove(&part);
            }
            (LayerTarget::Label(id), Some(LayerContent::Label(text))) => {
                self.entity_mut(id).label = Some(text);
            }
            (LayerTarget::Label(id), None) => self.entity_mut(id).label = None,
            (LayerTarget::Movie(id), Some(LayerContent::Movie(movie))) => {
                self.entity_mut(id).movie = Some(movie);
            }
            (LayerTarget::Movie(id), None) => self.entity_mut(id).movie = None,
            (LayerTarget::SideImage, Some(content)) => {
                self.logical.hud.side_image = Some(content.part().ok_or_else(mismatch)?);
            }
            (LayerTarget::SideImage, None) => self.logical.hud.side_image = None,
            _ => return Err(mismatch()),
        }
        Ok(())
    }

    /// Checks the value a pending change will set: fractions for opacity and volume.
    pub fn check_change(change: &PendingChange) -> Result<(), Trap> {
        match change {
            PendingChange::Prop(
                PropTarget::Entity(_, EntityProp::Opacity),
                PropValue::Decimal(value),
            ) => check_fraction(*value, "opacity"),
            PendingChange::Prop(PropTarget::ChannelVolume(_), PropValue::Decimal(value)) => {
                check_fraction(*value, "channel volume")
            }
            _ => Ok(()),
        }
    }

    /// `apply_animation` on a pending change: the change takes effect on the logical screen at
    /// once and the animation is scheduled at the cursor plus its delay. A channel change to
    /// the track already playing is a no-op.
    pub fn commit(&mut self, change: PendingChange, animation: Animation) -> Result<(), Trap> {
        match (change, animation) {
            (PendingChange::Prop(target, value), Animation::Tween(tween)) => {
                check_easing(&tween.easing)?;
                self.set_prop(target, value.clone())?;
                self.schedule(
                    Some(Target::Prop(target)),
                    ScheduledKind::Tween { tween, value },
                    tween.delay_ms,
                )
            }
            (PendingChange::Layer(target, content), Animation::Transition(transition)) => {
                check_easing(&transition.easing)?;
                self.set_layer(target, content.clone())?;
                let delay = transition.delay_ms;
                self.schedule(
                    Some(Target::Layer(target)),
                    ScheduledKind::Transition {
                        transition,
                        content,
                    },
                    delay,
                )
            }
            (PendingChange::Channel(channel, track), Animation::Fade(fade)) => {
                let current = &mut self.channel_mut(channel).track;
                if *current == track {
                    return Ok(());
                }
                current.clone_from(&track);
                self.schedule(
                    Some(Target::Channel(channel)),
                    ScheduledKind::AudioFade { fade, track },
                    0,
                )
            }
            _ => Err(host_trap(
                "`apply_animation` was given an animation of the wrong kind",
            )),
        }
    }

    /// `Entity::travel(to, motion)`: the pose becomes `to` at once through three tweens with the
    /// motion's timing, and the travel itself is scheduled for the motion shader.
    pub fn travel(
        &mut self,
        id: u32,
        to: PoseSpec,
        motion: TransitionalMotionSpec,
    ) -> Result<(), Trap> {
        let entity = self.entity_mut(id);
        let from = PoseSpec {
            position: entity.position,
            scale: entity.scale,
            rotation: entity.rotation,
        };
        check_easing(&motion.easing)?;
        let tween = Tween {
            duration_ms: motion.duration_ms,
            easing: motion.easing,
            delay_ms: motion.delay_ms,
        };
        for (prop, value) in [
            (EntityProp::Position, PropValue::Point(to.position)),
            (EntityProp::Scale, PropValue::Decimal(to.scale)),
            (EntityProp::Rotation, PropValue::Angle(to.rotation)),
        ] {
            self.commit(
                PendingChange::Prop(PropTarget::Entity(id, prop), value),
                Animation::Tween(tween),
            )?;
        }
        let delay = motion.delay_ms;
        self.schedule(
            Some(Target::Travel(id)),
            ScheduledKind::Travel { motion, from, to },
            delay,
        )
    }

    /// `apply_animation(motion)` on an entity or the camera: checks the identity rule and
    /// schedules the motion; the logical screen does not change.
    pub fn apply_motion(&mut self, target: Target, motion: Motion) -> Result<(), Trap> {
        check_motion(&motion)?;
        if let Target::Entity(id) = target {
            self.entity_mut(id);
        }
        let delay = motion.delay_ms;
        self.schedule(Some(target), ScheduledKind::Motion(motion), delay)
    }

    /// Schedules at the cursor plus `delay_ms`.
    pub fn schedule(
        &mut self,
        target: Option<Target>,
        kind: ScheduledKind,
        delay_ms: u32,
    ) -> Result<(), Trap> {
        let start = self.cursor_plus(delay_ms)?;
        self.step.scheduled.push(Scheduled {
            start,
            target,
            kind,
        });
        Ok(())
    }

    fn cursor_plus(&self, ms: u32) -> Result<Cursor, Trap> {
        let offset_ms = self
            .step
            .cursor
            .offset_ms
            .checked_add(ms)
            .ok_or_else(duration_overflow)?;
        Ok(Cursor {
            anchor: self.step.cursor.anchor,
            offset_ms,
        })
    }

    /// `play(sound)`; `length_ms` is the part of the file it plays.
    pub fn play(&mut self, sound: SoundSpec, length_ms: u32) -> Result<(), Trap> {
        check_fraction(sound.volume, "sound volume")?;
        let delay = sound.delay_ms;
        self.schedule(None, ScheduledKind::Sound { sound, length_ms }, delay)
    }

    /// `stop_sounds(fade_out)`.
    pub fn stop_sounds(&mut self, fade_out_ms: u32) -> Result<(), Trap> {
        self.schedule(None, ScheduledKind::StopSounds { fade_out_ms }, 0)
    }

    /// `replay_bgm()`: restarts the music channel's track at the cursor; nothing when the
    /// channel is silent.
    pub fn replay_music(&mut self) -> Result<(), Trap> {
        if self.logical.music.track.is_none() {
            return Ok(());
        }
        self.schedule(
            Some(Target::Channel(ChannelId::Music)),
            ScheduledKind::ReplayMusic,
            0,
        )
    }

    /// `wait(duration)`.
    pub fn wait(&mut self, ms: u32) -> Result<(), Trap> {
        self.step.cursor = self.cursor_plus(ms)?;
        Ok(())
    }

    /// `barrier()`.
    pub fn barrier(&mut self) {
        let count = u32::try_from(self.step.scheduled.len()).unwrap_or(u32::MAX);
        self.step.cursor = Cursor {
            anchor: CursorAnchor::Barrier(count),
            offset_ms: 0,
        };
    }

    /// `at_mark(Mark(n))`.
    pub fn at_mark(&mut self, mark: u8) {
        self.step.cursor = Cursor {
            anchor: CursorAnchor::Mark(mark),
            offset_ms: 0,
        };
    }

    /// `at_entry()`.
    pub fn at_entry(&mut self) {
        self.step.cursor = Cursor::ENTRY;
    }

    /// `begin_text()`: an error while the cursor is anchored on a mark.
    pub fn begin_text(&mut self) -> Result<(), Trap> {
        if let CursorAnchor::Mark(mark) = self.step.cursor.anchor {
            return Err(host_trap(format!(
                "`begin_text` while the timeline is anchored on `Mark({mark})`"
            )));
        }
        self.step.text_start = self.step.cursor;
        Ok(())
    }
}

impl StepState {
    fn entry() -> StepState {
        StepState {
            cursor: Cursor::ENTRY,
            text_start: Cursor::ENTRY,
            scheduled: Vec::new(),
            disabled_arms: Vec::new(),
            menu_align: VAlign::Middle,
            despawning: BTreeMap::new(),
        }
    }
}

pub(crate) fn duration_overflow() -> Trap {
    host_trap("`Duration` overflow: the time does not fit in `u32` milliseconds")
}

impl HostObject for ScreenObj {
    fn clone_object(&self) -> Rc<dyn HostObject> {
        Rc::new(self.clone())
    }

    fn type_name(&self) -> &'static str {
        "Screen"
    }

    fn eq_object(&self, other: &dyn HostObject) -> Option<bool> {
        let other = other.as_any().downcast_ref::<ScreenObj>()?;
        Some(self.logical == other.logical)
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn remap(&self, remap: &mut Remap<'_>) -> Result<Option<HostObj>, RemapError> {
        self.remapped(remap)
    }
}
