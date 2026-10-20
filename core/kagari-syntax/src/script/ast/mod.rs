//! Typed wrappers over the `.kis` CST.

pub mod text;

use rowan::ast::AstNode;

use crate::TextRange;

use super::{ScriptKind, ScriptLanguage, SyntaxNode, SyntaxToken};

pub use text::{DecodedText, SourceMap, decode_content};

macro_rules! ast_node {
    ($(#[$attr:meta])* $name:ident, $kind:ident) => {
        $(#[$attr])*
        #[derive(Clone, Debug, PartialEq, Eq, Hash)]
        pub struct $name {
            syntax: SyntaxNode,
        }

        impl AstNode for $name {
            type Language = ScriptLanguage;

            fn can_cast(kind: ScriptKind) -> bool {
                kind == ScriptKind::$kind
            }

            fn cast(syntax: SyntaxNode) -> Option<$name> {
                Self::can_cast(syntax.kind()).then_some($name { syntax })
            }

            fn syntax(&self) -> &SyntaxNode {
                &self.syntax
            }
        }
    };
}

ast_node!(
    /// The whole file.
    Document,
    DOCUMENT
);
ast_node!(
    /// An element: an empty-element tag, or a start tag, content and an end tag.
    Element,
    ELEMENT
);
ast_node!(
    /// `name="value"` inside a start or empty-element tag.
    Attribute,
    ATTRIBUTE
);

impl Document {
    /// The root element. Additional roots are wrapped in `ERROR` nodes and not returned.
    pub fn root(&self) -> Option<Element> {
        self.syntax.children().find_map(Element::cast)
    }
}

/// An item of element content, in document order.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Content {
    Element(Element),
    Text(SyntaxToken),
    EntityRef(SyntaxToken),
    CData(SyntaxToken),
    Comment(SyntaxToken),
}

impl Content {
    pub fn text_range(&self) -> TextRange {
        match self {
            Content::Element(element) => element.syntax.text_range(),
            Content::Text(token)
            | Content::EntityRef(token)
            | Content::CData(token)
            | Content::Comment(token) => token.text_range(),
        }
    }
}

impl Element {
    /// The `START_TAG` or `EMPTY_ELEM_TAG` node.
    fn tag(&self) -> Option<SyntaxNode> {
        self.syntax.children().find(|child| {
            matches!(
                child.kind(),
                ScriptKind::START_TAG | ScriptKind::EMPTY_ELEM_TAG
            )
        })
    }

    /// Whether the element is written as `<name … />`.
    pub fn is_empty_elem(&self) -> bool {
        self.tag()
            .is_some_and(|tag| tag.kind() == ScriptKind::EMPTY_ELEM_TAG)
    }

    /// The name token of the start tag.
    pub fn name(&self) -> Option<SyntaxToken> {
        self.tag()
            .and_then(|tag| first_token(&tag, ScriptKind::NAME))
    }

    pub fn name_text(&self) -> Option<String> {
        self.name().map(|token| token.text().to_owned())
    }

    pub fn name_range(&self) -> Option<TextRange> {
        self.name().map(|token| token.text_range())
    }

    /// Whether the element's name is `name`.
    pub fn is(&self, name: &str) -> bool {
        self.name().is_some_and(|token| token.text() == name)
    }

    /// The range of the start tag (or the empty-element tag).
    pub fn start_tag_range(&self) -> TextRange {
        self.tag()
            .map_or_else(|| self.syntax.text_range(), |tag| tag.text_range())
    }

    /// The range of the end tag, if there is one.
    pub fn end_tag_range(&self) -> Option<TextRange> {
        self.syntax
            .children()
            .find(|child| child.kind() == ScriptKind::END_TAG)
            .map(|tag| tag.text_range())
    }

    pub fn attributes(&self) -> impl Iterator<Item = Attribute> + use<> {
        self.tag()
            .into_iter()
            .flat_map(|tag| tag.children().filter_map(Attribute::cast))
    }

    /// The first attribute called `name`.
    pub fn attribute(&self, name: &str) -> Option<Attribute> {
        self.attributes()
            .find(|attribute| attribute.name().is_some_and(|token| token.text() == name))
    }

    /// The content between the start and end tags, in order. Empty for `<name/>`. Error nodes
    /// (stray end tags, unsupported markup) are skipped.
    pub fn content(&self) -> impl Iterator<Item = Content> + use<> {
        self.syntax
            .children_with_tokens()
            .filter_map(|child| match child {
                rowan::NodeOrToken::Node(node) => Element::cast(node).map(Content::Element),
                rowan::NodeOrToken::Token(token) => match token.kind() {
                    ScriptKind::TEXT => Some(Content::Text(token)),
                    ScriptKind::ENTITY_REF => Some(Content::EntityRef(token)),
                    ScriptKind::CDATA => Some(Content::CData(token)),
                    ScriptKind::COMMENT => Some(Content::Comment(token)),
                    _ => None,
                },
            })
    }

    pub fn child_elements(&self) -> impl Iterator<Item = Element> + use<> {
        self.syntax.children().filter_map(Element::cast)
    }
}

impl Attribute {
    pub fn name(&self) -> Option<SyntaxToken> {
        first_token(&self.syntax, ScriptKind::NAME)
    }

    pub fn name_text(&self) -> Option<String> {
        self.name().map(|token| token.text().to_owned())
    }

    pub fn name_range(&self) -> Option<TextRange> {
        self.name().map(|token| token.text_range())
    }

    /// The `ATTR_VALUE` token, quotes included.
    pub fn value_token(&self) -> Option<SyntaxToken> {
        first_token(&self.syntax, ScriptKind::ATTR_VALUE)
    }

    /// The range of the value, quotes included.
    pub fn value_range(&self) -> Option<TextRange> {
        self.value_token().map(|token| token.text_range())
    }

    /// The value with quotes stripped and references decoded. The map yields absolute file
    /// offsets.
    pub fn value(&self) -> Option<DecodedText> {
        self.value_token()
            .map(|token| text::decode_attr_value(token.text(), token.text_range().start()))
    }
}

fn first_token(node: &SyntaxNode, kind: ScriptKind) -> Option<SyntaxToken> {
    node.children_with_tokens()
        .filter_map(rowan::NodeOrToken::into_token)
        .find(|token| token.kind() == kind)
}
