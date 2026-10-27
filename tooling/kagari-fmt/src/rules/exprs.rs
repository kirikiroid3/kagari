//! Expressions other than chains, binary operators and blocks.

use kagari_syntax::{SyntaxKind, SyntaxNode};

use super::{Formatter, Space, children};
use crate::doc::Doc;

use SyntaxKind::*;

impl Formatter {
    /// `[x; n]` compact, otherwise a delimited list.
    pub(super) fn array(&self, node: &SyntaxNode) -> Doc {
        if children(node).iter().any(|e| e.kind() == SEMICOLON) {
            return self.spaced_kinds(node, |kind| match kind {
                SEMICOLON => Space::After,
                _ => Space::No,
            });
        }
        self.delimited_list(node, L_BRACK, false)
    }

    /// `name: expr` with the assignment layout, or the shorthand `name`.
    pub(super) fn record_expr_field(&self, node: &SyntaxNode) -> Doc {
        self.with_assign(node, COLON, false, |element| self.el(element))
    }

    /// `break`/`continue`/`return`, then ` 'label` and ` expr` if present.
    pub(super) fn jump(&self, node: &SyntaxNode) -> Doc {
        self.spaced_kinds(node, |kind| match kind {
            BREAK_KW | CONTINUE_KW | RETURN_KW => Space::No,
            _ => Space::Before,
        })
    }
}
