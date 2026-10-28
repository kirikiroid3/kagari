#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use crate::{
    Align, FontId, Fonts, Glyph, GlyphKey, Layout, Paragraph, ParagraphStyle, RubyText, Span,
    layout, units,
};

const FONT: &[u8] = include_bytes!("../tests/fonts/kagari-test-cjk.otf");
const SIZE: f32 = 20.0;
const RUBY: f32 = 10.0;

fn fonts() -> (Fonts, FontId) {
    let mut fonts = Fonts::new();
    let id = fonts.add(Arc::from(FONT)).unwrap();
    (fonts, id)
}

fn span(text: &str, font: FontId, ruby: Option<u32>) -> Span {
    Span {
        text: text.to_owned(),
        font,
        size_px: SIZE,
        color: [1.0; 4],
        bold: false,
        italic: false,
        outline: None,
        shadow: None,
        emphasis_dots: false,
        ruby,
    }
}

fn style() -> ParagraphStyle {
    ParagraphStyle {
        limit: None,
        line_height: 1.5,
        letter_spacing: 0.0,
        align: Align::Start,
        ruby_size_px: RUBY,
        hanging_indent: false,
        vertical: false,
    }
}

fn run(
    fonts: &Fonts,
    font: FontId,
    spans: Vec<Span>,
    rubies: Vec<RubyText>,
    style: ParagraphStyle,
) -> Layout {
    layout(
        fonts,
        &Paragraph {
            spans,
            rubies,
            style,
            fallback: font,
        },
    )
    .0
}

fn size_q(px: f32) -> u32 {
    (px * 64.0) as u32
}

fn base(l: &Layout) -> Vec<&Glyph> {
    l.glyphs
        .iter()
        .filter(|g| g.key.size_q == size_q(SIZE))
        .collect()
}

fn ruby(l: &Layout) -> Vec<&Glyph> {
    l.glyphs
        .iter()
        .filter(|g| g.key.size_q == size_q(RUBY))
        .collect()
}

fn assert_near(found: f32, expected: f32, what: &str) {
    assert!(
        (found - expected).abs() <= 0.5,
        "{what}: expected {expected} ±0.5, found {found}"
    );
}

#[test]
fn group_ruby_is_centred_over_a_wider_base() {
    let (fonts, font) = fonts();
    let l = run(
        &fonts,
        font,
        vec![span("漢字", font, Some(0))],
        vec![RubyText::Group("かんじ".into())],
        style(),
    );
    let bases = base(&l);
    let rubies = ruby(&l);
    assert_eq!(bases.len(), 2);
    assert_eq!(rubies.len(), 3);
    let x0 = bases[0].pos[0];
    let baseline = bases[0].pos[1];
    for (glyph, dx) in rubies.iter().zip([5.0, 15.0, 25.0]) {
        assert_near(glyph.pos[0], x0 + dx, "ruby x");
        assert!(
            glyph.pos[1] < baseline - 10.0,
            "ruby y {} vs baseline {baseline}",
            glyph.pos[1]
        );
        assert_eq!(glyph.unit, 0);
    }
}

#[test]
fn wide_ruby_pads_its_base() {
    let (fonts, font) = fonts();
    let l = run(
        &fonts,
        font,
        vec![span("日", font, Some(0))],
        vec![RubyText::Group("にちにち".into())],
        style(),
    );
    assert_near(base(&l)[0].pos[0], 10.0, "base x");
    assert_near(ruby(&l)[0].pos[0], 0.0, "first ruby x");
    assert_near(l.size[0], 40.0, "width");
}

#[test]
fn mono_ruby_sits_over_each_grapheme() {
    let (fonts, font) = fonts();
    let readings = RubyText::Mono(vec!["かん".into(), "たん".into()]);
    let l = run(
        &fonts,
        font,
        vec![span("簡単", font, Some(0))],
        vec![readings],
        style(),
    );
    let rubies = ruby(&l);
    let xs: Vec<f32> = rubies.iter().map(|g| g.pos[0]).collect();
    for (x, expected) in xs.iter().zip([0.0, 10.0, 20.0, 30.0]) {
        assert_near(*x, expected, "mono ruby x");
    }
    assert_eq!(rubies.len(), 4);
    let ruby_units: Vec<u32> = rubies.iter().map(|g| g.unit).collect();
    assert_eq!(ruby_units, [0, 0, 1, 1]);
}

#[test]
fn hanging_indent_indents_continuation_lines() {
    let (fonts, font) = fonts();
    for (hanging, expected) in [(true, 20.0), (false, 0.0)] {
        let style = ParagraphStyle {
            limit: Some(100.0),
            hanging_indent: hanging,
            ..style()
        };
        let l = run(
            &fonts,
            font,
            vec![span("「あいうえおかきくけ」", font, None)],
            vec![],
            style,
        );
        let glyphs = base(&l);
        let first_line = glyphs[0].pos[1];
        let line2: Vec<&&Glyph> = glyphs.iter().filter(|g| g.pos[1] > first_line).collect();
        let start = line2[0];
        assert_near(start.pos[0], expected, "line 2 start");
        assert_eq!(start.unit, 5, "line 2 starts with お");
    }
}

#[test]
fn kinsoku_pushes_the_comma_off_the_line_start() {
    let (fonts, font) = fonts();
    let style = ParagraphStyle {
        limit: Some(100.0),
        ..style()
    };
    let l = run(
        &fonts,
        font,
        vec![span("あいうえお、かきく", font, None)],
        vec![],
        style,
    );
    let glyphs = base(&l);
    let first_line = glyphs[0].pos[1];
    assert_eq!(glyphs.iter().filter(|g| g.pos[1] == first_line).count(), 4);
    let line2: Vec<&&Glyph> = glyphs.iter().filter(|g| g.pos[1] > first_line).collect();
    assert_eq!(line2[0].unit, 4, "line 2 starts with お");
    assert_near(line2[0].pos[0], 0.0, "お x");
    assert_eq!(line2[1].unit, 5, "、 follows お");
}

#[test]
fn vertical_columns_run_right_to_left() {
    let (fonts, font) = fonts();
    let style = ParagraphStyle {
        limit: Some(60.0),
        vertical: true,
        ..style()
    };
    let l = run(
        &fonts,
        font,
        vec![span("あいうえお", font, None)],
        vec![],
        style,
    );
    let glyphs = base(&l);
    // An upright glyph's origin is its left edge: the column centre is half an advance right.
    assert_near(glyphs[0].pos[0] + SIZE / 2.0, 45.0, "あ column centre");
    assert_eq!(glyphs[3].unit, 3);
    assert_near(glyphs[3].pos[0] + SIZE / 2.0, 15.0, "え column centre");
    assert_near(l.size[0], 60.0, "width");
    assert_near(l.size[1], 60.0, "height");
    assert!(glyphs.iter().all(|g| !g.key.rotate90));
    let mixed = run(&fonts, font, vec![span("aあ", font, None)], vec![], style);
    assert!(mixed.glyphs[0].key.rotate90, "`a` is turned");
    assert!(!mixed.glyphs[1].key.rotate90, "`あ` is upright");
}

#[test]
fn units_count_graphemes_without_newlines() {
    assert_eq!(units("か\u{3099}あ"), 2);
    assert_eq!(units("あ\nい"), 2);
    let (fonts, font) = fonts();
    for text in ["か\u{3099}あ", "あ\nい"] {
        let l = run(&fonts, font, vec![span(text, font, None)], vec![], style());
        assert_eq!(l.units, 2, "{text:?}");
    }
}

#[test]
fn rasterized_h_sits_on_the_baseline() {
    let (fonts, font) = fonts();
    let l = run(&fonts, font, vec![span("H", font, None)], vec![], style());
    let glyph = l.glyphs[0].key;
    let key = GlyphKey {
        size_q: size_q(32.0),
        ..glyph
    };
    let mask = fonts.rasterize(key, 0.0).unwrap();
    assert!(mask.top > 0, "top {}", mask.top);
    let bottom = mask
        .top
        .checked_sub(i32::try_from(mask.height).unwrap())
        .unwrap();
    assert!((-1..=1).contains(&bottom), "bottom row at {bottom}");
    let area = mask.width.checked_mul(mask.height).unwrap();
    assert_eq!(mask.alpha.len(), usize::try_from(area).unwrap());
    assert!(mask.alpha.contains(&255));
}
