//! Typed wrappers over the syntax tree.
//!
//! Every accessor returns `Option` or an iterator: the tree may come from erroneous input.
//! Accessor names follow rust-analyzer.

use rowan::ast::support;
pub use rowan::ast::{AstChildren, AstNode};

use crate::cst::{KagariLanguage, SyntaxKind, SyntaxNode, SyntaxToken};

/// A typed wrapper around one node kind.
macro_rules! ast_node {
    ($(#[$meta:meta])* $name:ident, $kind:ident) => {
        $(#[$meta])*
        #[derive(Clone, Debug, PartialEq, Eq, Hash)]
        pub struct $name(pub(crate) $crate::cst::SyntaxNode);

        impl ::rowan::ast::AstNode for $name {
            type Language = $crate::cst::KagariLanguage;

            fn can_cast(kind: $crate::cst::SyntaxKind) -> bool {
                kind == $crate::cst::SyntaxKind::$kind
            }

            fn cast(node: $crate::cst::SyntaxNode) -> Option<Self> {
                <Self as ::rowan::ast::AstNode>::can_cast(node.kind()).then(|| $name(node))
            }

            fn syntax(&self) -> &$crate::cst::SyntaxNode {
                &self.0
            }
        }
    };
}

/// A typed union over several node kinds, each wrapped by the variant's same-named node type.
macro_rules! ast_enum {
    ($(#[$meta:meta])* $name:ident { $($variant:ident($kind:ident)),* $(,)? }) => {
        $(#[$meta])*
        #[derive(Clone, Debug, PartialEq, Eq, Hash)]
        pub enum $name {
            $($variant($variant),)*
        }

        impl ::rowan::ast::AstNode for $name {
            type Language = $crate::cst::KagariLanguage;

            fn can_cast(kind: $crate::cst::SyntaxKind) -> bool {
                matches!(kind, $($crate::cst::SyntaxKind::$kind)|*)
            }

            fn cast(node: $crate::cst::SyntaxNode) -> Option<Self> {
                Some(match node.kind() {
                    $($crate::cst::SyntaxKind::$kind => $name::$variant($variant(node)),)*
                    _ => return None,
                })
            }

            fn syntax(&self) -> &$crate::cst::SyntaxNode {
                match self {
                    $($name::$variant(it) => ::rowan::ast::AstNode::syntax(it),)*
                }
            }
        }

        $(
            impl From<$variant> for $name {
                fn from(it: $variant) -> $name {
                    $name::$variant(it)
                }
            }
        )*
    };
}

/// Implements marker traits whose methods all have default bodies.
macro_rules! impl_traits {
    ($($trait:ident for $($ty:ident),+;)*) => {
        $($(impl $trait for $ty {})+)*
    };
}

mod exprs;
mod forms;
mod items;
mod literal;
mod paths;
mod pats;
mod types;

pub use exprs::*;
pub use forms::*;
pub use items::*;
pub use literal::*;
pub use paths::*;
pub use pats::*;
pub use types::*;

/// Direct token children, trivia included.
fn tokens(node: &SyntaxNode) -> impl Iterator<Item = SyntaxToken> + use<> {
    node.children_with_tokens().filter_map(|it| it.into_token())
}

/// The first direct non-trivia token.
fn first_token(node: &SyntaxNode) -> Option<SyntaxToken> {
    tokens(node).find(|t| !t.kind().is_trivia())
}

fn token(node: &SyntaxNode, kind: SyntaxKind) -> Option<SyntaxToken> {
    support::token(node, kind)
}

fn child<N: AstNode<Language = KagariLanguage>>(node: &SyntaxNode) -> Option<N> {
    support::child(node)
}

fn children<N: AstNode<Language = KagariLanguage>>(node: &SyntaxNode) -> AstChildren<N> {
    support::children(node)
}

/// The first `N` child that comes before the first token of `kind`.
fn child_before<N: AstNode<Language = KagariLanguage>>(
    node: &SyntaxNode,
    kind: SyntaxKind,
) -> Option<N> {
    node.children_with_tokens()
        .take_while(|it| it.kind() != kind)
        .filter_map(|it| it.into_node())
        .find_map(N::cast)
}

/// The first `N` child that comes after the first token for which `is_sep` holds.
fn child_after_token<N: AstNode<Language = KagariLanguage>>(
    node: &SyntaxNode,
    is_sep: impl std::ops::Fn(&SyntaxToken) -> bool,
) -> Option<N> {
    node.children_with_tokens()
        .skip_while(|it| !it.as_token().is_some_and(&is_sep))
        .skip(1)
        .filter_map(|it| it.into_node())
        .find_map(N::cast)
}

fn child_after<N: AstNode<Language = KagariLanguage>>(
    node: &SyntaxNode,
    kind: SyntaxKind,
) -> Option<N> {
    child_after_token(node, |t| t.kind() == kind)
}

/// Nodes carrying leading `#[…]` attributes as children.
pub trait HasAttrs: AstNode<Language = KagariLanguage> {
    fn attrs(&self) -> AstChildren<Attr> {
        children(self.syntax())
    }

    /// Whether an attribute with this single-segment name is present.
    fn has_attr(&self, name: &str) -> bool {
        self.attrs()
            .any(|attr| attr.name_text().as_deref() == Some(name))
    }
}

/// Nodes with a defining `NAME` child.
pub trait HasName: AstNode<Language = KagariLanguage> {
    fn name(&self) -> Option<Name> {
        child(self.syntax())
    }
}

/// Nodes with an optional `pub`.
pub trait HasVisibility: AstNode<Language = KagariLanguage> {
    fn visibility(&self) -> Option<Visibility> {
        child(self.syntax())
    }
}

/// Nodes with a `GENERIC_PARAM_LIST` child.
pub trait HasGenericParams: AstNode<Language = KagariLanguage> {
    fn generic_param_list(&self) -> Option<GenericParamList> {
        child(self.syntax())
    }

    /// The generic parameters, or nothing if there is no list.
    fn generic_params(
        &self,
    ) -> std::iter::Flatten<std::option::IntoIter<AstChildren<GenericParam>>> {
        self.generic_param_list()
            .map(|list| list.generic_params())
            .into_iter()
            .flatten()
    }
}

impl_traits! {
    HasAttrs for Fn, Struct, Enum, TypeAlias, Const, Static, Mod, Use, Impl, Trait, Item, AssocItem,
        Variant, RecordField, TupleField;
    HasName for Fn, Struct, Enum, TypeAlias, Const, Static, Mod, Trait, Item, AssocItem, Variant,
        RecordField, TypeParam, ConstParam, IdentPat, FormAttr;
    HasVisibility for Fn, Struct, Enum, TypeAlias, Const, Static, Mod, Use, Impl, Trait, Item,
        AssocItem, RecordField, TupleField;
    HasGenericParams for Fn, Struct, Enum, TypeAlias, Impl, Trait, Item, AssocItem;
}
