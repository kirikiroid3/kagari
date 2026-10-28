//! Line breaking: UAX #14 opportunities, kinsoku rules and the greedy fill.

use std::ops::Range;

use unicode_linebreak::{BreakOpportunity, linebreaks};

/// Chars that must not start a line.
const NO_START: &str = "、。，．・：；？！‼⁇⁈⁉ー‐゠–〜～）〕］｝〉》」』】〙〗｠’”ヽヾゝゞ々〻ぁぃぅぇぉっゃゅょゎゕゖァィゥェォッャュョヮヵヶ)]},.:;?!%％";
/// Chars that must not end a line.
const NO_END: &str = "「『（〔［｛〈《【〘〖｟‘“([{";
/// Line openers that hang their continuation lines.
const HANGING_OPENERS: [char; 3] = ['「', '『', '（'];
/// Float slack of the overflow test.
const EPSILON: f32 = 1e-3;

/// What breaking needs from one grapheme.
pub(crate) struct BreakItem<'a> {
    pub text: &'a str,
    /// Byte offset in the paragraph text.
    pub start: usize,
    pub newline: bool,
    /// Extent along the line: ruby padding, advance and letter spacing.
    pub extent: f32,
    /// Advance plus letter spacing (the hanging indent of an opener).
    pub advance: f32,
    /// The ruby base the grapheme belongs to.
    pub ruby: Option<usize>,
}

/// One laid-out line: graphemes `range` (no newline grapheme), indented by `indent`.
#[derive(Clone, PartialEq, Debug)]
pub(crate) struct Line {
    pub range: Range<usize>,
    pub indent: f32,
    /// The newline grapheme that ends the line, if any.
    pub newline: Option<usize>,
}

pub(crate) fn is_no_start(grapheme: &str) -> bool {
    grapheme
        .chars()
        .next()
        .is_some_and(|ch| NO_START.contains(ch) || ('\u{31F0}'..='\u{31FF}').contains(&ch))
}

pub(crate) fn is_no_end(grapheme: &str) -> bool {
    grapheme
        .chars()
        .next_back()
        .is_some_and(|ch| NO_END.contains(ch))
}

/// U+0020 and U+3000, which never cause an overflow at a line end.
pub(crate) fn is_space(grapheme: &str) -> bool {
    grapheme == " " || grapheme == "\u{3000}"
}

/// `allowed[i]`: a line may break before grapheme `i`.
fn opportunities(text: &str, items: &[BreakItem<'_>]) -> Vec<bool> {
    let mut allowed = vec![false; items.len()];
    for (byte, kind) in linebreaks(text) {
        if kind != BreakOpportunity::Allowed && kind != BreakOpportunity::Mandatory {
            continue;
        }
        let index = items.partition_point(|item| item.start < byte);
        if items.get(index).is_some_and(|item| item.start == byte)
            && let Some(slot) = allowed.get_mut(index)
        {
            *slot = true;
        }
    }
    for (i, slot) in allowed.iter_mut().enumerate() {
        let Some(prev) = i.checked_sub(1).and_then(|p| items.get(p)) else {
            *slot = false;
            continue;
        };
        let Some(item) = items.get(i) else { continue };
        if is_no_start(item.text)
            || is_no_end(prev.text)
            || (item.ruby.is_some() && item.ruby == prev.ruby)
        {
            *slot = false;
        }
    }
    allowed
}

/// Splits `items` into lines no longer than `limit` (when given).
pub(crate) fn break_lines(
    text: &str,
    items: &[BreakItem<'_>],
    limit: Option<f32>,
    hanging_indent: bool,
) -> Vec<Line> {
    let allowed = opportunities(text, items);
    let mut prefix = Vec::with_capacity(items.len().saturating_add(1));
    let mut sum = 0.0f32;
    prefix.push(sum);
    for item in items {
        sum += item.extent;
        prefix.push(sum);
    }
    let width = |a: usize, b: usize| -> f32 {
        prefix.get(b).copied().unwrap_or(sum) - prefix.get(a).copied().unwrap_or(sum)
    };
    let mut lines = Vec::new();
    let mut hard_start = 0usize;
    while hard_start <= items.len() {
        let newline = items
            .iter()
            .skip(hard_start)
            .position(|i| i.newline)
            .map(|n| hard_start.saturating_add(n));
        let hard_end = newline.unwrap_or(items.len());
        let hang = match items.get(hard_start) {
            Some(first)
                if hanging_indent
                    && hard_start < hard_end
                    && first.text.starts_with(HANGING_OPENERS) =>
            {
                first.advance
            }
            _ => 0.0,
        };
        let mut line_start = hard_start;
        let mut i = hard_start;
        while i < hard_end {
            let indent = if line_start == hard_start { 0.0 } else { hang };
            let next = i.saturating_add(1);
            let overflows = limit.is_some_and(|limit| {
                i > line_start
                    && !items.get(i).is_some_and(|item| is_space(item.text))
                    && indent + width(line_start, next) > limit + EPSILON
            });
            if overflows {
                let at = choose_break(items, &allowed, line_start, i);
                lines.push(Line {
                    range: line_start..at,
                    indent,
                    newline: None,
                });
                line_start = at;
                continue;
            }
            i = next;
        }
        let indent = if line_start == hard_start { 0.0 } else { hang };
        lines.push(Line {
            range: line_start..hard_end,
            indent,
            newline,
        });
        match newline {
            Some(n) => hard_start = n.saturating_add(1),
            None => break,
        }
    }
    lines
}

/// The break before the grapheme that overflows (`i`): the last allowed opportunity in
/// `(start, i]`; with none, before `i`, or before `i − 1` when `i` must not start a line.
fn choose_break(items: &[BreakItem<'_>], allowed: &[bool], start: usize, i: usize) -> usize {
    let after = start.saturating_add(1);
    if let Some(at) = (after..=i)
        .rev()
        .find(|k| allowed.get(*k).copied().unwrap_or(false))
    {
        return at;
    }
    let pushed_out = i.checked_sub(1).filter(|prev| *prev > start);
    match pushed_out {
        Some(prev) if items.get(i).is_some_and(|item| is_no_start(item.text)) => prev,
        _ => i,
    }
}
