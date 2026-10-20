//! Decoding of character data and attribute values, with a map back to source offsets.

use std::ops::Range;

use crate::script::lex::reference_body_len;
use crate::{TextRange, TextSize};

use super::Content;

/// Text with entity and character references decoded, CDATA unwrapped and comments removed.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct DecodedText {
    pub text: String,
    pub map: SourceMap,
}

/// Maps offsets in a [`DecodedText`] back to absolute offsets in the source file.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SourceMap {
    segments: Vec<Segment>,
    /// Where decoding started; the image of every offset when there are no segments.
    origin: TextSize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct Segment {
    decoded_start: TextSize,
    source_start: TextSize,
    decoded_len: TextSize,
    source_len: TextSize,
    /// Copied 1:1 from the source (as opposed to a decoded reference).
    verbatim: bool,
}

impl Segment {
    fn decoded_end(&self) -> TextSize {
        add(self.decoded_start, self.decoded_len)
    }

    fn source_end(&self) -> TextSize {
        add(self.source_start, self.source_len)
    }
}

impl SourceMap {
    /// The source offset of the decoded offset `decoded`. An offset inside a decoded reference
    /// maps to the start of the reference; offsets past the end map to the end of the last
    /// segment.
    pub fn to_source(&self, decoded: TextSize) -> TextSize {
        let index = self
            .segments
            .partition_point(|segment| segment.decoded_end() <= decoded);
        match self.segments.get(index) {
            Some(segment) if segment.verbatim => {
                add(segment.source_start, sub(decoded, segment.decoded_start))
            }
            Some(segment) => segment.source_start,
            None => self.end(),
        }
    }

    /// The source range covering the decoded range `range`. A range touching part of a decoded
    /// reference covers the whole reference.
    pub fn range_to_source(&self, range: TextRange) -> TextRange {
        let start = self.to_source(range.start());
        if range.is_empty() {
            return TextRange::empty(start);
        }
        let end = range.end();
        let index = self
            .segments
            .partition_point(|segment| segment.decoded_end() < end);
        let end = match self.segments.get(index) {
            Some(segment) if segment.verbatim => {
                add(segment.source_start, sub(end, segment.decoded_start))
            }
            Some(segment) => segment.source_end(),
            None => self.end(),
        };
        TextRange::new(start, end.max(start))
    }

    fn end(&self) -> TextSize {
        self.segments
            .last()
            .map_or(self.origin, Segment::source_end)
    }
}

/// Decodes element content: text verbatim, references decoded, CDATA unwrapped. Comments and
/// child elements contribute nothing. Unknown references are kept verbatim (the parser already
/// reported them).
pub fn decode_content(items: impl Iterator<Item = Content>) -> DecodedText {
    let mut builder = Builder::default();
    for item in items {
        match item {
            Content::Text(token) => {
                builder.locate(token.text_range().start());
                builder.verbatim(token.text(), token.text_range().start());
            }
            Content::EntityRef(token) => {
                builder.locate(token.text_range().start());
                let raw = token.text();
                let decoded = raw
                    .strip_prefix('&')
                    .and_then(|rest| rest.strip_suffix(';'))
                    .and_then(|body| decode_reference(body).ok());
                match decoded {
                    Some(c) => builder.replaced(c, token.text_range()),
                    None => builder.verbatim(raw, token.text_range().start()),
                }
            }
            Content::CData(token) => {
                let start = token.text_range().start();
                builder.locate(start);
                let raw = token.text();
                const OPEN: &str = "<![CDATA[";
                let (inner, inner_start) = match raw.strip_prefix(OPEN) {
                    Some(inner) => (inner, add(start, TextSize::of(OPEN))),
                    None => (raw, start),
                };
                let inner = inner.strip_suffix("]]>").unwrap_or(inner);
                builder.verbatim(inner, inner_start);
            }
            Content::Comment(_) | Content::Element(_) => {}
        }
    }
    builder.finish()
}

/// Decodes an attribute value token (quotes included) that starts at `offset` in the file.
pub(crate) fn decode_attr_value(raw: &str, offset: TextSize) -> DecodedText {
    let mut builder = Builder {
        origin: Some(offset),
        ..Builder::default()
    };
    let Some(quote) = raw.chars().next().filter(|c| matches!(c, '"' | '\'')) else {
        return builder.finish();
    };
    let inner = raw.get(1..).unwrap_or("");
    let inner = match inner.strip_suffix(quote) {
        Some(stripped) => stripped,
        None => inner,
    };
    let inner_start = add(offset, TextSize::of(quote));
    builder.origin = Some(inner_start);
    for piece in split_references(inner) {
        match piece {
            Piece::Text(range) => {
                builder.verbatim(slice(inner, &range), add(inner_start, size(range.start)));
            }
            Piece::BareAmp(at) => builder.verbatim("&", add(inner_start, size(at))),
            Piece::Ref { range, decoded } => {
                let source = TextRange::new(
                    add(inner_start, size(range.start)),
                    add(inner_start, size(range.end)),
                );
                match decoded {
                    Ok(c) => builder.replaced(c, source),
                    Err(_) => builder.verbatim(slice(inner, &range), source.start()),
                }
            }
        }
    }
    builder.finish()
}

/// Why a reference body (the text between `&` and `;`) does not decode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ReferenceError {
    UnknownEntity,
    InvalidCharRef,
}

impl ReferenceError {
    pub(crate) fn message(self, raw: &str) -> String {
        match self {
            ReferenceError::UnknownEntity => format!("unknown entity `{raw}`"),
            ReferenceError::InvalidCharRef => format!("invalid character reference `{raw}`"),
        }
    }
}

/// Decodes the body of a reference: one of the five predefined entities, `#N` or `#xN`.
pub(crate) fn decode_reference(body: &str) -> Result<char, ReferenceError> {
    if let Some(number) = body.strip_prefix('#') {
        let (digits, radix) = match number.strip_prefix('x') {
            Some(hex) => (hex, 16),
            None => (number, 10),
        };
        if digits.is_empty() {
            return Err(ReferenceError::InvalidCharRef);
        }
        let mut value: u32 = 0;
        for c in digits.chars() {
            let digit = c.to_digit(radix).ok_or(ReferenceError::InvalidCharRef)?;
            value = value
                .checked_mul(radix)
                .and_then(|v| v.checked_add(digit))
                .ok_or(ReferenceError::InvalidCharRef)?;
        }
        return char::from_u32(value)
            .filter(|&c| is_xml_char(c))
            .ok_or(ReferenceError::InvalidCharRef);
    }
    match body {
        "lt" => Ok('<'),
        "gt" => Ok('>'),
        "amp" => Ok('&'),
        "apos" => Ok('\''),
        "quot" => Ok('"'),
        _ => Err(ReferenceError::UnknownEntity),
    }
}

/// The XML `Char` production.
fn is_xml_char(c: char) -> bool {
    matches!(c, '\t' | '\n' | '\r' | '\u{20}'..='\u{D7FF}' | '\u{E000}'..='\u{FFFD}' | '\u{10000}'..)
}

/// A piece of an attribute value, by byte range within the value (quotes excluded).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Piece {
    Text(Range<usize>),
    /// A `&` that does not start a `;`-terminated reference.
    BareAmp(usize),
    /// `&…;`; `range` covers the whole reference.
    Ref {
        range: Range<usize>,
        decoded: Result<char, ReferenceError>,
    },
}

/// Splits attribute value text into verbatim runs and references.
pub(crate) fn split_references(text: &str) -> Vec<Piece> {
    let mut pieces = Vec::new();
    let mut run_start = 0usize;
    let mut pos = 0usize;
    while let Some(found) = text.get(pos..).and_then(|rest| rest.find('&')) {
        let amp = pos.saturating_add(found);
        if run_start < amp {
            pieces.push(Piece::Text(run_start..amp));
        }
        let after = amp.saturating_add(1);
        match reference_body_len(text.get(after..).unwrap_or("")) {
            Some(body_len) => {
                let body_end = after.saturating_add(body_len);
                let body = text.get(after..body_end).unwrap_or("");
                let end = body_end.saturating_add(1);
                pieces.push(Piece::Ref {
                    range: amp..end,
                    decoded: decode_reference(body),
                });
                pos = end;
            }
            None => {
                pieces.push(Piece::BareAmp(amp));
                pos = after;
            }
        }
        run_start = pos;
    }
    if run_start < text.len() {
        pieces.push(Piece::Text(run_start..text.len()));
    }
    pieces
}

#[derive(Default)]
struct Builder {
    text: String,
    segments: Vec<Segment>,
    origin: Option<TextSize>,
}

impl Builder {
    /// Records the first source position seen, for maps without segments.
    fn locate(&mut self, source: TextSize) {
        self.origin.get_or_insert(source);
    }

    fn verbatim(&mut self, text: &str, source_start: TextSize) {
        if text.is_empty() {
            return;
        }
        let len = TextSize::of(text);
        let decoded_start = TextSize::of(self.text.as_str());
        self.text.push_str(text);
        if let Some(last) = self.segments.last_mut()
            && last.verbatim
            && last.source_end() == source_start
        {
            last.decoded_len = add(last.decoded_len, len);
            last.source_len = add(last.source_len, len);
            return;
        }
        self.segments.push(Segment {
            decoded_start,
            source_start,
            decoded_len: len,
            source_len: len,
            verbatim: true,
        });
    }

    fn replaced(&mut self, c: char, source: TextRange) {
        let decoded_start = TextSize::of(self.text.as_str());
        self.text.push(c);
        self.segments.push(Segment {
            decoded_start,
            source_start: source.start(),
            decoded_len: TextSize::of(c),
            source_len: source.len(),
            verbatim: false,
        });
    }

    fn finish(self) -> DecodedText {
        DecodedText {
            text: self.text,
            map: SourceMap {
                segments: self.segments,
                origin: self.origin.unwrap_or_default(),
            },
        }
    }
}

fn add(a: TextSize, b: TextSize) -> TextSize {
    a.checked_add(b).unwrap_or(TextSize::new(u32::MAX))
}

fn sub(a: TextSize, b: TextSize) -> TextSize {
    a.checked_sub(b).unwrap_or_default()
}

fn size(len: usize) -> TextSize {
    TextSize::new(u32::try_from(len).unwrap_or(u32::MAX))
}

fn slice<'a>(text: &'a str, range: &Range<usize>) -> &'a str {
    text.get(range.clone()).unwrap_or("")
}

#[cfg(test)]
mod tests {
    use super::{decode_attr_value, decode_reference};
    use crate::{TextRange, TextSize};

    #[test]
    fn references() {
        assert_eq!(decode_reference("lt"), Ok('<'));
        assert_eq!(decode_reference("#12354"), Ok('\u{3042}'));
        assert_eq!(decode_reference("#x3042"), Ok('\u{3042}'));
        assert!(decode_reference("nbsp").is_err());
        assert!(decode_reference("#xD800").is_err());
        assert!(decode_reference("#0").is_err());
        assert!(decode_reference("#99999999999").is_err());
    }

    #[test]
    fn attr_value_map() {
        // Value token at offset 10: `"a&lt;b"`.
        let decoded = decode_attr_value("\"a&lt;b\"", TextSize::new(10));
        assert_eq!(decoded.text, "a<b");
        let map = &decoded.map;
        assert_eq!(map.to_source(TextSize::new(0)), TextSize::new(11));
        assert_eq!(map.to_source(TextSize::new(1)), TextSize::new(12));
        assert_eq!(map.to_source(TextSize::new(2)), TextSize::new(16));
        assert_eq!(map.to_source(TextSize::new(3)), TextSize::new(17));
        assert_eq!(map.to_source(TextSize::new(9)), TextSize::new(17));
        assert_eq!(
            map.range_to_source(TextRange::new(TextSize::new(1), TextSize::new(2))),
            TextRange::new(TextSize::new(12), TextSize::new(16))
        );
        let empty = decode_attr_value("''", TextSize::new(4));
        assert_eq!(empty.text, "");
        assert_eq!(empty.map.to_source(TextSize::new(0)), TextSize::new(5));
        let unterminated = decode_attr_value("\"ab", TextSize::new(0));
        assert_eq!(unterminated.text, "ab");
    }
}
