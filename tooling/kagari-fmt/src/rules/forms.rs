//! Forms (`name!(…)`, `name![…]`, `name!{…}`) and text (`t![…]`).

use kagari_syntax::{SyntaxKind, SyntaxNode};

use super::lists::list_parts;
use super::{Formatter, children};
use crate::doc::{Doc, concat, group, indent};

use SyntaxKind::*;

impl Formatter {
    /// `name!` + a delimited list with last-element hug; a `{}` form with arms is vertical.
    pub(super) fn form_expr(&self, node: &SyntaxNode) -> Doc {
        let open = node
            .children_with_tokens()
            .filter_map(|e| e.into_token())
            .find(|t| matches!(t.kind(), L_PAREN | L_BRACK | L_CURLY))
            .map(|t| t.kind());
        let Some(parts) = open.and_then(|open| list_parts(node, open)) else {
            return self.verbatim(node);
        };
        let arms = parts.elems.iter().any(|(n, _)| n.kind() == FORM_ARM);
        if parts.open.kind() == L_CURLY && arms {
            let mut docs: Vec<Doc> = parts.before.iter().map(|e| self.el(e)).collect();
            docs.push(self.vertical(&parts.open, self.velems(&parts), &parts.close, true));
            return concat(docs);
        }
        self.hug_parts(&parts)
    }

    /// `key` + the assignment layout with `" ="`.
    pub(super) fn form_attr(&self, node: &SyntaxNode) -> Doc {
        self.with_assign(node, EQ, true, |element| self.el(element))
    }

    /// `t![` + `G(I(S, item, L, item, …), S)` + `]`.
    pub(super) fn text_form(&self, node: &SyntaxNode) -> Doc {
        let Some(parts) = list_parts_juxtaposed(node) else {
            return self.verbatim(node);
        };
        let (before, open, items, close) = parts;
        let mut docs: Vec<Doc> = before.iter().map(|e| self.el(e)).collect();
        if items.is_empty() {
            docs.push(self.empty_list(&open, &close));
            return concat(docs);
        }
        let mut inner = vec![Doc::SoftLine];
        for (i, item) in items.iter().enumerate() {
            if i > 0 {
                inner.push(Doc::Line);
            }
            inner.push(self.node(item));
        }
        inner.push(self.dangling(&close, true));
        docs.push(self.tok(&open));
        docs.push(group(concat(vec![indent(concat(inner)), Doc::SoftLine])));
        docs.push(self.tok_bare(&close));
        concat(docs)
    }
}

type Juxtaposed = (
    Vec<kagari_syntax::SyntaxElement>,
    kagari_syntax::SyntaxToken,
    Vec<SyntaxNode>,
    kagari_syntax::SyntaxToken,
);

/// `t` `!` `[` items `]`: the items are nodes without separators.
fn list_parts_juxtaposed(node: &SyntaxNode) -> Option<Juxtaposed> {
    let elements = children(node);
    let open_index = elements.iter().position(|e| e.kind() == L_BRACK)?;
    let (before, rest) = elements.split_at(open_index);
    let (open, rest) = rest.split_first()?;
    let (close, items) = rest.split_last()?;
    let items: Option<Vec<SyntaxNode>> = items.iter().map(|e| e.as_node().cloned()).collect();
    Some((
        before.to_vec(),
        open.as_token()?.clone(),
        items?,
        close.as_token()?.clone(),
    ))
}
