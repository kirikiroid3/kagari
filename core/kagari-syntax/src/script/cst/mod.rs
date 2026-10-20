//! Syntax kinds, the rowan language for `.kis`, and the parser.

mod parser;

pub use parser::parse;

/// Token and node kinds of the `.kis` CST.
#[allow(non_camel_case_types, clippy::upper_case_acronyms)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u16)]
pub enum ScriptKind {
    /// `<` opening a start tag or an empty-element tag.
    L_ANGLE,
    /// `</` opening an end tag.
    L_ANGLE_SLASH,
    /// `>`.
    R_ANGLE,
    /// `/>`.
    SLASH_R_ANGLE,
    /// `=` between an attribute name and its value.
    EQ,
    /// An element or attribute name.
    NAME,
    /// An attribute value, quotes included.
    ATTR_VALUE,
    /// Whitespace inside a tag.
    WHITESPACE,
    /// Character data, whitespace included.
    TEXT,
    /// `&name;`, `&#N;` or `&#xN;`.
    ENTITY_REF,
    /// The whole `<![CDATA[…]]>`.
    CDATA,
    /// The whole `<!--…-->`.
    COMMENT,
    /// `<?xml…?>`.
    XML_DECL,
    /// Any other processing instruction `<?…?>`.
    PI,
    /// `<!DOCTYPE…>`.
    DOCTYPE,
    /// Input the lexer could not classify.
    ERROR_TOKEN,

    DOCUMENT,
    ELEMENT,
    START_TAG,
    END_TAG,
    EMPTY_ELEM_TAG,
    ATTRIBUTE,
    ERROR,
}

impl ScriptKind {
    pub const ALL: [ScriptKind; 23] = [
        ScriptKind::L_ANGLE,
        ScriptKind::L_ANGLE_SLASH,
        ScriptKind::R_ANGLE,
        ScriptKind::SLASH_R_ANGLE,
        ScriptKind::EQ,
        ScriptKind::NAME,
        ScriptKind::ATTR_VALUE,
        ScriptKind::WHITESPACE,
        ScriptKind::TEXT,
        ScriptKind::ENTITY_REF,
        ScriptKind::CDATA,
        ScriptKind::COMMENT,
        ScriptKind::XML_DECL,
        ScriptKind::PI,
        ScriptKind::DOCTYPE,
        ScriptKind::ERROR_TOKEN,
        ScriptKind::DOCUMENT,
        ScriptKind::ELEMENT,
        ScriptKind::START_TAG,
        ScriptKind::END_TAG,
        ScriptKind::EMPTY_ELEM_TAG,
        ScriptKind::ATTRIBUTE,
        ScriptKind::ERROR,
    ];

    pub fn from_u16(raw: u16) -> Option<ScriptKind> {
        ScriptKind::ALL.get(usize::from(raw)).copied()
    }

    /// Whether this kind is a token (as opposed to a node).
    pub fn is_token(self) -> bool {
        (self as u16) < (ScriptKind::DOCUMENT as u16)
    }
}

impl From<ScriptKind> for rowan::SyntaxKind {
    fn from(kind: ScriptKind) -> rowan::SyntaxKind {
        rowan::SyntaxKind(kind as u16)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ScriptLanguage {}

impl rowan::Language for ScriptLanguage {
    type Kind = ScriptKind;

    fn kind_from_raw(raw: rowan::SyntaxKind) -> ScriptKind {
        ScriptKind::from_u16(raw.0).unwrap_or(ScriptKind::ERROR)
    }

    fn kind_to_raw(kind: ScriptKind) -> rowan::SyntaxKind {
        kind.into()
    }
}

pub type SyntaxNode = rowan::SyntaxNode<ScriptLanguage>;
pub type SyntaxToken = rowan::SyntaxToken<ScriptLanguage>;
pub type SyntaxElement = rowan::SyntaxElement<ScriptLanguage>;

#[cfg(test)]
mod tests {
    use super::ScriptKind;

    #[test]
    fn all_matches_discriminants() {
        for (index, kind) in ScriptKind::ALL.iter().enumerate() {
            assert_eq!(usize::from(*kind as u16), index);
        }
    }
}
