//! Delimited, vertical and hugging lists, and the assignment layout.

use kagari_syntax::{SyntaxElement, SyntaxKind, SyntaxNode, SyntaxToken};

use super::{Formatter, children};
use crate::doc::{
    Doc, broken_group, concat, conditional_group, flatten, group, has_forced_break, if_break,
    indent, text,
};

use SyntaxKind::*;

/// Whether a delimited list ends with a comma.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Trailing {
    /// Only when the list breaks.
    IfBreak,
    Never,
    /// Always (a 1-tuple).
    Always,
}

/// A list node split at its delimiters.
pub(super) struct ListParts {
    /// Children before the opening delimiter (a path, a form name, a turbofish `::`).
    pub(super) before: Vec<SyntaxElement>,
    pub(super) open: SyntaxToken,
    /// Each element with the comma after it.
    pub(super) elems: Vec<(SyntaxNode, Option<SyntaxToken>)>,
    pub(super) close: SyntaxToken,
}

/// Splits `node` at its first `open` token. Every element must be a node and the only tokens
/// after `open` must be commas and one closing delimiter.
pub(super) fn list_parts(node: &SyntaxNode, open: SyntaxKind) -> Option<ListParts> {
    let mut before = Vec::new();
    let mut open_token: Option<SyntaxToken> = None;
    let mut elems: Vec<(SyntaxNode, Option<SyntaxToken>)> = Vec::new();
    let mut close: Option<SyntaxToken> = None;
    for element in children(node) {
        if open_token.is_none() {
            match element.as_token() {
                Some(token) if token.kind() == open => open_token = Some(token.clone()),
                _ => before.push(element),
            }
            continue;
        }
        if close.is_some() {
            return None;
        }
        match (element.as_node(), element.as_token()) {
            (Some(node), _) => elems.push((node.clone(), None)),
            (None, Some(token)) if token.kind() == COMMA => match elems.last_mut() {
                Some((_, comma @ None)) => *comma = Some(token.clone()),
                _ => return None,
            },
            (None, Some(token)) => close = Some(token.clone()),
            (None, None) => return None,
        }
    }
    Some(ListParts {
        before,
        open: open_token?,
        elems,
        close: close?,
    })
}

/// An element of a vertical list.
pub(super) struct VElem {
    pub(super) doc: Doc,
    pub(super) first: Option<SyntaxToken>,
    pub(super) comma: Option<SyntaxToken>,
}

/// Right-hand sides that start on the operator's line: `op + " " + rhs`.
fn rhs_stays(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        CALL_EXPR
            | METHOD_CALL_EXPR
            | FORM_EXPR
            | TEXT_FORM
            | RECORD_EXPR
            | ARRAY_EXPR
            | TUPLE_EXPR
            | CLOSURE_EXPR
            | BLOCK_EXPR
            | IF_EXPR
            | MATCH_EXPR
            | LOOP_EXPR
            | WHILE_EXPR
            | FOR_EXPR
            | PAREN_EXPR
    )
}

/// Huggable last elements of argument lists and `()`/`[]` forms.
fn huggable(node: &SyntaxNode) -> bool {
    match node.kind() {
        BLOCK_EXPR | RECORD_EXPR | ARRAY_EXPR => true,
        CLOSURE_EXPR => node
            .children()
            .last()
            .is_some_and(|body| body.kind() == BLOCK_EXPR),
        FORM_EXPR => node
            .children_with_tokens()
            .filter_map(|e| e.into_token())
            .any(|t| matches!(t.kind(), L_BRACK | L_CURLY)),
        _ => false,
    }
}

impl Formatter {
    /// `G(open, I(S|L, e1, ",", L, e2, …, trailing), S|L, close)`.
    pub(super) fn delimited(
        &self,
        open: &SyntaxToken,
        elems: Vec<(Doc, Option<SyntaxToken>)>,
        close: &SyntaxToken,
        spaces: bool,
        trailing: Trailing,
    ) -> Doc {
        if elems.is_empty() {
            return self.empty_list(open, close);
        }
        group(self.delimited_content(open, elems, close, spaces, trailing))
    }

    fn delimited_content(
        &self,
        open: &SyntaxToken,
        elems: Vec<(Doc, Option<SyntaxToken>)>,
        close: &SyntaxToken,
        spaces: bool,
        trailing: Trailing,
    ) -> Doc {
        let edge = || if spaces { Doc::Line } else { Doc::SoftLine };
        let mut inner = vec![edge()];
        let mut elems = elems.into_iter().peekable();
        while let Some((doc, comma)) = elems.next() {
            inner.push(doc);
            if elems.peek().is_some() {
                inner.push(comma.map_or_else(|| text(","), |c| self.tok(&c)));
                inner.push(Doc::Line);
            } else {
                inner.push(self.trailing_comma(comma.as_ref(), trailing));
            }
        }
        inner.push(self.dangling(close, true));
        concat(vec![
            self.tok(open),
            indent(concat(inner)),
            edge(),
            self.tok_bare(close),
        ])
    }

    fn trailing_comma(&self, comma: Option<&SyntaxToken>, trailing: Trailing) -> Doc {
        match (trailing, comma) {
            (Trailing::IfBreak, Some(comma)) => {
                if_break(self.tok(comma), self.comments_only(comma))
            }
            (Trailing::IfBreak, None) => if_break(text(","), Doc::Nil),
            (Trailing::Never, Some(comma)) => self.comments_only(comma),
            (Trailing::Never, None) => Doc::Nil,
            (Trailing::Always, Some(comma)) => self.tok(comma),
            (Trailing::Always, None) => text(","),
        }
    }

    /// `open` and `close` with nothing between, or with the dangling comments of `close`.
    pub(super) fn empty_list(&self, open: &SyntaxToken, close: &SyntaxToken) -> Doc {
        let leading = &self.trivia.of(close).leading;
        if leading.is_empty() {
            return concat(vec![self.tok(open), self.tok_bare(close)]);
        }
        if leading.iter().all(|c| !c.is_line && !c.newline_before) {
            return concat(vec![
                self.tok(open),
                self.dangling(close, false),
                self.tok_bare(close),
            ]);
        }
        concat(vec![
            self.tok(open),
            indent(self.dangling(close, false)),
            Doc::HardLine,
            self.tok_bare(close),
        ])
    }

    /// `open, I(H, e1[","], H, e2[","], …, dangling), H, close`, with a blank line before an
    /// element that had one in the source.
    pub(super) fn vertical(
        &self,
        open: &SyntaxToken,
        elems: Vec<VElem>,
        close: &SyntaxToken,
        comma: bool,
    ) -> Doc {
        if elems.is_empty() {
            return self.empty_list(open, close);
        }
        let mut inner = Vec::new();
        for (i, elem) in elems.into_iter().enumerate() {
            inner.push(Doc::HardLine);
            if i > 0 && elem.first.as_ref().is_some_and(|t| self.blank_before(t)) {
                inner.push(Doc::BlankLine);
            }
            inner.push(elem.doc);
            match (comma, elem.comma) {
                (true, Some(c)) => inner.push(self.tok(&c)),
                (true, None) => inner.push(text(",")),
                (false, Some(c)) => inner.push(self.comments_only(&c)),
                (false, None) => {}
            }
        }
        inner.push(self.dangling(close, true));
        concat(vec![
            self.tok(open),
            indent(concat(inner)),
            Doc::HardLine,
            self.tok_bare(close),
        ])
    }

    /// The elements of `parts` as vertical-list elements.
    pub(super) fn velems(&self, parts: &ListParts) -> Vec<VElem> {
        parts
            .elems
            .iter()
            .map(|(node, comma)| VElem {
                doc: self.node_hoisted(node),
                first: node.first_token(),
                comma: comma.clone(),
            })
            .collect()
    }

    /// A `{}` list node laid out vertically.
    pub(super) fn vertical_list(&self, node: &SyntaxNode, comma: bool) -> Doc {
        let Some(parts) = list_parts(node, L_CURLY) else {
            return self.verbatim(node);
        };
        let mut docs: Vec<Doc> = parts.before.iter().map(|e| self.el(e)).collect();
        docs.push(self.vertical(&parts.open, self.velems(&parts), &parts.close, comma));
        concat(docs)
    }

    /// The elements of `parts` as delimited-list elements.
    fn delems(&self, parts: &ListParts) -> Vec<(Doc, Option<SyntaxToken>)> {
        parts
            .elems
            .iter()
            .map(|(node, comma)| (self.node_hoisted(node), comma.clone()))
            .collect()
    }

    /// `before` followed by `delimited(…)` of `parts`.
    pub(super) fn delimited_parts(
        &self,
        parts: &ListParts,
        spaces: bool,
        trailing: Trailing,
    ) -> Doc {
        let mut docs: Vec<Doc> = parts.before.iter().map(|e| self.el(e)).collect();
        docs.push(self.delimited(
            &parts.open,
            self.delems(parts),
            &parts.close,
            spaces,
            trailing,
        ));
        concat(docs)
    }

    /// A list node whose delimiter is its first `(`, `[`, `{`, `<` or `|` token.
    pub(super) fn delimited_list(&self, node: &SyntaxNode, open: SyntaxKind, spaces: bool) -> Doc {
        match list_parts(node, open) {
            Some(parts) => self.delimited_parts(&parts, spaces, Trailing::IfBreak),
            None => self.verbatim(node),
        }
    }

    /// `(…)` and `<…>` lists without spaces inside and a trailing comma when broken.
    pub(super) fn comma_list(&self, node: &SyntaxNode) -> Doc {
        let open = match node.kind() {
            GENERIC_PARAM_LIST | GENERIC_ARG_LIST => L_ANGLE,
            USE_TREE_LIST => L_CURLY,
            _ => L_PAREN,
        };
        self.delimited_list(node, open, false)
    }

    /// `(a,)` keeps its comma; other tuples are delimited lists.
    pub(super) fn tuple(&self, node: &SyntaxNode) -> Doc {
        let Some(parts) = list_parts(node, L_PAREN) else {
            return self.verbatim(node);
        };
        let trailing = if parts.elems.len() == 1 {
            Trailing::Always
        } else {
            Trailing::IfBreak
        };
        self.delimited_parts(&parts, false, trailing)
    }

    /// A `()`/`[]`/`{}` list whose last element may hug the delimiters:
    /// `ConditionalGroup([all flat, open + flat prefix + G(last) + close, fully broken])`.
    pub(super) fn hug_parts(&self, parts: &ListParts) -> Doc {
        let mut docs: Vec<Doc> = parts.before.iter().map(|e| self.el(e)).collect();
        docs.push(self.hug_list_doc(parts));
        concat(docs)
    }

    pub(super) fn hug_list(&self, node: &SyntaxNode) -> Doc {
        match list_parts(node, L_PAREN) {
            Some(parts) => self.hug_parts(&parts),
            None => self.verbatim(node),
        }
    }

    fn hug_list_doc(&self, parts: &ListParts) -> Doc {
        let elems = self.delems(parts);
        let can_hug = parts.elems.last().is_some_and(|(last, _)| huggable(last))
            && !self.list_has_comments(parts);
        let hug = if can_hug {
            self.hug_state(parts, &elems)
        } else {
            None
        };
        let Some(hug) = hug else {
            return self.delimited(&parts.open, elems, &parts.close, false, Trailing::IfBreak);
        };
        let flat = group(self.delimited_content(
            &parts.open,
            elems.clone(),
            &parts.close,
            false,
            Trailing::IfBreak,
        ));
        let broken = broken_group(self.delimited_content(
            &parts.open,
            elems,
            &parts.close,
            false,
            Trailing::IfBreak,
        ));
        conditional_group(vec![flat, hug, broken])
    }

    /// `open + flat prefix joined ", " + G(last) + close`, unless the prefix cannot be flat.
    fn hug_state(&self, parts: &ListParts, elems: &[(Doc, Option<SyntaxToken>)]) -> Option<Doc> {
        let ((last, _), prefix) = elems.split_last()?;
        let mut docs = vec![self.tok(&parts.open)];
        for (doc, comma) in prefix {
            let doc = flatten(doc.clone());
            if has_forced_break(&doc) {
                return None;
            }
            docs.push(doc);
            docs.push(comma.as_ref().map_or_else(|| text(","), |c| self.tok(c)));
            docs.push(text(" "));
        }
        // The list has no comments, so a dropped trailing comma has none to keep.
        docs.push(group(last.clone()));
        docs.push(self.tok_bare(&parts.close));
        Some(concat(docs))
    }

    /// Whether any comment sits at the list's own tokens or at the edges of its elements.
    fn list_has_comments(&self, parts: &ListParts) -> bool {
        self.has_comments(&parts.open)
            || self.has_comments(&parts.close)
            || parts.elems.iter().any(|(node, comma)| {
                comma.as_ref().is_some_and(|c| self.has_comments(c))
                    || node
                        .first_token()
                        .is_some_and(|t| self.has_leading_comments(&t))
                    || node
                        .last_token()
                        .is_some_and(|t| self.has_trailing_comments(&t))
            })
    }

    /// `op + " " + rhs` for block-like and call-like right-hand sides, else
    /// `G(op, I(L, rhs))`.
    pub(super) fn assign(&self, op: Doc, rhs: &SyntaxNode) -> Doc {
        let rhs_doc = self.node_hoisted(rhs);
        if rhs_stays(rhs.kind()) {
            concat(vec![op, text(" "), rhs_doc])
        } else {
            group(concat(vec![op, indent(concat(vec![Doc::Line, rhs_doc]))]))
        }
    }

    /// Children of `node`, with the element after an `op_kind` token laid out by `assign`.
    /// `op_space` puts a space before the operator (`" ="`), as opposed to `":"`.
    pub(super) fn with_assign(
        &self,
        node: &SyntaxNode,
        op_kind: SyntaxKind,
        op_space: bool,
        mut rest: impl FnMut(&SyntaxElement) -> Doc,
    ) -> Doc {
        let mut docs = Vec::new();
        let elements = children(node);
        let mut iter = elements.iter();
        while let Some(element) = iter.next() {
            if element.kind() != op_kind {
                docs.push(rest(element));
                continue;
            }
            let op = match element.as_token() {
                Some(token) if op_space => concat(vec![text(" "), self.tok(token)]),
                _ => self.el(element),
            };
            match iter.next() {
                Some(rhs) => match rhs.as_node() {
                    Some(rhs) => docs.push(self.assign(op, rhs)),
                    None => docs.extend([op, text(" "), self.el(rhs)]),
                },
                None => docs.push(op),
            }
        }
        concat(docs)
    }
}
