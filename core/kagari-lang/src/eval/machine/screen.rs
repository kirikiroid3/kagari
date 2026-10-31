//! The model of `std::screen::Screen` that renders transform at compile time: the logical
//! screen (entities with components, Hud, camera, screen filter, audio channels) and the
//! step-only state (timeline cursor, scheduled animations, one-shot sounds, choice menu,
//! text start, despawning entities).
//!
//! Component values are std values ([`ConstValue`]s of `Point`, `Decimal`, `Filter`, …).
//! Visible changes go through [`PendingChange`]s that only [`Screen::commit`] applies,
//! which is what `apply_animation` does.

use imbl::OrdMap;
use std::hash::{Hash, Hasher};

use crate::diagnostic::DiagnosticCode;

use kagari_num::Decimal;

use super::super::Fault;
use super::super::std_items::StdTable;
use super::super::value::ConstValue;
use super::ErrorKind;
use super::anim;

/// A screen. Equality and hashing compare the logical screen only: the step-only parts are
/// ignored, as the language's `==` on `Screen`, snapshots and the consistency check do.
#[derive(Clone, Debug)]
pub struct Screen {
    pub logical: LogicalScreen,
    pub step: StepState,
}

impl PartialEq for Screen {
    fn eq(&self, other: &Screen) -> bool {
        self.logical == other.logical
    }
}

impl Eq for Screen {}

impl Hash for Screen {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.logical.hash(state);
    }
}

/// The persistent state after a render.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct LogicalScreen {
    /// Live entities by `EntityId`.
    pub entities: OrdMap<u32, Entity>,
    pub hud: Hud,
    pub camera: Camera,
    /// The screen filter, an `Option<Filter>`.
    pub filter: ConstValue,
    pub music: Channel,
    pub ambience: [Channel; 4],
}

/// An entity's components.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Entity {
    /// A `Plane`.
    pub plane: ConstValue,
    pub z: i128,
    /// A `Point`.
    pub position: ConstValue,
    /// An `Anchor`.
    pub anchor: ConstValue,
    /// A `Decimal`.
    pub scale: ConstValue,
    /// An `Angle`.
    pub rotation: ConstValue,
    /// A `Decimal` in `0.0..=1.0`.
    pub opacity: ConstValue,
    /// The colour filter, an `Option<Filter>`.
    pub filter: ConstValue,
    /// How the entity composites, an `Option<Mix>`.
    pub mix: ConstValue,
    /// The alpha mask over the entity's box, an `Option<Mask>`.
    pub mask: ConstValue,
    /// The local motion, an `Option<LocalMotion>`.
    pub local_motion: ConstValue,
    /// A `Color`.
    pub fill: ConstValue,
    /// An `Option<Size>`.
    pub fill_size: ConstValue,
    /// Image layers by `Part`: `ImageAsset`s or `Paint`s.
    pub parts: OrdMap<u8, ConstValue>,
    /// Image layer resizes by `Part`; a part without one is `Natural`.
    pub resize: OrdMap<u8, LayerResize>,
    /// The label's `Text`.
    pub label: Option<ConstValue>,
    /// The label's `TextStyle`; `None` is the system's default style.
    pub label_style: Option<ConstValue>,
    /// The label's wrapping width, an `Option<u32>`.
    pub label_width: ConstValue,
    /// A `Movie`.
    pub movie: Option<ConstValue>,
    /// A `Flip`.
    pub flip: ConstValue,
    /// An `Option<Setting<bool>>`.
    pub gate: ConstValue,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Hud {
    /// `Visibility` values.
    pub text_window: ConstValue,
    pub quick_menu: ConstValue,
    /// An `ImageAsset` or a `Paint`.
    pub side_image: Option<ConstValue>,
    /// The side image's `Resize`; `None` is `Natural`.
    pub side_image_resize: Option<ConstValue>,
}

/// How an image layer sizes its content, when it is not `Natural`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum LayerResize {
    /// `Resize::Cover` of the design resolution, which only the engine knows: what
    /// `Screen::empty()` gives the `BACKGROUND` entity's `MAIN` layer.
    CoverScreen,
    /// A `Resize` other than `Natural`.
    Set(ConstValue),
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Camera {
    /// An `Offset`.
    pub offset: ConstValue,
    /// A `Decimal`.
    pub zoom: ConstValue,
    /// An `Angle`.
    pub rotation: ConstValue,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Channel {
    /// The `Loop` playing.
    pub track: Option<ConstValue>,
    /// A `Decimal` in `0.0..=1.0`.
    pub volume: ConstValue,
}

/// The parts of a screen that belong to the step only.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct StepState {
    pub cursor: Cursor,
    /// Where the text reveal and the voice start.
    pub text_start: Cursor,
    pub scheduled: Vec<Scheduled>,
    /// The choice menu's display-only arms: `(at, Text)`.
    pub disabled_arms: Vec<(i128, ConstValue)>,
    /// The choice menu's `VAlign`.
    pub menu_align: ConstValue,
    /// Entities removed from the logical screen and still drawn until their `Despawn` item and
    /// their animations end.
    pub despawning: OrdMap<u32, Entity>,
}

/// A point of the step's timeline.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Cursor {
    pub anchor: CursorAnchor,
    pub offset_ms: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CursorAnchor {
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
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Scheduled {
    /// The cursor plus the animation's delay.
    pub start: Cursor,
    pub target: Option<Target>,
    pub kind: ScheduledKind,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ScheduledKind {
    /// A `Tween` of a property change.
    Tween(ConstValue),
    /// A `Transition` of a layer swap.
    Transition(ConstValue),
    /// A `Motion` of an entity or the camera.
    Motion(ConstValue),
    /// An `AudioFade` of a channel change.
    AudioFade(ConstValue),
    /// A one-shot `Sound`.
    Sound(ConstValue),
    /// `stop_sounds(fade_out)`.
    StopSounds { fade_out_ms: u32 },
    /// `replay_bgm()`: the music channel's track restarts.
    ReplayMusic,
    /// `Entity::travel`: a `TransitionalMotion` from the `Pose` `from` to the `Pose` `to`.
    Travel {
        motion: ConstValue,
        from: ConstValue,
        to: ConstValue,
    },
    /// `despawn(id)` at the cursor, on `Target::Entity`: the despawning entity is drawn until
    /// this time and the end of its animations, whichever is later.
    Despawn,
}

/// What an animation applies to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Target {
    Prop(PropTarget),
    Layer(LayerTarget),
    Channel(ChannelId),
    Entity(u32),
    /// The path of an entity's `travel`.
    Travel(u32),
    Camera,
}

/// An interpolated property (`Prop<T>`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PropTarget {
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
pub enum EntityProp {
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
pub enum LayerTarget {
    Part(u32, u8),
    Label(u32),
    Movie(u32),
    SideImage,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ChannelId {
    Music,
    Ambience(u8),
}

/// A part of the screen a builtin returns a `&mut` to: `&mut Entity`, `&mut Prop<T>`,
/// `&mut ImageLayer`, …
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ScreenObj {
    Entity(u32),
    Hud,
    ChoiceMenu,
    Camera,
    Prop(PropTarget),
    Layer(LayerTarget),
    Channel(ChannelId),
}

/// A change that happens only when [`Screen::commit`] gives it an animation: `Change<T>`,
/// `Swap` and `ChannelChange`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum PendingChange {
    Prop(PropTarget, ConstValue),
    /// `None` clears the layer.
    Layer(LayerTarget, Option<ConstValue>),
    /// `None` stops the channel.
    Channel(ChannelId, Option<ConstValue>),
}

fn decimal(value: Decimal) -> ConstValue {
    ConstValue::Decimal(value)
}

fn zero_angle(std: &StdTable) -> ConstValue {
    std.angle.make(vec![decimal(Decimal::ZERO)])
}

impl Entity {
    /// The components of a newly spawned entity: invisible, on `Stage` at z 0.
    pub fn spawned(std: &StdTable) -> Entity {
        Entity {
            plane: std.plane.unit("Stage"),
            z: 0,
            position: std.point.make(vec![ConstValue::Int(0), ConstValue::Int(0)]),
            anchor: std
                .anchor
                .make(vec![decimal(Decimal::ZERO), decimal(Decimal::ZERO)]),
            scale: decimal(Decimal::ONE),
            rotation: zero_angle(std),
            opacity: decimal(Decimal::ZERO),
            filter: std.none(),
            mix: std.none(),
            mask: std.none(),
            local_motion: std.none(),
            fill: std.color.make(vec![
                ConstValue::Int(0),
                ConstValue::Int(0),
                ConstValue::Int(0),
                ConstValue::Int(0),
            ]),
            fill_size: std.none(),
            parts: OrdMap::new(),
            resize: OrdMap::new(),
            label: None,
            label_style: None,
            label_width: std.none(),
            movie: None,
            flip: std.flip.unit("None"),
            gate: std.none(),
        }
    }

    fn prop(&mut self, prop: EntityProp) -> &mut ConstValue {
        match prop {
            EntityProp::Position => &mut self.position,
            EntityProp::Scale => &mut self.scale,
            EntityProp::Rotation => &mut self.rotation,
            EntityProp::Opacity => &mut self.opacity,
            EntityProp::Filter => &mut self.filter,
            EntityProp::Mix => &mut self.mix,
            EntityProp::Mask => &mut self.mask,
            EntityProp::LocalMotion => &mut self.local_motion,
            EntityProp::Fill => &mut self.fill,
        }
    }
}

impl Channel {
    fn silent() -> Channel {
        Channel {
            track: None,
            volume: decimal(Decimal::ONE),
        }
    }
}

impl Screen {
    /// `Screen::empty()`: only the opaque `BACKGROUND` entity on the `Background` plane, its
    /// `MAIN` layer covering the design resolution, the default Hud and camera, no filter and
    /// silent channels.
    pub fn empty(std: &StdTable) -> Screen {
        let mut background = Entity::spawned(std);
        background.plane = std.plane.unit("Background");
        background.opacity = decimal(Decimal::ONE);
        background.resize.insert(0, LayerResize::CoverScreen);
        let mut entities = OrdMap::new();
        entities.insert(0, background);
        Screen {
            logical: LogicalScreen {
                entities,
                hud: Hud {
                    text_window: std.visibility.unit("Shown"),
                    quick_menu: std.visibility.unit("Shown"),
                    side_image: None,
                    side_image_resize: None,
                },
                camera: Camera {
                    offset: std
                        .offset
                        .make(vec![ConstValue::Int(0), ConstValue::Int(0)]),
                    zoom: decimal(Decimal::ONE),
                    rotation: zero_angle(std),
                },
                filter: std.none(),
                music: Channel::silent(),
                ambience: [
                    Channel::silent(),
                    Channel::silent(),
                    Channel::silent(),
                    Channel::silent(),
                ],
            },
            step: StepState {
                cursor: Cursor::ENTRY,
                text_start: Cursor::ENTRY,
                scheduled: Vec::new(),
                disabled_arms: Vec::new(),
                menu_align: std.valign.unit("Middle"),
                despawning: OrdMap::new(),
            },
        }
    }

    /// What the engine does at step entry: the timeline and the text start go back to the
    /// entry, scheduled animations complete (despawning entities disappear), and the choice
    /// menu is emptied and set to `Middle`.
    pub fn step_entry(&mut self, std: &StdTable) {
        self.step = StepState {
            cursor: Cursor::ENTRY,
            text_start: Cursor::ENTRY,
            scheduled: Vec::new(),
            disabled_arms: Vec::new(),
            menu_align: std.valign.unit("Middle"),
            despawning: OrdMap::new(),
        };
    }

    /// `entity(id)`: the entity, spawned invisible if absent; a pending despawn is cancelled
    /// and the entity keeps its values.
    pub fn entity(&mut self, id: u32, std: &StdTable) -> &mut Entity {
        if !self.logical.entities.contains_key(&id) {
            let entity = self
                .step
                .despawning
                .remove(&id)
                .unwrap_or_else(|| Entity::spawned(std));
            self.logical.entities.insert(id, entity);
        }
        self.logical
            .entities
            .entry(id)
            .or_insert_with(|| Entity::spawned(std))
    }

    pub fn contains(&self, id: u32) -> bool {
        self.logical.entities.contains_key(&id)
    }

    /// The ids of the live entities on `plane`, ascending.
    pub fn entities_on(&self, plane: &ConstValue) -> Vec<u32> {
        self.logical
            .entities
            .iter()
            .filter(|(_, entity)| entity.plane == *plane)
            .map(|(id, _)| *id)
            .collect()
    }

    /// `despawn(id)`: removes the entity from the logical screen and schedules its `Despawn`
    /// at the cursor; nothing when it is absent.
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
            ChannelId::Ambience(index) => {
                let index = usize::from(index).min(3);
                &mut self.logical.ambience[index]
            }
        }
    }

    pub fn channel(&mut self, channel: ChannelId) -> &Channel {
        self.channel_mut(channel)
    }

    fn prop_mut(&mut self, target: PropTarget, std: &StdTable) -> &mut ConstValue {
        match target {
            PropTarget::Entity(id, prop) => self.entity(id, std).prop(prop),
            PropTarget::HudTextWindow => &mut self.logical.hud.text_window,
            PropTarget::HudQuickMenu => &mut self.logical.hud.quick_menu,
            PropTarget::CameraOffset => &mut self.logical.camera.offset,
            PropTarget::CameraZoom => &mut self.logical.camera.zoom,
            PropTarget::CameraRotation => &mut self.logical.camera.rotation,
            PropTarget::ScreenFilter => &mut self.logical.filter,
            PropTarget::ChannelVolume(channel) => &mut self.channel_mut(channel).volume,
        }
    }

    /// `Prop::get`: the logical value.
    pub fn prop(&mut self, target: PropTarget, std: &StdTable) -> ConstValue {
        self.prop_mut(target, std).clone()
    }

    /// The content of a layer.
    pub fn layer(&mut self, target: LayerTarget, std: &StdTable) -> Option<ConstValue> {
        match target {
            LayerTarget::Part(id, part) => self.entity(id, std).parts.get(&part).cloned(),
            LayerTarget::Label(id) => self.entity(id, std).label.clone(),
            LayerTarget::Movie(id) => self.entity(id, std).movie.clone(),
            LayerTarget::SideImage => self.logical.hud.side_image.clone(),
        }
    }

    fn set_layer(&mut self, target: LayerTarget, content: Option<ConstValue>, std: &StdTable) {
        match target {
            LayerTarget::Part(id, part) => {
                let parts = &mut self.entity(id, std).parts;
                match content {
                    Some(content) => {
                        parts.insert(part, content);
                    }
                    None => {
                        parts.remove(&part);
                    }
                }
            }
            LayerTarget::Label(id) => self.entity(id, std).label = content,
            LayerTarget::Movie(id) => self.entity(id, std).movie = content,
            LayerTarget::SideImage => self.logical.hud.side_image = content,
        }
    }

    /// `ImageLayer::set_resize`: `Natural` is stored as no resize.
    pub fn set_resize(
        &mut self,
        target: LayerTarget,
        resize: ConstValue,
        std: &StdTable,
    ) -> Result<(), ErrorKind> {
        let natural = resize == std.resize.unit("Natural");
        match target {
            LayerTarget::Part(id, part) => {
                let layers = &mut self.entity(id, std).resize;
                if natural {
                    layers.remove(&part);
                } else {
                    layers.insert(part, LayerResize::Set(resize));
                }
            }
            LayerTarget::SideImage => {
                self.logical.hud.side_image_resize = (!natural).then_some(resize);
            }
            LayerTarget::Label(_) | LayerTarget::Movie(_) => return Err(ErrorKind::Abort),
        }
        Ok(())
    }

    /// Checks the value a pending change will set: fractions for opacity and volume.
    pub fn check_change(change: &PendingChange) -> Result<(), Fault> {
        match change {
            PendingChange::Prop(PropTarget::Entity(_, EntityProp::Opacity), value) => {
                anim::check_fraction(value, "opacity")
            }
            PendingChange::Prop(PropTarget::ChannelVolume(_), value) => {
                anim::check_fraction(value, "channel volume")
            }
            _ => Ok(()),
        }
    }

    /// `apply_animation` on a pending change: the change takes effect on the logical screen
    /// at once and `animation` (a `Tween`, `Transition` or `AudioFade`) is scheduled at the
    /// cursor plus its delay. A channel change to the track already playing is a no-op.
    pub fn commit(
        &mut self,
        change: PendingChange,
        animation: ConstValue,
        std: &StdTable,
    ) -> Result<(), Fault> {
        match change {
            PendingChange::Prop(target, value) => {
                anim::check_tween(&animation, std)?;
                let delay = anim::delay_ms(&animation, &std.tween, std);
                *self.prop_mut(target, std) = value;
                self.schedule(
                    Some(Target::Prop(target)),
                    ScheduledKind::Tween(animation),
                    delay,
                )
            }
            PendingChange::Layer(target, content) => {
                anim::check_transition(&animation, std)?;
                let delay = anim::delay_ms(&animation, &std.transition, std);
                self.set_layer(target, content, std);
                self.schedule(
                    Some(Target::Layer(target)),
                    ScheduledKind::Transition(animation),
                    delay,
                )
            }
            PendingChange::Channel(channel, track) => {
                let current = &mut self.channel_mut(channel).track;
                if *current == track {
                    return Ok(());
                }
                *current = track;
                self.schedule(
                    Some(Target::Channel(channel)),
                    ScheduledKind::AudioFade(animation),
                    0,
                )
            }
        }
    }

    /// `apply_animation(motion)` on an entity or the camera: checks the identity rule and
    /// schedules the motion; the logical screen does not change.
    pub fn apply_motion(
        &mut self,
        target: Target,
        motion: ConstValue,
        std: &StdTable,
    ) -> Result<(), Fault> {
        anim::check_motion(&motion, std)?;
        let delay = anim::delay_ms(&motion, &std.motion, std);
        if let Target::Entity(id) = target {
            self.entity(id, std);
        }
        self.schedule(Some(target), ScheduledKind::Motion(motion), delay)
    }

    /// `travel(to, motion)` on entity `id`: the logical pose becomes `to` at once through
    /// three tweens with the motion's timing, and the path is scheduled as a `Travel`.
    pub fn travel(
        &mut self,
        id: u32,
        to: ConstValue,
        motion: ConstValue,
        std: &StdTable,
    ) -> Result<(), ErrorKind> {
        let entity = self.entity(id, std);
        let from = std.pose.make(vec![
            entity.position.clone(),
            entity.scale.clone(),
            entity.rotation.clone(),
        ]);
        let info = &std.transitional_motion;
        let part = |name: &str| {
            info.field(name)
                .and_then(|index| motion.field(index))
                .cloned()
                .ok_or(ErrorKind::Abort)
        };
        let easing = part("easing")?;
        anim::check_easing(&easing, std)?;
        let tween = std
            .tween
            .make(vec![part("duration")?, easing, part("delay")?]);
        let delay = anim::delay_ms(&motion, info, std);
        for (prop, name) in [
            (EntityProp::Position, "position"),
            (EntityProp::Scale, "scale"),
            (EntityProp::Rotation, "rotation"),
        ] {
            let value = std
                .pose
                .field(name)
                .and_then(|index| to.field(index))
                .cloned()
                .ok_or(ErrorKind::Abort)?;
            self.commit(
                PendingChange::Prop(PropTarget::Entity(id, prop), value),
                tween.clone(),
                std,
            )?;
        }
        self.schedule(
            Some(Target::Travel(id)),
            ScheduledKind::Travel { motion, from, to },
            delay,
        )?;
        Ok(())
    }

    /// Schedules at the cursor plus `delay_ms`.
    pub fn schedule(
        &mut self,
        target: Option<Target>,
        kind: ScheduledKind,
        delay_ms: u32,
    ) -> Result<(), Fault> {
        let start = self.cursor_plus(delay_ms)?;
        self.step.scheduled.push(Scheduled {
            start,
            target,
            kind,
        });
        Ok(())
    }

    fn cursor_plus(&self, ms: u32) -> Result<Cursor, Fault> {
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

    /// `play(sound)`.
    pub fn play(&mut self, sound: ConstValue, std: &StdTable) -> Result<(), Fault> {
        if let Some(volume) = std
            .sound
            .field("volume")
            .and_then(|index| sound.field(index))
        {
            anim::check_fraction(volume, "sound volume")?;
        }
        let delay = anim::delay_ms(&sound, &std.sound, std);
        self.schedule(None, ScheduledKind::Sound(sound), delay)
    }

    /// `stop_sounds(fade_out)`.
    pub fn stop_sounds(&mut self, fade_out_ms: u32) -> Result<(), Fault> {
        self.schedule(None, ScheduledKind::StopSounds { fade_out_ms }, 0)
    }

    /// `replay_bgm()`: restarts the music channel's track at the cursor; nothing when the
    /// channel is silent. The logical screen does not change.
    pub fn replay_music(&mut self) -> Result<(), Fault> {
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
    pub fn wait(&mut self, ms: u32) -> Result<(), Fault> {
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
    pub fn begin_text(&mut self) -> Result<(), Fault> {
        if let CursorAnchor::Mark(mark) = self.step.cursor.anchor {
            return Err(Fault::new(
                DiagnosticCode::EvalBeginTextOnMark,
                format!("`begin_text` while the timeline is anchored on `Mark({mark})`"),
            ));
        }
        self.step.text_start = self.step.cursor;
        Ok(())
    }
}

pub(crate) fn duration_overflow() -> Fault {
    Fault::new(
        DiagnosticCode::EvalDurationOverflow,
        "`Duration` overflow: the time does not fit in `u32` milliseconds",
    )
}
