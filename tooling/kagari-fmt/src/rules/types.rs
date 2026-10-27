//! Types not covered by the generic spacing rules in `mod.rs`.

use kagari_syntax::{SyntaxKind, SyntaxNode};

use super::{Formatter, Space};
use crate::doc::Doc;

use SyntaxKind::*;

impl Formatter {
    /// A `FN_PTR_TYPE` or `CLOSURE_TYPE`: `[const |pushdown |scene ]fn(A) -> B` and
    /// `[pushdown ]Fn(A) -> B`.
    pub(super) fn fn_type(&self, node: &SyntaxNode) -> Doc {
        self.spaced_kinds(node, |kind| match kind {
            CONST_KW | PUSHDOWN_KW | SCENE_KW => Space::After,
            RET_TYPE => Space::Before,
            _ => Space::No,
        })
    }
}
