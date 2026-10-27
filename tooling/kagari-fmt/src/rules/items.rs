//! Items: the source file, attributes, functions, ADTs, declarations, modules, uses, impls and
//! traits.

use kagari_syntax::{SyntaxKind, SyntaxNode};

use super::{Formatter, Space, children, first_token};
use crate::doc::{Doc, concat, group, text};

use SyntaxKind::*;

impl Formatter {
    /// Items separated by `H`, with `B` where the source had a blank line, then the comments at
    /// the end of the file.
    pub(super) fn source_file(&self, node: &SyntaxNode) -> Doc {
        let mut docs = Vec::new();
        for (i, element) in children(node).iter().enumerate() {
            if i > 0 {
                docs.push(Doc::HardLine);
                if first_token(element).is_some_and(|t| self.blank_before(&t)) {
                    docs.push(Doc::BlankLine);
                }
            }
            docs.push(self.hoisted(element));
        }
        docs.push(self.eof_comments());
        concat(docs)
    }

    /// `#[path(tokens)]` on its own line.
    pub(super) fn attr(&self, node: &SyntaxNode) -> Doc {
        concat(vec![self.plain(node), Doc::HardLine])
    }

    /// `G([vis][const |pushdown ]fn name[generics](params)[ -> ret])` then a vertical body or `;`.
    pub(super) fn fn_item(&self, node: &SyntaxNode) -> Doc {
        let mut docs = Vec::new();
        let mut header = Vec::new();
        for element in children(node) {
            match element.kind() {
                ATTR => docs.push(self.el(&element)),
                CONST_KW | PUSHDOWN_KW | FN_KW => {
                    header.push(self.el(&element));
                    header.push(text(" "));
                }
                RET_TYPE => {
                    header.push(text(" "));
                    header.push(self.el(&element));
                }
                BLOCK_EXPR => {
                    docs.push(group(concat(std::mem::take(&mut header))));
                    docs.push(text(" "));
                    match element.as_node() {
                        Some(body) => docs.push(self.block(body, true)),
                        None => docs.push(self.el(&element)),
                    }
                }
                SEMICOLON => {
                    docs.push(group(concat(std::mem::take(&mut header))));
                    docs.push(self.el(&element));
                }
                _ => header.push(self.el(&element)),
            }
        }
        if !header.is_empty() {
            docs.push(group(concat(header)));
        }
        concat(docs)
    }

    /// `(…)` parameters are a delimited list; `|…|` closure parameters never break.
    pub(super) fn param_list(&self, node: &SyntaxNode) -> Doc {
        let pipe = node.first_token().is_some_and(|t| t.kind() == PIPE);
        if !pipe {
            return self.delimited_list(node, L_PAREN, false);
        }
        let elements = children(node);
        let mut docs = Vec::new();
        let mut iter = elements.iter().peekable();
        while let Some(element) = iter.next() {
            let last = iter.peek().is_none_or(|next| next.kind() == PIPE);
            match element.as_token() {
                Some(comma) if comma.kind() == COMMA => {
                    if last {
                        docs.push(self.comments_only(comma));
                    } else {
                        docs.push(self.tok(comma));
                        docs.push(text(" "));
                    }
                }
                _ => docs.push(self.el(element)),
            }
        }
        concat(docs)
    }

    /// `struct`, `enum`, `mod`, `use`, `trait` and `impl`: keywords and names spaced, the body
    /// list after a space.
    pub(super) fn item(&self, node: &SyntaxNode) -> Doc {
        let kind = node.kind();
        self.spaced(node, |element| match element.kind() {
            ATTR | VISIBILITY | NAME | GENERIC_PARAM_LIST | SEMICOLON | USE_TREE => Space::No,
            STRUCT_KW | ENUM_KW | MOD_KW | USE_KW | TRAIT_KW | COLON => Space::After,
            RECORD_FIELD_LIST | VARIANT_LIST | ITEM_LIST | ASSOC_ITEM_LIST | FOR_KW => {
                Space::Before
            }
            TUPLE_FIELD_LIST | IMPL_KW | TYPE_BOUND_LIST => Space::No,
            // The types of an impl header.
            _ if kind == IMPL && element.as_node().is_some() => Space::Before,
            _ => Space::No,
        })
    }

    /// A struct's `{}` fields are vertical; an enum variant's are inline.
    pub(super) fn record_field_list(&self, node: &SyntaxNode) -> Doc {
        if node.parent().is_some_and(|p| p.kind() == VARIANT) {
            self.delimited_list(node, L_CURLY, true)
        } else {
            self.vertical_list(node, true)
        }
    }

    /// `type`, `const`, `static` and `let`: keywords spaced, `: type`, then `=` with the
    /// assignment layout.
    pub(super) fn decl(&self, node: &SyntaxNode) -> Doc {
        self.with_assign(node, EQ, true, |element| match element.kind() {
            TYPE_KW | CONST_KW | STATIC_KW | MUT_KW | LET_KW | COLON => {
                concat(vec![self.el(element), text(" ")])
            }
            _ => self.el(element),
        })
    }
}
