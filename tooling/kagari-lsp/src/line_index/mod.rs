//! Byte offsets ↔ LSP positions (line, UTF-16 column).

use kagari_syntax::{TextRange, TextSize};
use lsp_types::{Position, Range};

/// The start offset of every line of a text.
#[derive(Clone, Debug)]
pub struct LineIndex {
    line_starts: Vec<TextSize>,
}

impl LineIndex {
    pub fn new(text: &str) -> Self {
        let mut line_starts = vec![TextSize::new(0)];
        line_starts.extend(
            text.match_indices('\n')
                .filter_map(|(at, _)| at.checked_add(1))
                .filter_map(|start| TextSize::try_from(start).ok()),
        );
        LineIndex { line_starts }
    }

    /// The position of `offset` in `text`, the text this index was built from. Offsets past the
    /// end are clamped to the end; offsets inside a character to its start.
    pub fn position(&self, text: &str, offset: TextSize) -> Position {
        let mut offset = usize::from(offset).min(text.len());
        while !text.is_char_boundary(offset) {
            offset = offset.saturating_sub(1);
        }
        let line = self
            .line_starts
            .partition_point(|&start| usize::from(start) <= offset)
            .saturating_sub(1);
        let start = self
            .line_starts
            .get(line)
            .map_or(0, |&start| usize::from(start));
        let column = text
            .get(start..offset)
            .map_or(0, |part| part.encode_utf16().count());
        Position {
            line: u32::try_from(line).unwrap_or(u32::MAX),
            character: u32::try_from(column).unwrap_or(u32::MAX),
        }
    }

    /// The offset of `pos` in `text`. A column past the end of its line means the line's end;
    /// a line past the end of the text has no offset.
    pub fn offset(&self, text: &str, pos: Position) -> Option<TextSize> {
        let line = usize::try_from(pos.line).ok()?;
        let start = usize::from(*self.line_starts.get(line)?);
        let end = self
            .line_starts
            .get(line.checked_add(1)?)
            .map_or(text.len(), |&next| usize::from(next));
        let content = text.get(start..end)?;
        let content = content.strip_suffix('\n').unwrap_or(content);
        let content = content.strip_suffix('\r').unwrap_or(content);
        let mut units = 0u32;
        let mut at = content.len();
        for (index, ch) in content.char_indices() {
            if units >= pos.character {
                at = index;
                break;
            }
            units = units.saturating_add(u32::try_from(ch.len_utf16()).unwrap_or(2));
        }
        TextSize::try_from(start.checked_add(at)?).ok()
    }

    pub fn range(&self, text: &str, range: TextRange) -> Range {
        Range {
            start: self.position(text, range.start()),
            end: self.position(text, range.end()),
        }
    }
}

#[cfg(test)]
mod tests;
