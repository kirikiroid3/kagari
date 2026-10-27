//! Blocks, `if`, `match` and loops.

use kagari_syntax::{SyntaxElement, SyntaxKind, SyntaxNode};

use super::lists::VElem;
use super::{Formatter, Space, children, first_token, is_item};
use crate::doc::{Doc, concat, group, indent, text};

use SyntaxKind::*;

/// Statements and items make a block vertical; a lone tail expression does not.
fn is_statement(element: &SyntaxElement) -> bool {
    let kind = element.kind();
    matches!(kind, LET_STMT | EXPR_STMT | SEMICOLON | ERROR) || is_item(kind)
}

fn has_comment(node: &SyntaxNode) -> bool {
    node.descendants_with_tokens()
        .any(|e| matches!(e.kind(), LINE_COMMENT | BLOCK_COMMENT))
}

impl Formatter {
    /// `{ … }`: vertical when `fn_body`, when it has a statement or item, or when it contains a
    /// comment; otherwise `G({ I(L tail) L })`. Empty is `{}`.
    pub(super) fn block(&self, node: &SyntaxNode, fn_body: bool) -> Doc {
        let (doc, inline) = self.block_parts(node, fn_body);
        if inline { group(doc) } else { doc }
    }

    /// The block's doc, and whether it is the inline `{ I(L tail) L }` form that wants a group.
    fn block_parts(&self, node: &SyntaxNode, fn_body: bool) -> (Doc, bool) {
        let elements = children(node);
        let (Some((open, rest)), Some(close)) = (
            elements.split_first(),
            elements.last().and_then(|e| e.as_token()),
        ) else {
            return (self.verbatim(node), false);
        };
        let (Some(open), Some((_, inner))) = (open.as_token(), rest.split_last()) else {
            return (self.verbatim(node), false);
        };
        if inner.is_empty() {
            return (self.empty_list(open, close), false);
        }
        let vertical = fn_body || inner.iter().any(is_statement) || has_comment(node);
        if vertical {
            let elems = inner
                .iter()
                .map(|element| VElem {
                    doc: self.hoisted(element),
                    first: first_token(element),
                    comma: None,
                })
                .collect();
            return (self.vertical(open, elems, close, false), false);
        }
        let tail: Vec<Doc> = inner.iter().map(|e| self.el(e)).collect();
        let doc = concat(vec![
            self.tok(open),
            indent(concat(vec![Doc::Line, concat(tail)])),
            Doc::Line,
            self.tok_bare(close),
        ]);
        (doc, true)
    }

    /// `G(if cond " " block[ else (block|if)])`. The blocks of a whole `else if` chain share
    /// the one group, so either every block is inline or none is.
    pub(super) fn if_expr(&self, node: &SyntaxNode) -> Doc {
        group(self.if_chain(node))
    }

    fn if_chain(&self, node: &SyntaxNode) -> Doc {
        let mut docs = Vec::new();
        let mut seen_condition = false;
        for element in children(node) {
            match element.kind() {
                IF_KW => {
                    docs.push(self.el(&element));
                    docs.push(text(" "));
                }
                ELSE_KW => {
                    docs.push(text(" "));
                    docs.push(self.el(&element));
                }
                BLOCK_EXPR | IF_EXPR if seen_condition => {
                    docs.push(text(" "));
                    match element.as_node() {
                        Some(n) if n.kind() == IF_EXPR => docs.push(self.if_chain(n)),
                        Some(n) => docs.push(self.block_parts(n, false).0),
                        None => docs.push(self.el(&element)),
                    }
                }
                _ => {
                    seen_condition = true;
                    docs.push(self.el(&element));
                }
            }
        }
        concat(docs)
    }

    /// `pat[ if guard] =>` + the body in the assignment layout; a block body gets no comma,
    /// every other body gets one.
    pub(super) fn match_arm(&self, node: &SyntaxNode) -> Doc {
        let mut comma = None;
        let mut block_body = false;
        let elements: Vec<SyntaxElement> = children(node)
            .into_iter()
            .filter(|e| match e.as_token() {
                Some(t) if t.kind() == COMMA => {
                    comma = Some(t.clone());
                    false
                }
                _ => true,
            })
            .collect();
        let mut docs = Vec::new();
        let mut iter = elements.iter();
        while let Some(element) = iter.next() {
            match element.kind() {
                MATCH_GUARD => {
                    docs.push(text(" "));
                    docs.push(self.el(element));
                }
                FAT_ARROW => {
                    let op = concat(vec![text(" "), self.el(element)]);
                    match iter.next().and_then(|e| e.as_node()) {
                        Some(body) => {
                            block_body = body.kind() == BLOCK_EXPR;
                            docs.push(self.assign(op, body));
                        }
                        None => docs.push(op),
                    }
                }
                _ => docs.push(self.el(element)),
            }
        }
        docs.push(match (block_body, comma) {
            (true, Some(comma)) => self.comments_only(&comma),
            (true, None) => Doc::Nil,
            (false, Some(comma)) => self.tok(&comma),
            (false, None) => text(","),
        });
        concat(docs)
    }

    /// `['a: ]loop block`, `while cond block`, `for pat in expr block`.
    pub(super) fn loop_expr(&self, node: &SyntaxNode) -> Doc {
        let elements = children(node);
        let last = elements.last().cloned();
        self.spaced(node, |element| match element.kind() {
            LABEL_DECL | WHILE_KW | FOR_KW => Space::After,
            IN_KW => Space::Both,
            _ if Some(element) == last.as_ref() => Space::Before,
            _ => Space::No,
        })
    }
}
