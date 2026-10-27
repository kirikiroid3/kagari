//! Patterns not covered by the generic spacing rules in `mod.rs`.

use kagari_syntax::{SyntaxKind, SyntaxNode};

use super::lists::{Trailing, list_parts};
use super::{Formatter, children};
use crate::doc::{Doc, concat, group, indent, text};

use SyntaxKind::*;

impl Formatter {
    /// A record pattern's fields: `{ a, b: c, .. }`, no trailing comma after `..`.
    pub(super) fn record_pat_field_list(&self, node: &SyntaxNode) -> Doc {
        let Some(parts) = list_parts(node, L_CURLY) else {
            return self.verbatim(node);
        };
        let trailing = match parts.elems.last() {
            Some((last, _)) if last.kind() == REST_PAT => Trailing::Never,
            _ => Trailing::IfBreak,
        };
        self.delimited_parts(&parts, true, trailing)
    }

    /// `G(p1, I(L "|" p2, …))`; a leading `|` is kept.
    pub(super) fn or_pat(&self, node: &SyntaxNode) -> Doc {
        let mut head = Vec::new();
        let mut rest = Vec::new();
        let mut seen_first = false;
        for element in children(node) {
            let is_pipe = element.kind() == PIPE;
            match (seen_first, is_pipe) {
                (false, true) => {
                    head.push(self.el(&element));
                    head.push(text(" "));
                }
                (false, false) => {
                    head.push(self.el(&element));
                    seen_first = true;
                }
                (true, true) => {
                    rest.push(Doc::Line);
                    rest.push(self.el(&element));
                    rest.push(text(" "));
                }
                (true, false) => rest.push(self.el(&element)),
            }
        }
        head.push(indent(concat(rest)));
        group(concat(head))
    }
}
