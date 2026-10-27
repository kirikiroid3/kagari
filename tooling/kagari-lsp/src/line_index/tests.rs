#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use kagari_syntax::{TextRange, TextSize};
use lsp_types::{Position, Range};

use super::LineIndex;

fn pos(line: u32, character: u32) -> Position {
    Position { line, character }
}

#[test]
fn ascii_lines() {
    let text = "fn a() {}\nlet b = 1;\n";
    let index = LineIndex::new(text);
    assert_eq!(index.position(text, TextSize::new(0)), pos(0, 0));
    assert_eq!(index.position(text, TextSize::new(3)), pos(0, 3));
    assert_eq!(index.position(text, TextSize::new(10)), pos(1, 0));
    assert_eq!(index.position(text, TextSize::new(14)), pos(1, 4));
    assert_eq!(index.offset(text, pos(1, 4)), Some(TextSize::new(14)));
    assert_eq!(index.offset(text, pos(0, 0)), Some(TextSize::new(0)));
    assert_eq!(
        index.range(text, TextRange::new(TextSize::new(3), TextSize::new(14))),
        Range {
            start: pos(0, 3),
            end: pos(1, 4)
        }
    );
}

#[test]
fn utf16_columns() {
    // `あ` is 3 bytes and 1 UTF-16 unit; `𝄞` is 4 bytes and 2 units.
    let text = "x\n\"あ𝄞b\"\n";
    let index = LineIndex::new(text);
    let (a, clef, b) = (3, 6, 10); // byte offsets of `あ`, `𝄞` and `b`
    assert_eq!(index.position(text, TextSize::new(a)), pos(1, 1));
    assert_eq!(index.position(text, TextSize::new(clef)), pos(1, 2));
    assert_eq!(index.position(text, TextSize::new(b)), pos(1, 4));
    assert_eq!(index.offset(text, pos(1, 1)), Some(TextSize::new(a)));
    assert_eq!(index.offset(text, pos(1, 2)), Some(TextSize::new(clef)));
    assert_eq!(index.offset(text, pos(1, 4)), Some(TextSize::new(b)));
    // A column past the line's end means the end of the line, before its newline.
    assert_eq!(index.offset(text, pos(1, 99)), Some(TextSize::new(12)));
}

#[test]
fn end_of_file() {
    let text = "ab\ncd";
    let index = LineIndex::new(text);
    let end = TextSize::new(5);
    assert_eq!(index.position(text, end), pos(1, 2));
    assert_eq!(index.offset(text, pos(1, 2)), Some(end));
    assert_eq!(index.position(text, TextSize::new(99)), pos(1, 2));
    assert_eq!(index.offset(text, pos(2, 0)), None);

    let text = "ab\n";
    let index = LineIndex::new(text);
    assert_eq!(index.position(text, TextSize::new(3)), pos(1, 0));
    assert_eq!(index.offset(text, pos(1, 0)), Some(TextSize::new(3)));
}
