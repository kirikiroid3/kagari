//! Parse results shared by every language in this crate.

use std::marker::PhantomData;

use rowan::{GreenNode, Language, SyntaxNode, TextRange};

/// A syntax error found while lexing or parsing.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SyntaxError {
    pub message: String,
    pub range: TextRange,
}

impl SyntaxError {
    pub fn new(message: impl Into<String>, range: TextRange) -> SyntaxError {
        SyntaxError {
            message: message.into(),
            range,
        }
    }
}

/// A lossless green tree plus the syntax errors found while building it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Parse<L: Language> {
    green: GreenNode,
    errors: Vec<SyntaxError>,
    _lang: PhantomData<fn() -> L>,
}

impl<L: Language> Parse<L> {
    pub fn new(green: GreenNode, errors: Vec<SyntaxError>) -> Parse<L> {
        Parse {
            green,
            errors,
            _lang: PhantomData,
        }
    }

    pub fn syntax_node(&self) -> SyntaxNode<L> {
        SyntaxNode::new_root(self.green.clone())
    }

    pub fn errors(&self) -> &[SyntaxError] {
        &self.errors
    }

    pub fn green(&self) -> &GreenNode {
        &self.green
    }
}
