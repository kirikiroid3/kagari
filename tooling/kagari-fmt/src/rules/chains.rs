//! Postfix chains (`a.b(c).d?[e]`) and binary operators.

use kagari_syntax::{SyntaxElement, SyntaxKind, SyntaxNode, SyntaxToken};

use super::{Formatter, children};
use crate::doc::{Doc, concat, group, indent, text};

use SyntaxKind::*;

fn is_link(kind: SyntaxKind) -> bool {
    matches!(kind, METHOD_CALL_EXPR | FIELD_EXPR | INDEX_EXPR | TRY_EXPR)
}

fn is_assignment(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        EQ | PLUS_EQ
            | MINUS_EQ
            | STAR_EQ
            | SLASH_EQ
            | PERCENT_EQ
            | CARET_EQ
            | AMP_EQ
            | PIPE_EQ
            | SHL_EQ
            | SHR_EQ
    )
}

/// The binding-power class of a binary operator (the parser's left binding power).
fn class(kind: SyntaxKind) -> u8 {
    match kind {
        PIPE2 => 5,
        AMP2 => 7,
        EQ2 | NEQ | L_ANGLE | R_ANGLE | LTEQ | GTEQ => 9,
        PIPE => 11,
        CARET => 13,
        AMP => 15,
        SHL | SHR => 17,
        PLUS | MINUS => 19,
        STAR | SLASH | PERCENT => 21,
        _ => 0,
    }
}

/// `lhs op rhs` of a `BIN_EXPR`.
fn bin_parts(node: &SyntaxNode) -> Option<(SyntaxNode, SyntaxToken, SyntaxNode)> {
    let mut lhs = None;
    let mut op = None;
    let mut rhs = None;
    for element in children(node) {
        match (element.as_node(), element.as_token()) {
            (Some(n), _) if lhs.is_none() => lhs = Some(n.clone()),
            (Some(n), _) => rhs = Some(n.clone()),
            (None, Some(t)) => op = Some(t.clone()),
            (None, None) => {}
        }
    }
    Some((lhs?, op?, rhs?))
}

impl Formatter {
    /// Flattens the postfix chain into a root and links. With two or more method calls:
    /// `G(root, I(S .link1, S .link2, …))`, where `?` and `[i]` stay on their link and field
    /// accesses right after the root stay on the root's line. Otherwise the links are
    /// concatenated and only argument lists break.
    pub(super) fn chain(&self, node: &SyntaxNode) -> Doc {
        let mut links = Vec::new();
        let mut current = node.clone();
        while is_link(current.kind()) {
            let Some(receiver) = current.first_child() else {
                return self.verbatim(node);
            };
            links.push(current);
            current = receiver;
        }
        links.reverse();
        let calls = links
            .iter()
            .filter(|l| l.kind() == METHOD_CALL_EXPR)
            .count();
        let mut head = vec![self.node(&current)];
        if calls < 2 {
            head.extend(links.iter().map(|l| self.link(l)));
            return concat(head);
        }
        let mut rest = Vec::new();
        let mut seen_call = false;
        for link in &links {
            seen_call |= link.kind() == METHOD_CALL_EXPR;
            let breaks = matches!(link.kind(), METHOD_CALL_EXPR | FIELD_EXPR);
            if !seen_call {
                head.push(self.link(link));
            } else {
                if breaks {
                    rest.push(Doc::SoftLine);
                }
                rest.push(self.link(link));
            }
        }
        head.push(indent(concat(rest)));
        group(concat(head))
    }

    /// The part of a postfix node after its receiver.
    fn link(&self, node: &SyntaxNode) -> Doc {
        let elements: Vec<SyntaxElement> = children(node).into_iter().skip(1).collect();
        concat(elements.iter().map(|e| self.el(e)).collect())
    }

    /// Assignments: `lhs` + the assignment layout with `" " + op`. Other operators: left-nested
    /// operators of one binding-power class are flattened into `G(first, I(L op " " operand, …))`.
    pub(super) fn bin_expr(&self, node: &SyntaxNode) -> Doc {
        let Some((lhs, op, rhs)) = bin_parts(node) else {
            return self.verbatim(node);
        };
        if is_assignment(op.kind()) {
            let op = concat(vec![text(" "), self.tok(&op)]);
            return concat(vec![self.node(&lhs), self.assign(op, &rhs)]);
        }
        let level = class(op.kind());
        let mut tail = vec![(op, rhs)];
        let mut first = lhs;
        while first.kind() == BIN_EXPR {
            let Some((lhs, op, rhs)) = bin_parts(&first) else {
                break;
            };
            if class(op.kind()) != level || is_assignment(op.kind()) {
                break;
            }
            tail.push((op, rhs));
            first = lhs;
        }
        tail.reverse();
        let mut rest = Vec::new();
        for (op, operand) in &tail {
            rest.push(Doc::Line);
            rest.push(self.tok(op));
            rest.push(text(" "));
            rest.push(self.node(operand));
        }
        group(concat(vec![self.node(&first), indent(concat(rest))]))
    }
}
