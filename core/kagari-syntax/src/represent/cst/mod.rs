//! Concrete syntax tree for `.kirp`/`.kix`: syntax kinds, the rowan language and the parser.

mod parser;

pub use parser::{parse_file, parse_inline};

macro_rules! kinds {
    ($($(#[$meta:meta])* $name:ident),* $(,)?) => {
        /// Every token and node kind of the `.kirp`/`.kix` syntax tree.
        #[allow(non_camel_case_types)]
        #[repr(u16)]
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub enum RepresentKind {
            $($(#[$meta])* $name,)*
        }

        impl RepresentKind {
            /// All kinds, indexed by their raw value.
            pub const ALL: [RepresentKind; <[()]>::len(&[$(kinds!(@unit $name)),*])] =
                [$(RepresentKind::$name),*];
        }
    };
    (@unit $name:ident) => { () };
}

kinds! {
    // Trivia.
    WHITESPACE,
    /// `//…`, doc comments included.
    LINE_COMMENT,
    /// `/* … */`, nested.
    BLOCK_COMMENT,

    // Literals and names.
    IDENT,
    INT_NUMBER,
    DECIMAL_NUMBER,
    /// `"…"`, quotes included.
    STRING,

    // Keywords.
    FN_KW,
    STRUCT_KW,
    ENUM_KW,
    TYPE_KW,
    CONST_KW,
    MOD_KW,
    IMPL_KW,
    LET_KW,
    MATCH_KW,
    AS_KW,
    /// `self`
    SELF_KW,
    /// `Self`
    SELF_TYPE_KW,
    IMPORT_KW,
    TRUE_KW,
    FALSE_KW,

    // Punctuation.
    L_PAREN,
    R_PAREN,
    L_CURLY,
    R_CURLY,
    L_BRACK,
    R_BRACK,
    L_ANGLE,
    R_ANGLE,
    COMMA,
    SEMICOLON,
    COLON,
    COLON2,
    DOT,
    DOT2,
    DOT2EQ,
    THIN_ARROW,
    FAT_ARROW,
    EQ,
    EQ2,
    NEQ,
    LTEQ,
    GTEQ,
    PLUS,
    MINUS,
    STAR,
    SLASH,
    PERCENT,
    AMP,
    AMP2,
    PIPE,
    PIPE2,
    BANG,
    POUND,
    UNDERSCORE,
    /// Input the lexer does not recognise.
    ERROR_TOKEN,

    // Roots.
    SOURCE_FILE,
    INLINE_RENDER,

    // Items.
    ATTR,
    /// The balanced `( … )` arguments of an attribute.
    TOKEN_TREE,
    FN,
    PARAM_LIST,
    SELF_PARAM,
    PARAM,
    RET_TYPE,
    STRUCT,
    RECORD_FIELD_LIST,
    RECORD_FIELD,
    ENUM,
    VARIANT_LIST,
    VARIANT,
    TUPLE_FIELD_LIST,
    TYPE_ALIAS,
    CONST,
    MOD,
    IMPL,
    /// `{ Item* }` of a `mod` or `impl`.
    ITEM_LIST,
    ASSET_IMPORT,
    GENERIC_PARAM_LIST,
    GENERIC_PARAM,
    GENERIC_ARG_LIST,

    // Names, paths and types.
    NAME,
    NAME_REF,
    PATH,
    PATH_SEGMENT,
    PATH_TYPE,
    UNIT_TYPE,

    // Statements and expressions.
    BLOCK_EXPR,
    LET_STMT,
    EXPR_STMT,
    LITERAL,
    PATH_EXPR,
    RECORD_EXPR,
    RECORD_EXPR_FIELD_LIST,
    RECORD_EXPR_FIELD,
    PAREN_EXPR,
    UNIT_EXPR,
    MATCH_EXPR,
    MATCH_ARM_LIST,
    MATCH_ARM,
    CALL_EXPR,
    ARG_LIST,
    METHOD_CALL_EXPR,
    FIELD_EXPR,
    CAST_EXPR,
    BIN_EXPR,
    PREFIX_EXPR,

    // Patterns.
    OR_PAT,
    WILDCARD_PAT,
    LITERAL_PAT,
    RANGE_PAT,
    TUPLE_STRUCT_PAT,
    RECORD_PAT,
    RECORD_PAT_FIELD_LIST,
    RECORD_PAT_FIELD,
    REST_PAT,
    PATH_PAT,
    IDENT_PAT,

    /// Malformed input, kept for losslessness.
    ERROR,
}

impl RepresentKind {
    pub fn from_u16(raw: u16) -> Option<RepresentKind> {
        RepresentKind::ALL.get(usize::from(raw)).copied()
    }

    pub fn is_trivia(self) -> bool {
        matches!(
            self,
            RepresentKind::WHITESPACE | RepresentKind::LINE_COMMENT | RepresentKind::BLOCK_COMMENT
        )
    }

    /// The keyword spelled `text`, if any. `from` is contextual and therefore not a keyword.
    pub fn from_keyword(text: &str) -> Option<RepresentKind> {
        Some(match text {
            "fn" => RepresentKind::FN_KW,
            "struct" => RepresentKind::STRUCT_KW,
            "enum" => RepresentKind::ENUM_KW,
            "type" => RepresentKind::TYPE_KW,
            "const" => RepresentKind::CONST_KW,
            "mod" => RepresentKind::MOD_KW,
            "impl" => RepresentKind::IMPL_KW,
            "let" => RepresentKind::LET_KW,
            "match" => RepresentKind::MATCH_KW,
            "as" => RepresentKind::AS_KW,
            "self" => RepresentKind::SELF_KW,
            "Self" => RepresentKind::SELF_TYPE_KW,
            "import" => RepresentKind::IMPORT_KW,
            "true" => RepresentKind::TRUE_KW,
            "false" => RepresentKind::FALSE_KW,
            _ => return None,
        })
    }

    /// How the kind is written in "expected …" messages.
    pub fn describe(self) -> &'static str {
        use RepresentKind::*;
        match self {
            IDENT => "an identifier",
            INT_NUMBER | DECIMAL_NUMBER => "a number",
            STRING => "a string literal",
            FN_KW => "`fn`",
            STRUCT_KW => "`struct`",
            ENUM_KW => "`enum`",
            TYPE_KW => "`type`",
            CONST_KW => "`const`",
            MOD_KW => "`mod`",
            IMPL_KW => "`impl`",
            LET_KW => "`let`",
            MATCH_KW => "`match`",
            AS_KW => "`as`",
            SELF_KW => "`self`",
            SELF_TYPE_KW => "`Self`",
            IMPORT_KW => "`import`",
            TRUE_KW => "`true`",
            FALSE_KW => "`false`",
            L_PAREN => "`(`",
            R_PAREN => "`)`",
            L_CURLY => "`{`",
            R_CURLY => "`}`",
            L_BRACK => "`[`",
            R_BRACK => "`]`",
            L_ANGLE => "`<`",
            R_ANGLE => "`>`",
            COMMA => "`,`",
            SEMICOLON => "`;`",
            COLON => "`:`",
            COLON2 => "`::`",
            DOT => "`.`",
            DOT2 => "`..`",
            DOT2EQ => "`..=`",
            THIN_ARROW => "`->`",
            FAT_ARROW => "`=>`",
            EQ => "`=`",
            EQ2 => "`==`",
            NEQ => "`!=`",
            LTEQ => "`<=`",
            GTEQ => "`>=`",
            PLUS => "`+`",
            MINUS => "`-`",
            STAR => "`*`",
            SLASH => "`/`",
            PERCENT => "`%`",
            AMP => "`&`",
            AMP2 => "`&&`",
            PIPE => "`|`",
            PIPE2 => "`||`",
            BANG => "`!`",
            POUND => "`#`",
            UNDERSCORE => "`_`",
            _ => "a token",
        }
    }
}

/// The rowan language of `.kirp`/`.kix` trees.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum RepresentLanguage {}

impl rowan::Language for RepresentLanguage {
    type Kind = RepresentKind;

    fn kind_from_raw(raw: rowan::SyntaxKind) -> RepresentKind {
        RepresentKind::from_u16(raw.0).unwrap_or(RepresentKind::ERROR)
    }

    fn kind_to_raw(kind: RepresentKind) -> rowan::SyntaxKind {
        rowan::SyntaxKind(kind as u16)
    }
}

pub type SyntaxNode = rowan::SyntaxNode<RepresentLanguage>;
pub type SyntaxToken = rowan::SyntaxToken<RepresentLanguage>;
pub type SyntaxElement = rowan::SyntaxElement<RepresentLanguage>;
