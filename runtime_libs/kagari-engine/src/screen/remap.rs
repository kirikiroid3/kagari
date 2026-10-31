//! A screen rebuilt for another program (scene hot swap): its assets, statics and effect
//! argument values by path ([`Remap`]). Only a screen at a step's entry moves (what a render
//! receives, which is what the story keeps between steps); its step state is fresh.

use std::collections::BTreeMap;
use std::rc::Rc;

use kagari_vm::{HostObj, Remap, RemapError};

use super::model::{Channel, Entity, Hud, LogicalScreen, PartContent, ScreenObj};
use crate::values::{EffectSpec, MovieSpec, PaintSpec};

impl ScreenObj {
    /// This screen for the new program; `None` when it holds step state (scheduled items,
    /// despawning entities, disabled arms), which only the step that made it can use.
    pub(crate) fn remapped(&self, r: &mut Remap<'_>) -> Result<Option<HostObj>, RemapError> {
        let step = &self.step;
        if !step.scheduled.is_empty()
            || !step.despawning.is_empty()
            || !step.disabled_arms.is_empty()
        {
            return Ok(None);
        }
        let mut screen = ScreenObj {
            logical: self.logical.remapped(r)?,
            step: self.step.clone(),
        };
        screen.step_entry();
        Ok(Some(HostObj::new(screen)))
    }
}

impl LogicalScreen {
    fn remapped(&self, r: &mut Remap<'_>) -> Result<LogicalScreen, RemapError> {
        let mut entities = BTreeMap::new();
        for (id, entity) in &self.entities {
            entities.insert(*id, entity.remapped(r)?);
        }
        let mut ambience = self.ambience.clone();
        for channel in &mut ambience {
            *channel = channel.remapped(r)?;
        }
        Ok(LogicalScreen {
            entities,
            hud: Hud {
                text_window: self.hud.text_window,
                quick_menu: self.hud.quick_menu,
                side_image: self
                    .hud
                    .side_image
                    .as_ref()
                    .map(|p| part(p, r))
                    .transpose()?,
                side_image_resize: self.hud.side_image_resize,
            },
            camera: self.camera.clone(),
            filter: opt_effect(self.filter.as_ref(), r)?,
            music: self.music.remapped(r)?,
            ambience,
        })
    }
}

impl Entity {
    fn remapped(&self, r: &mut Remap<'_>) -> Result<Entity, RemapError> {
        let mut parts = BTreeMap::new();
        for (index, content) in &self.parts {
            parts.insert(*index, part(content, r)?);
        }
        Ok(Entity {
            filter: opt_effect(self.filter.as_ref(), r)?,
            mix: opt_effect(self.mix.as_ref(), r)?,
            mask: opt_effect(self.mask.as_ref(), r)?,
            local_motion: opt_effect(self.local_motion.as_ref(), r)?,
            parts,
            label: self.label.as_ref().map(|t| r.text(t)).transpose()?,
            label_style: self.label_style.as_ref().map(|v| r.value(v)).transpose()?,
            movie: self
                .movie
                .as_ref()
                .map(|m| {
                    Ok(MovieSpec {
                        video: r.asset(m.video)?,
                        looping: m.looping,
                    })
                })
                .transpose()?,
            gate: self.gate.map(|g| r.static_id(g)).transpose()?,
            ..self.clone()
        })
    }
}

impl Channel {
    fn remapped(&self, r: &mut Remap<'_>) -> Result<Channel, RemapError> {
        let mut track = self.track.clone();
        if let Some(track) = track.as_mut() {
            track.audio = r.asset(track.audio)?;
        }
        Ok(Channel {
            track,
            volume: self.volume,
        })
    }
}

fn part(content: &PartContent, r: &mut Remap<'_>) -> Result<PartContent, RemapError> {
    Ok(match content {
        PartContent::Image(image) => PartContent::Image(r.asset(*image)?),
        PartContent::Paint(paint) => PartContent::Paint(Rc::new(PaintSpec {
            texture: effect(&paint.texture, r)?,
            size: paint.size,
            mask: paint.mask.as_ref().map(|m| effect(m, r)).transpose()?,
        })),
    })
}

fn opt_effect(
    e: Option<&Rc<EffectSpec>>,
    r: &mut Remap<'_>,
) -> Result<Option<Rc<EffectSpec>>, RemapError> {
    e.map(|e| effect(e, r).map(Rc::new)).transpose()
}

/// The shader and the argument value by path; the converted and packed arguments stay (the
/// argument types are the same, or [`Remap::value`] refuses).
fn effect(e: &EffectSpec, r: &mut Remap<'_>) -> Result<EffectSpec, RemapError> {
    Ok(EffectSpec {
        shader: r.asset(e.shader)?,
        args: r.value(&e.args)?,
        arg_value: e.arg_value.clone(),
        packed: e.packed.clone(),
        behavior: e.behavior,
    })
}
