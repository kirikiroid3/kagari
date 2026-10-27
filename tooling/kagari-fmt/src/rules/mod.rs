//! Layout rules: every CST node kind to a `Doc`.
//!
//! Every significant token is emitted exactly once, through `Formatter::tok` (or `tok_bare` after
//! `dangling` for closing delimiters), which also emits the comments the token owns. The only
//! exception is commas, which lists add or drop; a dropped comma still emits its comments.

mod blocks;
mod chains;
mod exprs;
mod forms;
mod items;
mod lists;
mod patterns;
mod types;

use std::cell::RefCell;

use kagari_syntax::{SyntaxElement, SyntaxKind, SyntaxNode, SyntaxToken, TextSize};
use rustc_hash::FxHashSet;

use crate::doc::{Doc, concat, text};
use crate::trivia::{TokenTrivia, Trivia};

use SyntaxKind::*;

/// The `Doc` for a whole `SOURCE_FILE`.
pub(crate) fn format_file(root: &SyntaxNode) -> Doc {
    let formatter = Formatter {
        trivia: Trivia::new(root),
        hoisted: RefCell::default(),
    };
    formatter.node(root)
}

pub(crate) struct Formatter {
    trivia: Trivia,
    /// Tokens whose leading comments were already emitted in front of their element, outside
    /// the element's groups (see `hoisted`).
    hoisted: RefCell<FxHashSet<TextSize>>,
}

/// Where `Formatter::spaced` puts a space around a child.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Space {
    No,
    Before,
    After,
    Both,
}

/// The significant (non-trivia) children of `node`, nodes and tokens.
pub(super) fn children(node: &SyntaxNode) -> Vec<SyntaxElement> {
    node.children_with_tokens()
        .filter(|e| !e.kind().is_trivia())
        .collect()
}

/// The first significant token of an element.
pub(super) fn first_token(element: &SyntaxElement) -> Option<SyntaxToken> {
    match element.as_node() {
        Some(node) => node.first_token(),
        None => element.as_token().cloned(),
    }
}

pub(super) fn is_item(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        FN | STRUCT | ENUM | TYPE_ALIAS | CONST | STATIC | MOD | USE | IMPL | TRAIT
    )
}

impl Formatter {
    /// Leading comments, the token text and trailing comments.
    pub(super) fn tok(&self, token: &SyntaxToken) -> Doc {
        let trivia = self.trivia.of(token);
        let leading = if self.hoisted.borrow().contains(&token.text_range().start()) {
            Doc::Nil
        } else {
            self.leading_doc(trivia)
        };
        concat(vec![leading, text(token.text()), trailing_doc(trivia)])
    }

    /// An element preceded by the leading comments of its first token, so that those comments
    /// sit outside the element's groups: a comment line above `a + b` must not break the `+`.
    pub(super) fn hoisted(&self, element: &SyntaxElement) -> Doc {
        let mut leading = Doc::Nil;
        if let Some(first) = first_token(element)
            && self.hoisted.borrow_mut().insert(first.text_range().start())
        {
            leading = self.leading_doc(self.trivia.of(&first));
        }
        concat(vec![leading, self.el(element)])
    }

    /// `hoisted` for a node.
    pub(super) fn node_hoisted(&self, node: &SyntaxNode) -> Doc {
        self.hoisted(&SyntaxElement::Node(node.clone()))
    }

    /// The token text and trailing comments; the leading comments go through `dangling`.
    pub(super) fn tok_bare(&self, token: &SyntaxToken) -> Doc {
        concat(vec![
            text(token.text()),
            trailing_doc(self.trivia.of(token)),
        ])
    }

    /// The comments of a token whose text is dropped (a comma).
    pub(super) fn comments_only(&self, token: &SyntaxToken) -> Doc {
        let trivia = self.trivia.of(token);
        concat(vec![self.leading_doc(trivia), trailing_doc(trivia)])
    }

    /// The leading comments of a closing delimiter, emitted inside the list's indentation.
    /// `after_elements` tells whether an inline block comment follows list content.
    pub(super) fn dangling(&self, close: &SyntaxToken, after_elements: bool) -> Doc {
        let mut docs = Vec::new();
        for comment in &self.trivia.of(close).leading {
            if comment.is_line || comment.newline_before {
                docs.push(Doc::HardLine);
                if comment.blank_line_before {
                    docs.push(Doc::BlankLine);
                }
                docs.push(text(comment.text.as_str()));
                docs.push(Doc::HardLine);
            } else if after_elements {
                docs.push(text(format!(" {}", comment.text)));
            } else {
                docs.push(text(comment.text.as_str()));
            }
        }
        concat(docs)
    }

    /// Comments before the virtual end-of-file token.
    pub(super) fn eof_comments(&self) -> Doc {
        self.leading_doc(self.trivia.eof())
    }

    fn leading_doc(&self, trivia: &TokenTrivia) -> Doc {
        if trivia.leading.is_empty() {
            return Doc::Nil;
        }
        let mut docs = Vec::new();
        for comment in &trivia.leading {
            if comment.is_line || comment.newline_before {
                docs.push(Doc::HardLine);
                if comment.blank_line_before {
                    docs.push(Doc::BlankLine);
                }
                docs.push(text(comment.text.as_str()));
                if comment.is_line || comment.newline_after {
                    docs.push(Doc::HardLine);
                } else {
                    docs.push(text(" "));
                }
            } else {
                docs.push(text(format!("{} ", comment.text)));
            }
        }
        if trivia.blank_after_leading {
            docs.push(Doc::BlankLine);
        }
        concat(docs)
    }

    pub(super) fn has_comments(&self, token: &SyntaxToken) -> bool {
        let trivia = self.trivia.of(token);
        !trivia.leading.is_empty() || !trivia.trailing.is_empty()
    }

    pub(super) fn has_leading_comments(&self, token: &SyntaxToken) -> bool {
        !self.trivia.of(token).leading.is_empty()
    }

    pub(super) fn has_trailing_comments(&self, token: &SyntaxToken) -> bool {
        !self.trivia.of(token).trailing.is_empty()
    }

    /// Whether the source has an empty line before `token` or its first leading comment.
    pub(super) fn blank_before(&self, token: &SyntaxToken) -> bool {
        self.trivia.of(token).blank_before
    }

    pub(super) fn el(&self, element: &SyntaxElement) -> Doc {
        match (element.as_node(), element.as_token()) {
            (Some(node), _) => self.node(node),
            (None, Some(token)) => self.tok(token),
            (None, None) => Doc::Nil,
        }
    }

    /// The children of `node` in order, with spaces placed by `rule`.
    pub(super) fn spaced(&self, node: &SyntaxNode, rule: impl Fn(&SyntaxElement) -> Space) -> Doc {
        let mut docs = Vec::new();
        for element in children(node) {
            let space = rule(&element);
            if matches!(space, Space::Before | Space::Both) {
                docs.push(text(" "));
            }
            docs.push(self.el(&element));
            if matches!(space, Space::After | Space::Both) {
                docs.push(text(" "));
            }
        }
        concat(docs)
    }

    /// The children of `node` with nothing between them.
    pub(super) fn plain(&self, node: &SyntaxNode) -> Doc {
        self.spaced(node, |_| Space::No)
    }

    /// Spaced by `rule_kind`, which looks at kinds only.
    pub(super) fn spaced_kinds(
        &self,
        node: &SyntaxNode,
        rule_kind: impl Fn(SyntaxKind) -> Space,
    ) -> Doc {
        self.spaced(node, |element| rule_kind(element.kind()))
    }

    /// Tokens as written: one space between two tokens iff the source had trivia between them,
    /// never after an opening or before a closing delimiter.
    pub(super) fn verbatim(&self, node: &SyntaxNode) -> Doc {
        let tokens: Vec<SyntaxToken> = node
            .descendants_with_tokens()
            .filter_map(|e| e.into_token())
            .filter(|t| !t.kind().is_trivia())
            .collect();
        let mut docs = Vec::new();
        let mut prev: Option<&SyntaxToken> = None;
        for token in &tokens {
            if let Some(prev) = prev {
                let separated = prev.next_token().is_some_and(|t| t.kind().is_trivia());
                let line_comment_after = self.trivia.of(prev).trailing.iter().any(|c| c.is_line);
                if separated
                    && !line_comment_after
                    && !matches!(prev.kind(), L_PAREN | L_BRACK | L_CURLY)
                    && !matches!(token.kind(), R_PAREN | R_BRACK | R_CURLY)
                {
                    docs.push(text(" "));
                }
            }
            docs.push(self.tok(token));
            prev = Some(token);
        }
        concat(docs)
    }

    /// Dispatch on the node kind. Every node kind is listed; token kinds never get here.
    pub(super) fn node(&self, node: &SyntaxNode) -> Doc {
        match node.kind() {
            SOURCE_FILE => self.source_file(node),

            // Items.
            ATTR => self.attr(node),
            TOKEN_TREE | ERROR => self.verbatim(node),
            VISIBILITY => concat(vec![self.plain(node), text(" ")]),
            FN => self.fn_item(node),
            PARAM_LIST => self.param_list(node),
            SELF_PARAM => self.spaced_kinds(node, |k| after_if(k, MUT_KW)),
            PARAM | RECORD_FIELD | TYPE_PARAM | RECORD_PAT_FIELD => {
                self.spaced_kinds(node, |k| after_if(k, COLON))
            }
            RET_TYPE => self.spaced_kinds(node, |k| after_if(k, THIN_ARROW)),
            STRUCT | ENUM | MOD | USE | TRAIT | IMPL => self.item(node),
            RECORD_FIELD_LIST => self.record_field_list(node),
            TUPLE_FIELD_LIST | USE_TREE_LIST | GENERIC_PARAM_LIST | GENERIC_ARG_LIST
            | TUPLE_STRUCT_PAT => self.comma_list(node),
            TUPLE_FIELD | TYPE_BOUND | NAME | NAME_REF | PATH | PATH_SEGMENT | TYPE_ARG
            | CONST_ARG => self.plain(node),
            VARIANT_LIST => self.vertical_list(node, true),
            ITEM_LIST | ASSOC_ITEM_LIST | MATCH_ARM_LIST => self.vertical_list(node, false),
            VARIANT => self.spaced_kinds(node, |k| before_if(k, RECORD_FIELD_LIST)),
            TYPE_ALIAS | CONST | STATIC | LET_STMT => self.decl(node),
            USE_TREE => self.spaced_kinds(node, |k| before_if(k, RENAME)),
            RENAME => self.spaced_kinds(node, |k| after_if(k, AS_KW)),
            CONST_PARAM => self.spaced_kinds(node, |k| match k {
                CONST_KW | COLON => Space::After,
                _ => Space::No,
            }),
            TYPE_BOUND_LIST => self.spaced_kinds(node, |k| both_if(k, PLUS)),

            // Types.
            PATH_TYPE | PAREN_TYPE | INFER_TYPE => self.plain(node),
            TUPLE_TYPE | TUPLE_PAT | TUPLE_EXPR => self.tuple(node),
            ARRAY_TYPE => self.spaced_kinds(node, |k| after_if(k, SEMICOLON)),
            REF_TYPE | REF_EXPR => self.spaced_kinds(node, |k| after_if(k, MUT_KW)),
            FN_PTR_TYPE | CLOSURE_TYPE => self.fn_type(node),

            // Patterns.
            WILDCARD_PAT | PATH_PAT | LITERAL_PAT | RANGE_PAT | REST_PAT | PAREN_PAT => {
                self.plain(node)
            }
            IDENT_PAT => self.spaced_kinds(node, |k| after_if(k, MUT_KW)),
            RECORD_PAT => self.spaced_kinds(node, |k| before_if(k, RECORD_PAT_FIELD_LIST)),
            RECORD_PAT_FIELD_LIST => self.record_pat_field_list(node),
            OR_PAT => self.or_pat(node),

            // Statements and blocks.
            BLOCK_EXPR => self.block(node, false),
            EXPR_STMT => self.plain(node),

            // Expressions.
            LITERAL | PATH_EXPR | PAREN_EXPR | PREFIX_EXPR | RANGE_EXPR | CALL_EXPR
            | LABEL_DECL => self.plain(node),
            ARRAY_EXPR => self.array(node),
            RECORD_EXPR => self.spaced_kinds(node, |k| before_if(k, RECORD_EXPR_FIELD_LIST)),
            RECORD_EXPR_FIELD_LIST => self.delimited_list(node, L_CURLY, true),
            RECORD_EXPR_FIELD => self.record_expr_field(node),
            IF_EXPR => self.if_expr(node),
            LET_EXPR => self.spaced_kinds(node, |k| match k {
                LET_KW => Space::After,
                EQ => Space::Both,
                _ => Space::No,
            }),
            MATCH_EXPR => self.spaced_kinds(node, |k| match k {
                MATCH_KW => Space::After,
                MATCH_ARM_LIST => Space::Before,
                _ => Space::No,
            }),
            MATCH_ARM => self.match_arm(node),
            MATCH_GUARD | FORM_GUARD => self.spaced_kinds(node, |k| after_if(k, IF_KW)),
            LOOP_EXPR | WHILE_EXPR | FOR_EXPR => self.loop_expr(node),
            BREAK_EXPR | CONTINUE_EXPR | RETURN_EXPR => self.jump(node),
            ARG_LIST => self.hug_list(node),
            METHOD_CALL_EXPR | FIELD_EXPR | INDEX_EXPR | TRY_EXPR => self.chain(node),
            CAST_EXPR => self.spaced_kinds(node, |k| both_if(k, AS_KW)),
            BIN_EXPR => self.bin_expr(node),
            CLOSURE_EXPR => self.spaced_kinds(node, |k| match k {
                PARAM_LIST => Space::No,
                _ => Space::Before,
            }),

            // Forms.
            FORM_EXPR => self.form_expr(node),
            FORM_ATTR => self.form_attr(node),
            FORM_ARM => self.spaced_kinds(node, |k| match k {
                FORM_GUARD => Space::Before,
                FAT_ARROW => Space::Both,
                _ => Space::No,
            }),
            TEXT_FORM => self.text_form(node),
            TEXT_STRING | TEXT_RUBY | RUBY_READINGS | TEXT_EFFECT | TEXT_INTERP => self.plain(node),

            // Token kinds are never nodes.
            _ => self.verbatim(node),
        }
    }
}

fn trailing_doc(trivia: &TokenTrivia) -> Doc {
    if trivia.trailing.is_empty() {
        return Doc::Nil;
    }
    let mut docs = Vec::new();
    for comment in &trivia.trailing {
        docs.push(text(" "));
        docs.push(text(comment.text.as_str()));
        if comment.is_line {
            docs.push(Doc::HardLine);
        }
    }
    concat(docs)
}

fn after_if(kind: SyntaxKind, target: SyntaxKind) -> Space {
    if kind == target {
        Space::After
    } else {
        Space::No
    }
}

fn before_if(kind: SyntaxKind, target: SyntaxKind) -> Space {
    if kind == target {
        Space::Before
    } else {
        Space::No
    }
}

fn both_if(kind: SyntaxKind, target: SyntaxKind) -> Space {
    if kind == target {
        Space::Both
    } else {
        Space::No
    }
}
