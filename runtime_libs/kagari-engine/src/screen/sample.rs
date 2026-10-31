//! Sampling a step's screen at a time: the visual values of every property and layer, with the
//! cut-to-end rule of the timeline, the motions playing, and the shader effects with their
//! effect clocks.

use std::rc::Rc;

use kagari_shader::ArgValue;
use kagari_vm::{AssetId, StaticId, Text, Value};

use super::model::{
    Entity, EntityProp, LayerContent, LayerTarget, LogicalScreen, PropTarget, PropValue,
    ScheduledKind, Target,
};
use super::runtime::{EffectKey, EffectSlot, ScreenRuntime};
use crate::anim::{color_premul_linear, decimal_f32, ease, lerp4, motion_at, transition_progress};
use crate::frame::{Dir, MotionStage, Pose, ShaderEffect, TransitionDraw};
use crate::values::{
    Direction, EffectSpec, Flip, MAX_ARGS_BYTES, Plane, PoseSpec, Resize, Transition,
    TransitionKind, Visibility,
};

/// A screen as drawn at one time.
#[derive(Clone, Debug)]
pub(crate) struct VisualScreen {
    /// Sorted by `(plane, z, id)`.
    pub entities: Vec<VisualEntity>,
    pub camera: VisualCamera,
    pub hud: VisualHud,
    /// The colour filter over the camera group.
    pub filter: Option<ShaderEffect>,
}

/// Hud visibilities as fade alphas (Shown = 1.0, Hidden = 0.0), and the side image layer.
#[derive(Clone, Debug)]
pub(crate) struct VisualHud {
    pub text_window: f32,
    pub quick_menu: f32,
    pub side_image: VisualImageLayer,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct VisualCamera {
    pub offset: [f32; 2],
    pub zoom: f32,
    pub rotation_deg: f32,
}

#[derive(Clone, Debug)]
pub(crate) struct VisualEntity {
    pub id: u32,
    pub plane: Plane,
    pub z: i32,
    pub position: [f32; 2],
    pub scale: f32,
    pub rotation_deg: f32,
    pub opacity: f32,
    pub anchor: [f32; 2],
    /// Premultiplied linear.
    pub fill: [f32; 4],
    pub fill_size: Option<[u32; 2]>,
    pub parts: Vec<(u8, VisualImageLayer)>,
    pub label: VisualLayer<Text>,
    pub label_style: Option<Value>,
    pub label_width: Option<u32>,
    pub flip: Flip,
    pub gate: Option<StaticId>,
    pub filter: Option<ShaderEffect>,
    pub mix: Option<ShaderEffect>,
    pub mask: Option<ShaderEffect>,
    /// GPU motion stages: a playing travel, then the local motion.
    pub stages: Vec<MotionStage>,
}

/// An image layer at one time: what it shows and how it sizes it.
#[derive(Clone, Debug)]
pub(crate) struct VisualImageLayer {
    pub layer: VisualLayer<VisualContent>,
    pub resize: Resize,
}

/// What an image layer shows at one time.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum VisualContent {
    Image(AssetId),
    Paint(VisualPaint),
}

/// A paint with its shader effects at one time.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct VisualPaint {
    pub size: [u32; 2],
    pub texture: ShaderEffect,
    pub mask: Option<ShaderEffect>,
}

#[derive(Clone, Debug)]
pub(crate) enum VisualLayer<T> {
    Empty,
    Shown(T),
    Transition {
        from: Option<T>,
        to: Option<T>,
        kind: TransitionDraw,
        progress: f32,
    },
}

/// Interpolation lanes of a property value.
fn lanes(value: &PropValue) -> [f32; 4] {
    match value {
        PropValue::Point(p) | PropValue::Offset(p) => [p[0] as f32, p[1] as f32, 0.0, 0.0],
        PropValue::Decimal(d) | PropValue::Angle(d) => [decimal_f32(*d), 0.0, 0.0, 0.0],
        PropValue::Color(c) => color_premul_linear(*c),
        PropValue::Visibility(v) => [
            if *v == Visibility::Shown { 1.0 } else { 0.0 },
            0.0,
            0.0,
            0.0,
        ],
        PropValue::Effect(_) => [0.0; 4],
    }
}

fn layer_of<T>(content: Option<T>) -> VisualLayer<T> {
    match content {
        Some(content) => VisualLayer::Shown(content),
        None => VisualLayer::Empty,
    }
}

fn label(content: Option<LayerContent>) -> Option<Text> {
    match content {
        Some(LayerContent::Label(text)) => Some(text),
        _ => None,
    }
}

/// An effect drawn with `spec`'s shader, its packed arguments (or `lerped` ones) and the effect
/// time `ms`; `None` when the arguments are over `MAX_ARGS_BYTES`.
fn shader_effect(spec: &EffectSpec, lerped: Option<&ArgValue>, ms: u64) -> Option<ShaderEffect> {
    let args: Rc<[u8]> = match lerped {
        Some(value) => Rc::from(kagari_shader::pack(value)),
        None => spec.packed.clone(),
    };
    (args.len() <= MAX_ARGS_BYTES).then(|| ShaderEffect {
        shader: spec.shader,
        args,
        time: ms as f32 / 1000.0,
    })
}

fn pose(p: &PoseSpec) -> Pose {
    Pose {
        position: [p.position[0] as f32, p.position[1] as f32],
        scale: decimal_f32(p.scale),
        rotation: decimal_f32(p.rotation).to_radians(),
    }
}

fn dir(d: Direction) -> Dir {
    match d {
        Direction::Left => Dir::Left,
        Direction::Right => Dir::Right,
        Direction::Up => Dir::Up,
        Direction::Down => Dir::Down,
    }
}

impl ScreenRuntime {
    pub(super) fn start(&self, index: usize) -> Option<u64> {
        self.items.get(index)?.start
    }

    fn length(&self, index: usize) -> u32 {
        self.items.get(index).map_or(0, |item| item.length_ms)
    }

    /// The active item of a chain at `t` (the latest committed one that has started) and the
    /// item it starts from (the latest earlier one starting no later), if any.
    fn chain(&self, target: Target, t: u64) -> Option<(usize, Option<usize>)> {
        let indices = self.by_target.get(&target)?;
        let active = *indices
            .iter()
            .rev()
            .find(|i| self.start(**i).is_some_and(|s| s <= t))?;
        let active_start = self.start(active)?;
        let from = indices
            .iter()
            .rev()
            .filter(|i| **i < active)
            .find(|i| self.start(**i).is_some_and(|s| s <= active_start))
            .copied();
        Some((active, from))
    }

    /// Progress of item `index` at `t`, before easing: 1 once ended.
    fn linear_progress(&self, index: usize, t: u64) -> (f32, u32) {
        let start = self.start(index).unwrap_or(t);
        let length = self.length(index);
        let elapsed = t.saturating_sub(start);
        let elapsed = u32::try_from(elapsed).unwrap_or(u32::MAX);
        if length == 0 || elapsed >= length {
            (1.0, elapsed)
        } else {
            (elapsed as f32 / length as f32, elapsed)
        }
    }

    fn tween_value(&self, index: usize) -> Option<(&PropValue, &crate::values::Tween)> {
        match &self.screen.step.scheduled.get(index)?.kind {
            ScheduledKind::Tween { tween, value } => Some((value, tween)),
            _ => None,
        }
    }

    /// A property's lanes at `t`.
    fn sample_prop(&self, target: PropTarget, base: &PropValue, t: u64) -> [f32; 4] {
        let Some((active, from)) = self.chain(Target::Prop(target), t) else {
            return lanes(base);
        };
        let Some((to, tween)) = self.tween_value(active) else {
            return lanes(base);
        };
        let from = from
            .and_then(|i| self.tween_value(i))
            .map_or(base, |(value, _)| value);
        let (x, _) = self.linear_progress(active, t);
        let p = if x >= 1.0 {
            1.0
        } else {
            ease(&tween.easing, x)
        };
        lerp4(lanes(from), lanes(to), p)
    }

    /// Milliseconds on the clock of the effect `key` in `slot`, with its origin taken at `at`;
    /// 0 when animations are off.
    fn effect_ms(&self, slot: EffectSlot, key: EffectKey, at: u64, t: u64) -> u64 {
        if !self.animations {
            return 0;
        }
        t.saturating_sub(self.effect_origin(slot, key, at))
    }

    /// An effect property at `t`. While a tween runs between two values of the same shader
    /// (and behaviour), the arguments interpolate; otherwise the value switches at the start.
    fn sample_effect(
        &self,
        target: PropTarget,
        slot: EffectSlot,
        base: Option<Rc<EffectSpec>>,
        t: u64,
    ) -> Option<ShaderEffect> {
        let mut lerped = None;
        let active = self
            .chain(Target::Prop(target), t)
            .and_then(|(active, from)| Some((active, from, self.tween_value(active)?)));
        let spec = match active {
            None => base?,
            Some((active, from, (to, tween))) => {
                let PropValue::Effect(to) = to else {
                    return None;
                };
                let to = to.clone()?;
                let from = match from.and_then(|i| self.tween_value(i)) {
                    Some((PropValue::Effect(from), _)) => from.clone(),
                    Some(_) => None,
                    None => base,
                };
                let (x, _) = self.linear_progress(active, t);
                if let Some(from) = from
                    && from.shader == to.shader
                    && from.behavior == to.behavior
                    && x < 1.0
                {
                    lerped =
                        kagari_shader::lerp(&from.arg_value, &to.arg_value, ease(&tween.easing, x));
                }
                to
            }
        };
        let ms = self.effect_ms(slot, EffectKey::of_effect(&spec), t, t);
        let ms = spec.behavior.map_or(ms, |behavior| behavior.map_ms(ms));
        shader_effect(&spec, lerped.as_ref(), ms)
    }

    /// An entity's effect property at `t`.
    fn entity_effect(
        &self,
        id: u32,
        prop: EntityProp,
        base: &Option<Rc<EffectSpec>>,
        t: u64,
    ) -> Option<ShaderEffect> {
        self.sample_effect(
            PropTarget::Entity(id, prop),
            EffectSlot::Entity(id, prop),
            base.clone(),
            t,
        )
    }

    fn layer_content(&self, index: usize) -> Option<(Option<LayerContent>, &Transition)> {
        match &self.screen.step.scheduled.get(index)?.kind {
            ScheduledKind::Transition {
                transition,
                content,
            } => Some((content.clone(), transition)),
            _ => None,
        }
    }

    /// How a transition draws `elapsed_ms` after its start; `None` for a cut.
    fn transition_draw(&self, transition: &Transition, elapsed_ms: u32) -> Option<TransitionDraw> {
        Some(match &transition.kind {
            TransitionKind::Cut => return None,
            TransitionKind::Crossfade => TransitionDraw::Crossfade,
            TransitionKind::Rule {
                mask,
                softness,
                reverse,
            } => TransitionDraw::Rule {
                mask: *mask,
                softness: *softness,
                reverse: *reverse,
            },
            TransitionKind::Wipe(direction) => TransitionDraw::Wipe {
                dir: dir(*direction),
            },
            TransitionKind::Blinds { direction, count } => TransitionDraw::Blinds {
                dir: dir(*direction),
                count: *count,
            },
            TransitionKind::ThroughColor { color, .. } => TransitionDraw::ThroughColor {
                color: color_premul_linear(*color),
                hold_fraction: hold_fraction(transition),
            },
            TransitionKind::Masked {
                mask,
                softness,
                reverse,
            } => {
                let ms = if self.animations {
                    u64::from(elapsed_ms)
                } else {
                    0
                };
                match shader_effect(mask, None, ms) {
                    Some(mask) => TransitionDraw::Masked {
                        mask,
                        softness: *softness,
                        reverse: *reverse,
                    },
                    None => TransitionDraw::Crossfade,
                }
            }
        })
    }

    /// A layer at `t`; `pick(content, at)` turns a content into what is drawn, with effect
    /// clocks taken at `at` (just before the transition for its `from` side).
    fn sample_layer<T>(
        &self,
        target: LayerTarget,
        base: Option<LayerContent>,
        t: u64,
        pick: impl Fn(Option<LayerContent>, u64) -> Option<T>,
    ) -> VisualLayer<T> {
        let Some((active, from)) = self.chain(Target::Layer(target), t) else {
            return layer_of(pick(base, t));
        };
        let Some((to, transition)) = self.layer_content(active) else {
            return layer_of(pick(base, t));
        };
        let (x, elapsed) = self.linear_progress(active, t);
        let kind = if x >= 1.0 {
            None
        } else {
            self.transition_draw(transition, elapsed)
        };
        let Some(kind) = kind else {
            return layer_of(pick(to, t));
        };
        let from = match from.and_then(|i| self.layer_content(i)) {
            Some((content, _)) => content,
            None => base,
        };
        let before = self.start(active).unwrap_or(t).saturating_sub(1);
        VisualLayer::Transition {
            from: pick(from, before),
            to: pick(to, t),
            kind,
            progress: transition_progress(transition, elapsed, self.length(active)),
        }
    }

    /// An image layer's content at `t`, its paint clock taken at `at`.
    fn visual_content(
        &self,
        target: LayerTarget,
        content: Option<LayerContent>,
        at: u64,
        t: u64,
    ) -> Option<VisualContent> {
        match content? {
            LayerContent::Image(image) => Some(VisualContent::Image(image)),
            LayerContent::Paint(paint) => {
                let key = EffectKey::of_paint(&paint);
                let ms = self.effect_ms(EffectSlot::Layer(target), key, at, t);
                Some(VisualContent::Paint(VisualPaint {
                    size: paint.size,
                    texture: shader_effect(&paint.texture, None, ms)?,
                    mask: paint
                        .mask
                        .as_ref()
                        .and_then(|mask| shader_effect(mask, None, ms)),
                }))
            }
            LayerContent::Label(_) | LayerContent::Movie(_) => None,
        }
    }

    fn image_layer(
        &self,
        target: LayerTarget,
        base: Option<LayerContent>,
        t: u64,
    ) -> VisualLayer<VisualContent> {
        self.sample_layer(target, base, t, |content, at| {
            self.visual_content(target, content, at, t)
        })
    }

    /// The motions on `target` playing at `t`: summed offsets and rotations, multiplied scales.
    fn motions(&self, target: Target, t: u64) -> ([f32; 2], f32, f32) {
        let mut out = ([0.0, 0.0], 1.0, 0.0);
        for index in self.by_target.get(&target).into_iter().flatten() {
            let Some(item) = self.screen.step.scheduled.get(*index) else {
                continue;
            };
            let ScheduledKind::Motion(motion) = &item.kind else {
                continue;
            };
            let Some(start) = self.start(*index) else {
                continue;
            };
            let end = start.saturating_add(u64::from(self.length(*index)));
            if start <= t && t < end {
                let elapsed = u32::try_from(t.saturating_sub(start)).unwrap_or(u32::MAX);
                let (offset, scale, rotation) = motion_at(motion, elapsed);
                out.0 = [out.0[0] + offset[0], out.0[1] + offset[1]];
                out.1 *= scale;
                out.2 += rotation;
            }
        }
        out
    }

    /// The travel of entity `id` playing at `t` (the latest committed one), as a motion stage.
    fn travel_stage(&self, id: u32, t: u64) -> Option<MotionStage> {
        let indices = self.by_target.get(&Target::Travel(id))?;
        let index = *indices.iter().rev().find(|i| {
            self.start(**i).is_some_and(|start| {
                start <= t && t < start.saturating_add(u64::from(self.length(**i)))
            })
        })?;
        let ScheduledKind::Travel { motion, from, to } =
            &self.screen.step.scheduled.get(index)?.kind
        else {
            return None;
        };
        let (x, elapsed) = self.linear_progress(index, t);
        Some(MotionStage::Transitional {
            effect: shader_effect(&motion.effect, None, u64::from(elapsed))?,
            from: pose(from),
            to: pose(to),
            progress: ease(&motion.easing, x),
        })
    }

    /// Whether a despawning entity still has an item (its `Despawn` at the cursor, or an
    /// animation) that is unresolved or ends after `t`.
    fn still_animating(&self, id: u32, t: u64) -> bool {
        self.by_target.iter().any(|(target, indices)| {
            let mine = match target {
                Target::Prop(PropTarget::Entity(e, _)) | Target::Entity(e) | Target::Travel(e) => {
                    *e == id
                }
                Target::Layer(
                    LayerTarget::Part(e, _) | LayerTarget::Label(e) | LayerTarget::Movie(e),
                ) => *e == id,
                _ => false,
            };
            mine && indices.iter().any(|i| match self.start(*i) {
                None => true,
                Some(start) => start.saturating_add(u64::from(self.length(*i))) > t,
            })
        })
    }

    fn sample_entity(&self, id: u32, record: &Entity, t: u64) -> VisualEntity {
        let base: &LogicalScreen = &self.base;
        let spawned = Entity::spawned();
        let base_entity = base.entities.get(&id).unwrap_or(&spawned);
        let prop =
            |p: EntityProp| self.sample_prop(PropTarget::Entity(id, p), &base_entity.prop(p), t);
        let position = prop(EntityProp::Position);
        let (offset, motion_scale, motion_rotation) = self.motions(Target::Entity(id), t);
        let mut keys: Vec<u8> = base_entity
            .parts
            .keys()
            .chain(record.parts.keys())
            .copied()
            .chain(self.by_target.keys().filter_map(|target| match target {
                Target::Layer(LayerTarget::Part(e, part)) if *e == id => Some(*part),
                _ => None,
            }))
            .collect();
        keys.sort_unstable();
        keys.dedup();
        let parts = keys
            .into_iter()
            .filter_map(|part| {
                let target = LayerTarget::Part(id, part);
                match self.image_layer(target, base_entity.layer(target), t) {
                    VisualLayer::Empty => None,
                    layer => Some((
                        part,
                        VisualImageLayer {
                            layer,
                            resize: record.resize(part),
                        },
                    )),
                }
            })
            .collect();
        let label_target = LayerTarget::Label(id);
        let stages = self
            .travel_stage(id, t)
            .into_iter()
            .chain(
                self.entity_effect(id, EntityProp::LocalMotion, &base_entity.local_motion, t)
                    .map(|effect| MotionStage::Local { effect }),
            )
            .collect();
        VisualEntity {
            id,
            plane: record.plane,
            z: record.z,
            position: [position[0] + offset[0], position[1] + offset[1]],
            scale: prop(EntityProp::Scale)[0] * motion_scale,
            rotation_deg: prop(EntityProp::Rotation)[0] + motion_rotation,
            opacity: prop(EntityProp::Opacity)[0].clamp(0.0, 1.0),
            anchor: [decimal_f32(record.anchor[0]), decimal_f32(record.anchor[1])],
            fill: prop(EntityProp::Fill).map(|c| c.clamp(0.0, 1.0)),
            fill_size: record.fill_size,
            parts,
            label: self.sample_layer(
                label_target,
                base_entity.layer(label_target),
                t,
                |content, _| label(content),
            ),
            label_style: record.label_style.clone(),
            label_width: record.label_width,
            flip: record.flip,
            gate: record.gate,
            filter: self.entity_effect(id, EntityProp::Filter, &base_entity.filter, t),
            mix: self.entity_effect(id, EntityProp::Mix, &base_entity.mix, t),
            mask: self.entity_effect(id, EntityProp::Mask, &base_entity.mask, t),
            stages,
        }
    }

    /// The screen at story time `now_ms`.
    pub fn sample(&self, now_ms: u64) -> VisualScreen {
        let t = now_ms;
        let logical = &self.screen.logical;
        let mut entities: Vec<VisualEntity> = logical
            .entities
            .iter()
            .map(|(id, record)| self.sample_entity(*id, record, t))
            .collect();
        for (id, record) in &self.screen.step.despawning {
            if !logical.entities.contains_key(id) && self.still_animating(*id, t) {
                entities.push(self.sample_entity(*id, record, t));
            }
        }
        entities.sort_by_key(|e| (e.plane, e.z, e.id));
        let base = &self.base;
        let read = |target: PropTarget| self.sample_prop(target, &base.read_prop(target), t);
        let offset = read(PropTarget::CameraOffset);
        let zoom = read(PropTarget::CameraZoom);
        let rotation = read(PropTarget::CameraRotation);
        let (motion_offset, motion_scale, motion_rotation) = self.motions(Target::Camera, t);
        let fade = |target: PropTarget| read(target)[0].clamp(0.0, 1.0);
        VisualScreen {
            entities,
            camera: VisualCamera {
                offset: [offset[0] + motion_offset[0], offset[1] + motion_offset[1]],
                zoom: zoom[0] * motion_scale,
                rotation_deg: rotation[0] + motion_rotation,
            },
            hud: VisualHud {
                text_window: fade(PropTarget::HudTextWindow),
                quick_menu: fade(PropTarget::HudQuickMenu),
                side_image: VisualImageLayer {
                    layer: self.image_layer(
                        LayerTarget::SideImage,
                        base.read_layer(LayerTarget::SideImage),
                        t,
                    ),
                    resize: logical.hud.side_image_resize,
                },
            },
            filter: self.sample_effect(
                PropTarget::ScreenFilter,
                EffectSlot::ScreenFilter,
                base.filter.clone(),
                t,
            ),
        }
    }
}

/// `hold / (duration + hold)` of a ThroughColor transition; 0 otherwise or when both are 0.
fn hold_fraction(transition: &Transition) -> f32 {
    match transition.kind {
        TransitionKind::ThroughColor { hold_ms, .. } => {
            let total = f64::from(transition.duration_ms) + f64::from(hold_ms);
            if total > 0.0 {
                (f64::from(hold_ms) / total) as f32
            } else {
                0.0
            }
        }
        _ => 0.0,
    }
}
