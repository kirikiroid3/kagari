//! The engine side of text: `TextStyle` and `TextEffect` decoding, flattening a `Text` tree into
//! `kagari_text` spans, the system font and style, and a layout cache.

use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::rc::Rc;

use kagari_text::{FontId, Layout, LayoutMetrics, Paragraph, ParagraphStyle, RubyText, Span};
use kagari_vm::{Adt, Text, Trap, Value, value_eq};

use crate::anim::{color_premul_linear, decimal_f32};
use crate::assets::AssetStore;

/// The layout cache is cleared when it holds more entries than this.
const CACHE_LIMIT: usize = 1024;

/// An `f32` stored by its bits, so styles can be hashed and compared exactly.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) struct Px(u32);

impl Px {
    pub fn new(v: f32) -> Px {
        Px(v.to_bits())
    }

    pub fn get(self) -> f32 {
        f32::from_bits(self.0)
    }
}

fn px4(c: [f32; 4]) -> [Px; 4] {
    c.map(Px::new)
}

fn f4(c: [Px; 4]) -> [f32; 4] {
    c.map(Px::get)
}

/// `std::narrative::text::TextAlign`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) enum TextAlign {
    Start,
    Center,
    End,
}

/// `std::narrative::text::EmphasisStyle`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) enum EmphasisStyle {
    Dots,
    Bold,
    Italic,
}

/// A decoded `TextStyle`, with colours premultiplied linear.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) struct TextStyleSpec {
    /// `None`: the system font.
    pub font: Option<FontId>,
    pub size: Px,
    pub color: [Px; 4],
    /// A multiple of `size`.
    pub line_height: Px,
    pub letter_spacing: Px,
    pub align: TextAlign,
    /// Width px, colour.
    pub outline: Option<(Px, [Px; 4])>,
    /// Offset px, blur px, colour.
    pub shadow: Option<([Px; 2], Px, [Px; 4])>,
    pub ruby_size: Px,
    pub emphasis: EmphasisStyle,
    pub hanging_indent: bool,
    pub vertical: bool,
}

impl TextStyleSpec {
    pub fn color(&self) -> [f32; 4] {
        f4(self.color)
    }

    /// This style in another colour.
    pub fn with_color(mut self, color: [f32; 4]) -> TextStyleSpec {
        self.color = px4(color);
        self
    }
}

/// A laid-out text and its metrics.
#[derive(Clone, Debug)]
pub(crate) struct TextLayout {
    pub layout: Rc<Layout>,
    pub metrics: LayoutMetrics,
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct LayoutKey {
    text_hash: u64,
    style: TextStyleSpec,
    /// Width × 64.
    width_q: Option<u32>,
}

/// The system font and style, and the layout cache. The fonts live in the `AssetStore`.
pub(crate) struct TextSystem {
    system: Option<(FontId, TextStyleSpec)>,
    cache: HashMap<LayoutKey, (Text, TextLayout)>,
}

fn malformed(what: &str) -> Trap {
    Trap::host(format!("malformed `{what}`"))
}

// ---------------------------------------------------------------------------------------------
// Positional decoding of std values

fn adt<'v>(v: &'v Value, what: &str) -> Result<&'v Adt, Trap> {
    match v {
        Value::Adt(adt) => Ok(adt),
        _ => Err(malformed(what)),
    }
}

fn fields<'v>(v: &'v Value, what: &str, count: usize) -> Result<&'v [Value], Trap> {
    let adt = adt(v, what)?;
    if adt.fields.len() == count {
        Ok(&adt.fields)
    } else {
        Err(malformed(what))
    }
}

fn option(v: &Value) -> Result<Option<&Value>, Trap> {
    let adt = adt(v, "std::Option")?;
    match (adt.variant, adt.fields.as_slice()) {
        (0, []) => Ok(None),
        (1, [inner]) => Ok(Some(inner)),
        _ => Err(malformed("std::Option")),
    }
}

fn int(v: &Value, what: &str) -> Result<i128, Trap> {
    match v {
        Value::Int(n) => Ok(*n),
        _ => Err(malformed(what)),
    }
}

fn variant(v: &Value, what: &str) -> Result<u32, Trap> {
    let adt = adt(v, what)?;
    if adt.fields.is_empty() {
        Ok(adt.variant)
    } else {
        Err(malformed(what))
    }
}

fn color(v: &Value) -> Result<[f32; 4], Trap> {
    let f = fields(v, "std::screen::Color", 4)?;
    let mut c = [0u8; 4];
    for (slot, field) in c.iter_mut().zip(f) {
        *slot = u8::try_from(int(field, "std::screen::Color")?)
            .map_err(|_| malformed("std::screen::Color"))?;
    }
    Ok(color_premul_linear(c))
}

fn outline(v: &Value) -> Result<(f32, [f32; 4]), Trap> {
    let f = fields(v, "std::narrative::text::Outline", 2)?;
    Ok((
        int(&f[1], "std::narrative::text::Outline")? as f32,
        color(&f[0])?,
    ))
}

fn shadow(v: &Value) -> Result<([f32; 2], f32, [f32; 4]), Trap> {
    let f = fields(v, "std::narrative::text::Shadow", 3)?;
    let offset = fields(&f[1], "std::screen::Offset", 2)?;
    Ok((
        [
            int(&offset[0], "std::screen::Offset")? as f32,
            int(&offset[1], "std::screen::Offset")? as f32,
        ],
        int(&f[2], "std::narrative::text::Shadow")? as f32,
        color(&f[0])?,
    ))
}

fn font_of(assets: &AssetStore, v: &Value) -> Result<FontId, Trap> {
    let Value::Asset(id) = v else {
        return Err(malformed("FontAsset"));
    };
    assets
        .font(*id)
        .ok_or_else(|| Trap::host(format!("font asset `{}` is not loaded", assets.path(*id))))
}

/// The `Emphasis` of a `TextEffect`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Emphasis {
    Styled,
    Dots,
}

/// A decoded `TextEffect` without its reveal cues; `None` fields inherit.
#[derive(Clone, Copy, Default, PartialEq, Debug)]
struct Effect {
    color: Option<[f32; 4]>,
    size: Option<f32>,
    bold: Option<bool>,
    italic: Option<bool>,
    font: Option<FontId>,
    outline: Option<(f32, [f32; 4])>,
    shadow: Option<([f32; 2], f32, [f32; 4])>,
    emphasis: Option<Emphasis>,
}

impl Effect {
    fn decode(assets: &AssetStore, v: &Value) -> Result<Effect, Trap> {
        const WHAT: &str = "std::narrative::text::TextEffect";
        let f = fields(v, WHAT, 11)?;
        Ok(Effect {
            color: option(&f[0])?.map(color).transpose()?,
            size: option(&f[1])?
                .map(|d| match d {
                    Value::Decimal(d) => Ok(decimal_f32(*d)),
                    _ => Err(malformed(WHAT)),
                })
                .transpose()?,
            bold: option(&f[2])?
                .map(|w| variant(w, "std::narrative::text::Weight").map(|w| w == 1))
                .transpose()?,
            italic: option(&f[3])?
                .map(|b| match b {
                    Value::Bool(b) => Ok(*b),
                    _ => Err(malformed(WHAT)),
                })
                .transpose()?,
            font: option(&f[4])?.map(|v| font_of(assets, v)).transpose()?,
            outline: option(&f[5])?.map(outline).transpose()?,
            shadow: option(&f[6])?.map(shadow).transpose()?,
            emphasis: option(&f[7])?
                .map(|e| {
                    variant(e, "std::narrative::text::Emphasis").map(|e| {
                        if e == 0 {
                            Emphasis::Styled
                        } else {
                            Emphasis::Dots
                        }
                    })
                })
                .transpose()?,
        })
    }

    /// `self` with the fields `inner` sets replaced (`TextEffect::and`).
    fn and(self, inner: Effect) -> Effect {
        Effect {
            color: inner.color.or(self.color),
            size: inner.size.or(self.size),
            bold: inner.bold.or(self.bold),
            italic: inner.italic.or(self.italic),
            font: inner.font.or(self.font),
            outline: inner.outline.or(self.outline),
            shadow: inner.shadow.or(self.shadow),
            emphasis: inner.emphasis.or(self.emphasis),
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Flattening

/// Builds the spans and rubies of a paragraph from a `Text` tree.
struct Flatten<'a> {
    assets: &'a AssetStore,
    style: &'a TextStyleSpec,
    /// The font of spans whose style and effects name none.
    default_font: FontId,
    spans: Vec<Span>,
    rubies: Vec<RubyText>,
    /// The ruby group the walk is inside.
    ruby: Option<u32>,
}

impl Flatten<'_> {
    fn walk(&mut self, text: &Text, effect: Effect) -> Result<(), Trap> {
        match text {
            Text::Plain(s) => {
                self.push(s, effect);
                Ok(())
            }
            Text::Concat(items) => items.iter().try_for_each(|item| self.walk(item, effect)),
            Text::Effect(pair) => {
                let inner = Effect::decode(self.assets, &pair.1)?;
                self.walk(&pair.0, effect.and(inner))
            }
            Text::Ruby(pair) => self.ruby(&pair.0, effect, || RubyText::Group(plain(&pair.1))),
            Text::MonoRuby(pair) => self.ruby(&pair.0, effect, || {
                RubyText::Mono(pair.1.iter().map(plain).collect())
            }),
        }
    }

    /// A ruby base; a ruby nested inside another's base drops its reading.
    fn ruby(
        &mut self,
        base: &Text,
        effect: Effect,
        reading: impl FnOnce() -> RubyText,
    ) -> Result<(), Trap> {
        if self.ruby.is_some() {
            return self.walk(base, effect);
        }
        let index = u32::try_from(self.rubies.len()).map_err(|_| malformed("Text"))?;
        self.rubies.push(reading());
        self.ruby = Some(index);
        let result = self.walk(base, effect);
        self.ruby = None;
        result
    }

    fn push(&mut self, s: &str, effect: Effect) {
        if s.is_empty() {
            return;
        }
        let style = self.style;
        let emphasis = effect.emphasis;
        let styled = |e: EmphasisStyle| emphasis == Some(Emphasis::Styled) && style.emphasis == e;
        let span = Span {
            text: String::new(),
            font: effect.font.or(style.font).unwrap_or(self.default_font),
            size_px: style.size.get() * effect.size.unwrap_or(1.0),
            color: effect.color.unwrap_or_else(|| style.color()),
            bold: effect.bold == Some(true) || styled(EmphasisStyle::Bold),
            italic: effect.italic == Some(true) || styled(EmphasisStyle::Italic),
            outline: effect
                .outline
                .or_else(|| style.outline.map(|(w, c)| (w.get(), f4(c)))),
            shadow: effect.shadow.or_else(|| {
                style
                    .shadow
                    .map(|(o, b, c)| (o.map(Px::get), b.get(), f4(c)))
            }),
            emphasis_dots: emphasis == Some(Emphasis::Dots) || styled(EmphasisStyle::Dots),
            ruby: self.ruby,
        };
        if let Some(last) = self.spans.last_mut()
            && same_style(last, &span)
        {
            last.text.push_str(s);
            return;
        }
        self.spans.push(Span {
            text: s.to_owned(),
            ..span
        });
    }
}

fn same_style(a: &Span, b: &Span) -> bool {
    a.font == b.font
        && a.size_px == b.size_px
        && a.color == b.color
        && a.bold == b.bold
        && a.italic == b.italic
        && a.outline == b.outline
        && a.shadow == b.shadow
        && a.emphasis_dots == b.emphasis_dots
        && a.ruby == b.ruby
}

/// The base text of `text`: `Plain` leaves in order, ruby readings left out.
fn plain(text: &Text) -> String {
    fn walk(text: &Text, out: &mut String) {
        match text {
            Text::Plain(s) => out.push_str(s),
            Text::Ruby(pair) => walk(&pair.0, out),
            Text::MonoRuby(pair) => walk(&pair.0, out),
            Text::Effect(pair) => walk(&pair.0, out),
            Text::Concat(items) => items.iter().for_each(|item| walk(item, out)),
        }
    }
    let mut out = String::new();
    walk(text, &mut out);
    out
}

// ---------------------------------------------------------------------------------------------
// Hashing for the cache

fn hash_text(text: &Text, h: &mut DefaultHasher) {
    match text {
        Text::Plain(s) => {
            0u8.hash(h);
            s.hash(h);
        }
        Text::Ruby(pair) => {
            1u8.hash(h);
            hash_text(&pair.0, h);
            hash_text(&pair.1, h);
        }
        Text::MonoRuby(pair) => {
            2u8.hash(h);
            hash_text(&pair.0, h);
            pair.1.len().hash(h);
            pair.1.iter().for_each(|r| hash_text(r, h));
        }
        Text::Effect(pair) => {
            3u8.hash(h);
            hash_text(&pair.0, h);
            hash_value(&pair.1, h);
        }
        Text::Concat(items) => {
            4u8.hash(h);
            items.len().hash(h);
            items.iter().for_each(|item| hash_text(item, h));
        }
    }
}

fn hash_value(v: &Value, h: &mut DefaultHasher) {
    v.tag().hash(h);
    match v {
        Value::Bool(b) => b.hash(h),
        Value::Int(n) => n.hash(h),
        Value::Decimal(d) => {
            d.mantissa().hash(h);
            d.scale().hash(h);
        }
        Value::Asset(a) => a.0.hash(h),
        Value::String(s) => s.hash(h),
        Value::Text(t) => hash_text(t, h),
        Value::Adt(adt) => {
            adt.variant.hash(h);
            adt.fields.iter().for_each(|f| hash_value(f, h));
        }
        Value::Tuple(items) | Value::Array(items) | Value::Vec(items) => {
            items.iter().for_each(|f| hash_value(f, h));
        }
        _ => {}
    }
}

impl TextSystem {
    pub fn new() -> TextSystem {
        TextSystem {
            system: None,
            cache: HashMap::new(),
        }
    }

    /// The `System::font` face and the decoded `System::text` (boot).
    pub fn set_system(&mut self, font: FontId, style: TextStyleSpec) {
        self.system = Some((font, style));
    }

    /// `System::text`.
    pub fn system_style(&self) -> Result<&TextStyleSpec, Trap> {
        self.system
            .as_ref()
            .map(|(_, style)| style)
            .ok_or_else(|| Trap::host("no system text style before `main` returned"))
    }

    /// Decodes a `TextStyle` (fields 0..=11).
    pub fn decode_style(&self, assets: &AssetStore, v: &Value) -> Result<TextStyleSpec, Trap> {
        const WHAT: &str = "std::narrative::text::TextStyle";
        let f = fields(v, WHAT, 12)?;
        let decimal = |v: &Value| match v {
            Value::Decimal(d) => Ok(decimal_f32(*d)),
            _ => Err(malformed(WHAT)),
        };
        let boolean = |v: &Value| match v {
            Value::Bool(b) => Ok(*b),
            _ => Err(malformed(WHAT)),
        };
        let align = match variant(&f[5], "std::narrative::text::TextAlign")? {
            0 => TextAlign::Start,
            1 => TextAlign::Center,
            2 => TextAlign::End,
            _ => return Err(malformed("std::narrative::text::TextAlign")),
        };
        let emphasis = match variant(&f[9], "std::narrative::text::EmphasisStyle")? {
            0 => EmphasisStyle::Dots,
            1 => EmphasisStyle::Bold,
            2 => EmphasisStyle::Italic,
            _ => return Err(malformed("std::narrative::text::EmphasisStyle")),
        };
        Ok(TextStyleSpec {
            font: option(&f[0])?.map(|v| font_of(assets, v)).transpose()?,
            size: Px::new(int(&f[1], WHAT)? as f32),
            color: px4(color(&f[2])?),
            line_height: Px::new(decimal(&f[3])?),
            letter_spacing: Px::new(int(&f[4], WHAT)? as f32),
            align,
            outline: option(&f[6])?
                .map(outline)
                .transpose()?
                .map(|(w, c)| (Px::new(w), px4(c))),
            shadow: option(&f[7])?
                .map(shadow)
                .transpose()?
                .map(|(o, b, c)| (o.map(Px::new), Px::new(b), px4(c))),
            ruby_size: Px::new(int(&f[8], WHAT)? as f32),
            emphasis,
            hanging_indent: boolean(&f[10])?,
            vertical: boolean(&f[11])?,
        })
    }

    /// The font of text whose style and effects name none: `System::font`, else the first
    /// loaded font.
    fn default_font(&self, assets: &AssetStore) -> Result<FontId, Trap> {
        self.system
            .map(|(font, _)| font)
            .or_else(|| assets.first_font())
            .ok_or_else(|| Trap::host("no font is loaded"))
    }

    /// Lays `text` out in `style`; `width` is the wrap width (the column length when vertical).
    pub fn layout(
        &mut self,
        assets: &AssetStore,
        text: &Text,
        style: &TextStyleSpec,
        width: Option<f32>,
    ) -> Result<TextLayout, Trap> {
        let mut h = DefaultHasher::new();
        hash_text(text, &mut h);
        let key = LayoutKey {
            text_hash: h.finish(),
            style: *style,
            width_q: width.map(|w| (w.max(0.0) * 64.0).round() as u32),
        };
        if let Some((cached, layout)) = self.cache.get(&key)
            && matches!(
                value_eq(&Value::Text(cached.clone()), &Value::Text(text.clone())),
                Ok(true)
            )
        {
            return Ok(layout.clone());
        }
        let default_font = self.default_font(assets)?;
        let mut flatten = Flatten {
            assets,
            style,
            default_font,
            spans: Vec::new(),
            rubies: Vec::new(),
            ruby: None,
        };
        flatten.walk(text, Effect::default())?;
        let paragraph = Paragraph {
            spans: flatten.spans,
            rubies: flatten.rubies,
            style: paragraph_style(style, width),
            fallback: default_font,
        };
        let (layout, metrics) = kagari_text::layout(assets.fonts(), &paragraph);
        let result = TextLayout {
            layout: Rc::new(layout),
            metrics,
        };
        if self.cache.len() >= CACHE_LIMIT {
            self.cache.clear();
        }
        self.cache.insert(key, (text.clone(), result.clone()));
        Ok(result)
    }

    /// A label of the screen: `style` `None` is `System::text`; a trap is logged and gives an
    /// empty layout.
    pub fn layout_label(
        &mut self,
        assets: &AssetStore,
        text: &Text,
        style: Option<&Value>,
        width: Option<u32>,
    ) -> Rc<Layout> {
        let style = match style {
            Some(v) => self.decode_style(assets, v),
            None => self.system_style().copied(),
        };
        let result =
            style.and_then(|style| self.layout(assets, text, &style, width.map(|w| w as f32)));
        match result {
            Ok(layout) => layout.layout,
            Err(trap) => {
                log::warn!("label not laid out: {trap}");
                Rc::new(Layout::default())
            }
        }
    }

    /// One plain span in the system font, else the first loaded font; `None` without any font.
    pub fn layout_plain(
        &mut self,
        assets: &AssetStore,
        text: &str,
        size_px: f32,
        color: [f32; 4],
        width: Option<f32>,
    ) -> Option<Rc<Layout>> {
        let font = self.default_font(assets).ok()?;
        let style = TextStyleSpec {
            font: Some(font),
            size: Px::new(size_px),
            color: px4(color),
            line_height: Px::new(1.5),
            letter_spacing: Px::new(0.0),
            align: TextAlign::Start,
            outline: None,
            shadow: None,
            ruby_size: Px::new(size_px / 2.0),
            emphasis: EmphasisStyle::Dots,
            hanging_indent: false,
            vertical: false,
        };
        let text = Text::Plain(Rc::from(text));
        match self.layout(assets, &text, &style, width) {
            Ok(layout) => Some(layout.layout),
            Err(trap) => {
                log::warn!("text not laid out: {trap}");
                None
            }
        }
    }
}

fn paragraph_style(style: &TextStyleSpec, width: Option<f32>) -> ParagraphStyle {
    ParagraphStyle {
        limit: width,
        line_height: style.line_height.get(),
        letter_spacing: style.letter_spacing.get(),
        align: match style.align {
            TextAlign::Start => kagari_text::Align::Start,
            TextAlign::Center => kagari_text::Align::Center,
            TextAlign::End => kagari_text::Align::End,
        },
        ruby_size_px: style.ruby_size.get(),
        hanging_indent: style.hanging_indent,
        vertical: style.vertical,
    }
}
