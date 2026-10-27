//! The text shown for an item: its header without body, and its `///` doc comment.

use kagari_syntax::{SyntaxElement, SyntaxKind, SyntaxNode};
use rowan::NodeOrToken;

/// The item's text up to its body, value or `;`, attributes and comments left out, with every
/// whitespace run collapsed to one space: `pub fn helper(x: u32) -> u32`.
pub(crate) fn item_header(node: &SyntaxNode) -> String {
    let stops_at_eq = matches!(node.kind(), SyntaxKind::CONST | SyntaxKind::STATIC);
    let mut text = String::new();
    for child in node.children_with_tokens() {
        let kind = child.kind();
        match kind {
            SyntaxKind::ATTR | SyntaxKind::LINE_COMMENT | SyntaxKind::BLOCK_COMMENT => continue,
            SyntaxKind::BLOCK_EXPR
            | SyntaxKind::RECORD_FIELD_LIST
            | SyntaxKind::VARIANT_LIST
            | SyntaxKind::ASSOC_ITEM_LIST
            | SyntaxKind::ITEM_LIST
            | SyntaxKind::SEMICOLON => break,
            SyntaxKind::EQ if stops_at_eq => break,
            _ => push_element(&mut text, &child),
        }
    }
    collapse_whitespace(&text)
}

/// The `///` comment lines right before `node` (a blank line ends them), without the `///`
/// and one following space, joined with `\n`. Empty when there are none.
pub(crate) fn doc_comment(node: &SyntaxNode) -> String {
    let mut lines: Vec<String> = Vec::new();
    let mut current = node.prev_sibling_or_token();
    while let Some(element) = current {
        let NodeOrToken::Token(token) = &element else {
            break;
        };
        match token.kind() {
            SyntaxKind::WHITESPACE if token.text().matches('\n').count() <= 1 => {}
            SyntaxKind::LINE_COMMENT
                if token.text().starts_with("///") && !token.text().starts_with("////") =>
            {
                let line = token.text().trim_start_matches("///");
                let line = line.strip_prefix(' ').unwrap_or(line);
                lines.push(line.trim_end().to_owned());
            }
            _ => break,
        }
        current = element.prev_sibling_or_token();
    }
    lines.reverse();
    lines.join("\n")
}

/// `text` with every whitespace run replaced by one space, trimmed.
pub(crate) fn collapse_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn push_element(text: &mut String, element: &SyntaxElement) {
    match element {
        NodeOrToken::Node(node) => text.push_str(&node.text().to_string()),
        NodeOrToken::Token(token) => text.push_str(token.text()),
    }
}
