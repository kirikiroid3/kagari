//! `std::ui::style`: paints, looks and control styles, their defaults, look selection and the
//! look-change tween.

use kagari_vm::{AssetId, Trap, Value};

use super::text::TextStyleSpec;
use crate::HostCtx;
use crate::anim::{color_premul_linear, lerp4, tween_progress};
use crate::values::{
    Easing, NamedEasing, SoundSpec, StdTy, Tween, decode_asset, decode_color, decode_int,
    decode_size, decode_sound, decode_tween,
};

/// What fills a box; colours premultiplied linear.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub(crate) enum Paint {
    #[default]
    None,
    Color([f32; 4]),
    Image(AssetId),
    /// `insets`: [top, right, bottom, left].
    NineSlice {
        image: AssetId,
        insets: [f32; 4],
    },
}

/// A background and a text style; `text` `None` is `System::text` (the default style).
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) struct Look {
    pub background: Paint,
    pub text: Option<TextStyleSpec>,
}

/// A decoded `ButtonStyle`.
#[derive(Clone, PartialEq, Debug)]
pub(crate) struct ButtonStyleSpec {
    pub idle: Look,
    pub hover: Option<Look>,
    pub focus: Option<Look>,
    pub pressed: Option<Look>,
    pub disabled: Option<Look>,
    pub change: Tween,
    pub hover_sound: Option<SoundSpec>,
    pub click_sound: Option<SoundSpec>,
}

impl ButtonStyleSpec {
    /// The style of a control without `style`: no idle background, a faint white hover and
    /// focus background, `System::text`, instant changes, no sounds.
    pub fn default_style() -> ButtonStyleSpec {
        let highlight = Look {
            background: Paint::Color([0.15; 4]),
            text: None,
        };
        ButtonStyleSpec {
            idle: Look {
                background: Paint::None,
                text: None,
            },
            hover: Some(highlight),
            focus: Some(highlight),
            pressed: None,
            disabled: None,
            change: instant(),
            hover_sound: None,
            click_sound: None,
        }
    }
}

/// A decoded `SliderStyle`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) struct SliderStyleSpec {
    pub track: Paint,
    pub fill: Paint,
    pub thumb: Paint,
    pub thumb_size: [f32; 2],
    /// The default style draws track and fill as 4 px bars; a decoded style fills the track's
    /// full height.
    pub bars: bool,
}

impl SliderStyleSpec {
    pub fn default_style() -> SliderStyleSpec {
        SliderStyleSpec {
            track: Paint::Color([0.3; 4]),
            fill: Paint::Color([0.8; 4]),
            thumb: Paint::Color([1.0; 4]),
            thumb_size: [16.0, 24.0],
            bars: true,
        }
    }
}

fn instant() -> Tween {
    Tween {
        duration_ms: 0,
        easing: Easing::Named(NamedEasing::Linear),
        delay_ms: 0,
    }
}

/// Decodes a `Paint`, loading its image.
pub(crate) fn decode_paint(ctx: &mut HostCtx, v: &Value) -> Result<Paint, Trap> {
    let std = &ctx.state.std;
    let (variant, fields) = std.variant(v, StdTy::Paint)?;
    let paint = match (variant, fields) {
        (0, []) => Paint::None,
        (1, [c]) => Paint::Color(color_premul_linear(decode_color(std, c)?)),
        (2, [image]) => Paint::Image(decode_asset(image)?),
        (3, [image, insets]) => {
            let f = std.fields(insets, StdTy::Insets)?;
            let mut out = [0.0f32; 4];
            for (slot, field) in out.iter_mut().zip(f) {
                *slot = decode_int::<i32>(field, "Insets", "inset")? as f32;
            }
            Paint::NineSlice {
                image: decode_asset(image)?,
                insets: out.map(|v| v.max(0.0)),
            }
        }
        _ => return Err(Trap::host("malformed `std::ui::style::Paint`")),
    };
    if let Paint::Image(image) | Paint::NineSlice { image, .. } = paint {
        ctx.state.assets.ensure_image(image, ctx.platform)?;
    }
    Ok(paint)
}

fn decode_look(ctx: &mut HostCtx, v: &Value) -> Result<Look, Trap> {
    let f = ctx.state.std.fields(v, StdTy::Look)?.to_vec();
    let [background, text] = f.as_slice() else {
        return Err(Trap::host("malformed `std::ui::style::Look`"));
    };
    Ok(Look {
        background: decode_paint(ctx, background)?,
        text: Some(ctx.state.text.decode_style(&ctx.state.assets, text)?),
    })
}

fn decode_option_look(ctx: &mut HostCtx, v: &Value) -> Result<Option<Look>, Trap> {
    let inner = ctx.state.std.option(v)?.cloned();
    inner.map(|look| decode_look(ctx, &look)).transpose()
}

fn decode_option_sound(ctx: &HostCtx, v: &Value) -> Result<Option<SoundSpec>, Trap> {
    let std = &ctx.state.std;
    std.option(v)?
        .map(|s| decode_sound(std, &ctx.state.program, s))
        .transpose()
}

/// Decodes a `ButtonStyle` (8 fields), loading its images.
pub(crate) fn decode_button_style(ctx: &mut HostCtx, v: &Value) -> Result<ButtonStyleSpec, Trap> {
    let f = ctx.state.std.fields(v, StdTy::ButtonStyle)?.to_vec();
    let [
        idle,
        hover,
        focus,
        pressed,
        disabled,
        change,
        hover_sound,
        click_sound,
    ] = f.as_slice()
    else {
        return Err(Trap::host("malformed `std::ui::style::ButtonStyle`"));
    };
    Ok(ButtonStyleSpec {
        idle: decode_look(ctx, idle)?,
        hover: decode_option_look(ctx, hover)?,
        focus: decode_option_look(ctx, focus)?,
        pressed: decode_option_look(ctx, pressed)?,
        disabled: decode_option_look(ctx, disabled)?,
        change: decode_tween(&ctx.state.std, change)?,
        hover_sound: decode_option_sound(ctx, hover_sound)?,
        click_sound: decode_option_sound(ctx, click_sound)?,
    })
}

/// Decodes a `SliderStyle` (4 fields), loading its images.
pub(crate) fn decode_slider_style(ctx: &mut HostCtx, v: &Value) -> Result<SliderStyleSpec, Trap> {
    let f = ctx.state.std.fields(v, StdTy::SliderStyle)?.to_vec();
    let [track, fill, thumb, thumb_size] = f.as_slice() else {
        return Err(Trap::host("malformed `std::ui::style::SliderStyle`"));
    };
    let [w, h] = decode_size(&ctx.state.std, thumb_size)?;
    Ok(SliderStyleSpec {
        track: decode_paint(ctx, track)?,
        fill: decode_paint(ctx, fill)?,
        thumb: decode_paint(ctx, thumb)?,
        thumb_size: [w as f32, h as f32],
        bars: false,
    })
}

/// Which look of a `ButtonStyle` a control shows.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum LookKey {
    Idle,
    Hover,
    Focus,
    Pressed,
    Disabled,
}

/// The look a control shows, and how.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct LookChoice {
    pub key: LookKey,
    /// A toggle that is on but has neither a pressed nor a hover look: a 2 px outline in the idle
    /// text colour.
    pub outline: bool,
    /// A disabled control without a disabled look: its idle look at half opacity.
    pub dim: bool,
}

/// The interaction state of a control.
#[derive(Clone, Copy, Default, Debug)]
pub(crate) struct Interaction {
    pub disabled: bool,
    pub hovered: bool,
    pub focused: bool,
    pub pressed: bool,
    /// A toggle whose value is true.
    pub on: bool,
}

/// `docs/std/en/src/ui-style.md` §Interaction looks: disabled replaces everything; else the
/// first `Some` of pressed (while held over the control), focus (focused with focus shown) and
/// hover; else idle.
pub(crate) fn choose(style: &ButtonStyleSpec, i: Interaction) -> LookChoice {
    let choice = |key| LookChoice {
        key,
        outline: false,
        dim: false,
    };
    if i.disabled {
        return match style.disabled {
            Some(_) => choice(LookKey::Disabled),
            None => LookChoice {
                dim: true,
                ..choice(LookKey::Idle)
            },
        };
    }
    let key = if i.pressed && i.hovered && style.pressed.is_some() {
        LookKey::Pressed
    } else if i.focused && style.focus.is_some() {
        LookKey::Focus
    } else if i.hovered && style.hover.is_some() {
        LookKey::Hover
    } else {
        LookKey::Idle
    };
    if i.on && key == LookKey::Idle {
        if style.pressed.is_some() {
            return choice(LookKey::Pressed);
        }
        if style.hover.is_some() {
            return choice(LookKey::Hover);
        }
        return LookChoice {
            outline: true,
            ..choice(LookKey::Idle)
        };
    }
    choice(key)
}

/// The look `key` of `style`; a missing look is the idle one.
pub(crate) fn look(style: &ButtonStyleSpec, key: LookKey) -> Look {
    let pick = match key {
        LookKey::Idle => None,
        LookKey::Hover => style.hover,
        LookKey::Focus => style.focus,
        LookKey::Pressed => style.pressed,
        LookKey::Disabled => style.disabled,
    };
    pick.unwrap_or(style.idle)
}

/// The look-change tween of one control.
#[derive(Clone, Copy, Default, Debug)]
pub(crate) struct LookState {
    current: Option<LookChoice>,
    from: Option<Look>,
    start_ms: u64,
}

/// A look as drawn at one moment: backgrounds with their alphas, and the text style.
#[derive(Clone, Debug)]
pub(crate) struct DrawnLook {
    pub backgrounds: Vec<(Paint, f32)>,
    /// `None`: `System::text`.
    pub text: Option<TextStyleSpec>,
    pub choice: LookChoice,
}

impl LookState {
    /// The look of `style` for `choice` at `now_ms`, tweening from the previous look by
    /// `style.change` (instant when `animations` is off).
    pub fn draw(
        &mut self,
        style: &ButtonStyleSpec,
        choice: LookChoice,
        now_ms: u64,
        animations: bool,
        system: Option<&TextStyleSpec>,
    ) -> DrawnLook {
        let to = look(style, choice.key);
        match self.current {
            Some(current) if current == choice => {}
            Some(current) => {
                self.from = Some(look(style, current.key));
                self.start_ms = now_ms;
                self.current = Some(choice);
            }
            None => {
                self.current = Some(choice);
                self.from = None;
            }
        }
        let p = match self.from {
            Some(_) if animations => {
                tween_progress(&style.change, now_ms.saturating_sub(self.start_ms))
            }
            _ => 1.0,
        };
        let from = self.from.filter(|_| p < 1.0);
        let text_of = |l: &Look| l.text.or_else(|| system.copied());
        let backgrounds = match from {
            None => vec![(to.background, 1.0)],
            Some(from) => match (from.background, to.background) {
                (Paint::Color(a), Paint::Color(b)) => vec![(Paint::Color(lerp4(a, b, p)), 1.0)],
                (a, b) => vec![(a, 1.0 - p), (b, p)],
            },
        };
        let text = match (from.and_then(|f| text_of(&f)), text_of(&to)) {
            (Some(a), Some(b)) => Some(b.with_color(lerp4(a.color(), b.color(), p))),
            (_, b) => b,
        };
        DrawnLook {
            backgrounds,
            text,
            choice,
        }
    }
}
