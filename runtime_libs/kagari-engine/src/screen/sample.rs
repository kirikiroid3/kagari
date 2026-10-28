//! Sampling a step's screen at a time: the visual values of every property and layer, with the
//! cut-to-end rule of the timeline, and the motions playing.

use kagari_vm::{AssetId, StaticId, Text, Value};

use super::model::{
    Entity, EntityProp, LayerContent, LayerTarget, LogicalScreen, PropTarget, PropValue,
    ScheduledKind, Target,
};
use super::runtime::ScreenRuntime;
use crate::anim::{color_premul_linear, decimal_f32, ease, lerp4, motion_at, transition_progress};
use crate::values::{BlendMode, Flip, Plane, TransitionKind, Visibility};

/// A screen as drawn at one time.
#[derive(Clone, Debug)]
pub(crate) struct VisualScreen {
    /// Sorted by `(plane, z, id)`.
    pub entities: Vec<VisualEntity>,
    pub camera: VisualCamera,
    pub hud: VisualHud,
}

/// Hud visibilities as fade alphas (Shown = 1.0, Hidden = 0.0), and the side image layer.
#[derive(Clone, Debug)]
pub(crate) struct VisualHud {
    pub text_window: f32,
    pub quick_menu: f32,
    pub side_image: VisualLayer<AssetId>,
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
    pub parts: Vec<(u8, VisualLayer<AssetId>)>,
    pub label: VisualLayer<Text>,
    pub label_style: Option<Value>,
    pub label_width: Option<u32>,
    pub flip: Flip,
    pub blend: BlendMode,
    pub gate: Option<StaticId>,
}

#[derive(Clone, Debug)]
pub(crate) enum VisualLayer<T> {
    Empty,
    Shown(T),
    Transition {
        from: Option<T>,
        to: Option<T>,
        kind: TransitionKind,
        progress: f32,
        /// ThroughColor: `hold / (duration + hold)`; 0 for other kinds.
        hold_fraction: f32,
    },
}

/// Interpolation lanes of a property value.
fn lanes(value: PropValue) -> [f32; 4] {
    match value {
        PropValue::Point(p) | PropValue::Offset(p) => [p[0] as f32, p[1] as f32, 0.0, 0.0],
        PropValue::Decimal(d) | PropValue::Angle(d) => [decimal_f32(d), 0.0, 0.0, 0.0],
        PropValue::Color(c) => color_premul_linear(c),
        PropValue::Visibility(v) => [
            if v == Visibility::Shown { 1.0 } else { 0.0 },
            0.0,
            0.0,
            0.0,
        ],
        PropValue::Shader(_) => [0.0; 4],
    }
}

fn layer_of<T>(content: Option<T>) -> VisualLayer<T> {
    match content {
        Some(content) => VisualLayer::Shown(content),
        None => VisualLayer::Empty,
    }
}

fn image(content: Option<LayerContent>) -> Option<AssetId> {
    match content {
        Some(LayerContent::Image(id)) => Some(id),
        _ => None,
    }
}

fn label(content: Option<LayerContent>) -> Option<Text> {
    match content {
        Some(LayerContent::Label(text)) => Some(text),
        _ => None,
    }
}

impl ScreenRuntime {
    fn start(&self, index: usize) -> Option<u64> {
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

    fn tween_value(&self, index: usize) -> Option<(PropValue, &crate::values::Tween)> {
        match &self.screen.step.scheduled.get(index)?.kind {
            ScheduledKind::Tween { tween, value } => Some((*value, tween)),
            _ => None,
        }
    }

    /// A property's lanes at `t`.
    fn sample_prop(&self, target: PropTarget, base: PropValue, t: u64) -> [f32; 4] {
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

    fn layer_content(
        &self,
        index: usize,
    ) -> Option<(Option<LayerContent>, &crate::values::Transition)> {
        match &self.screen.step.scheduled.get(index)?.kind {
            ScheduledKind::Transition {
                transition,
                content,
            } => Some((content.clone(), transition)),
            _ => None,
        }
    }

    /// A layer at `t`.
    fn sample_layer<T>(
        &self,
        target: LayerTarget,
        base: Option<LayerContent>,
        t: u64,
        pick: fn(Option<LayerContent>) -> Option<T>,
    ) -> VisualLayer<T> {
        let Some((active, from)) = self.chain(Target::Layer(target), t) else {
            return layer_of(pick(base));
        };
        let Some((to, transition)) = self.layer_content(active) else {
            return layer_of(pick(base));
        };
        let (x, elapsed) = self.linear_progress(active, t);
        if x >= 1.0 || transition.kind == TransitionKind::Cut {
            return layer_of(pick(to));
        }
        let from = match from.and_then(|i| self.layer_content(i)) {
            Some((content, _)) => content,
            None => base,
        };
        VisualLayer::Transition {
            from: pick(from),
            to: pick(to),
            kind: transition.kind,
            progress: transition_progress(transition, elapsed, self.length(active)),
            hold_fraction: hold_fraction(transition),
        }
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

    /// Whether a despawning entity still has an item that is unresolved or ends after `t`.
    fn still_animating(&self, id: u32, t: u64) -> bool {
        self.by_target.iter().any(|(target, indices)| {
            let mine = match target {
                Target::Prop(PropTarget::Entity(e, _)) | Target::Entity(e) => *e == id,
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
            |p: EntityProp| self.sample_prop(PropTarget::Entity(id, p), base_entity.prop(p), t);
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
                match self.sample_layer(target, base_entity.layer(target), t, image) {
                    VisualLayer::Empty => None,
                    layer => Some((part, layer)),
                }
            })
            .collect();
        let label_target = LayerTarget::Label(id);
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
            label: self.sample_layer(label_target, base_entity.layer(label_target), t, label),
            label_style: record.label_style.clone(),
            label_width: record.label_width,
            flip: record.flip,
            blend: record.blend,
            gate: record.gate,
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
        let offset = self.sample_prop(
            PropTarget::CameraOffset,
            base.read_prop(PropTarget::CameraOffset),
            t,
        );
        let zoom = self.sample_prop(
            PropTarget::CameraZoom,
            base.read_prop(PropTarget::CameraZoom),
            t,
        );
        let rotation = self.sample_prop(
            PropTarget::CameraRotation,
            base.read_prop(PropTarget::CameraRotation),
            t,
        );
        let (motion_offset, motion_scale, motion_rotation) = self.motions(Target::Camera, t);
        let fade = |target: PropTarget| {
            self.sample_prop(target, base.read_prop(target), t)[0].clamp(0.0, 1.0)
        };
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
                side_image: self.sample_layer(
                    LayerTarget::SideImage,
                    base.read_layer(LayerTarget::SideImage),
                    t,
                    image,
                ),
            },
        }
    }
}

/// `hold / (duration + hold)` of a ThroughColor transition; 0 otherwise or when both are 0.
fn hold_fraction(transition: &crate::values::Transition) -> f32 {
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
