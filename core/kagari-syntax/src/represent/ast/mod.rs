//! Typed wrappers over the `.kirp`/`.kix` syntax tree.
//!
//! Every accessor returns `Option` or an iterator: the tree may come from erroneous input.

use rowan::ast::{AstChildren, AstNode, support};

use crate::represent::cst::{RepresentLanguage, SyntaxNode, SyntaxToken};

/// A typed wrapper around one node kind.
macro_rules! ast_node {
    ($(#[$meta:meta])* $name:ident, $kind:ident) => {
        $(#[$meta])*
        #[derive(Clone, Debug, PartialEq, Eq, Hash)]
        pub struct $name(pub(super) $crate::represent::cst::SyntaxNode);

        impl ::rowan::ast::AstNode for $name {
            type Language = $crate::represent::cst::RepresentLanguage;

            fn can_cast(kind: $crate::represent::cst::RepresentKind) -> bool {
                kind == $crate::represent::cst::RepresentKind::$kind
            }

            fn cast(node: $crate::represent::cst::SyntaxNode) -> Option<Self> {
                <Self as ::rowan::ast::AstNode>::can_cast(node.kind()).then(|| $name(node))
            }

            fn syntax(&self) -> &$crate::represent::cst::SyntaxNode {
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
            type Language = $crate::represent::cst::RepresentLanguage;

            fn can_cast(kind: $crate::represent::cst::RepresentKind) -> bool {
                matches!(kind, $($crate::represent::cst::RepresentKind::$kind)|*)
            }

            fn cast(node: $crate::represent::cst::SyntaxNode) -> Option<Self> {
                Some(match node.kind() {
                    $($crate::represent::cst::RepresentKind::$kind => $name::$variant($variant(node)),)*
                    _ => return None,
                })
            }

            fn syntax(&self) -> &$crate::represent::cst::SyntaxNode {
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

mod exprs;
mod items;
mod literal;
mod paths;
mod pats;
mod stmts;

pub use exprs::*;
pub use items::*;
pub use literal::*;
pub use paths::*;
pub use pats::*;
pub use stmts::*;

fn tokens(node: &SyntaxNode) -> impl Iterator<Item = SyntaxToken> + use<> {
    node.children_with_tokens().filter_map(|it| it.into_token())
}

fn token_text(token: Option<SyntaxToken>) -> String {
    token.map(|t| t.text().to_owned()).unwrap_or_default()
}

/// Items carry their leading `#[…]` attributes as children.
pub trait HasAttrs: AstNode<Language = RepresentLanguage> {
    fn attrs(&self) -> AstChildren<Attr> {
        support::children(self.syntax())
    }

    /// Whether an attribute with this name is present.
    fn has_attr(&self, name: &str) -> bool {
        self.attrs()
            .any(|attr| attr.name_text().as_deref() == Some(name))
    }
}

/// Nodes with a defining `NAME` child.
pub trait HasName: AstNode<Language = RepresentLanguage> {
    fn name(&self) -> Option<Name> {
        support::child(self.syntax())
    }
}

/// Nodes with a `GENERIC_PARAM_LIST` child.
pub trait HasGenericParams: AstNode<Language = RepresentLanguage> {
    fn generic_param_list(&self) -> Option<GenericParamList> {
        support::child(self.syntax())
    }

    /// The generic parameters, or nothing if there is no list.
    fn generic_params(
        &self,
    ) -> std::iter::Flatten<std::option::IntoIter<AstChildren<GenericParam>>> {
        self.generic_param_list()
            .map(|list| list.params())
            .into_iter()
            .flatten()
    }
}
