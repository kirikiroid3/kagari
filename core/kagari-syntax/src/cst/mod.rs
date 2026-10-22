//! Concrete syntax tree: syntax kinds, the rowan language and the parser.

pub(crate) mod parser;

macro_rules! kinds {
    ($($(#[$meta:meta])* $name:ident),* $(,)?) => {
        /// Every token and node kind of the Kagari syntax tree.
        #[allow(non_camel_case_types)]
        #[repr(u16)]
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub enum SyntaxKind {
            $($(#[$meta])* $name,)*
        }

        impl SyntaxKind {
            /// All kinds, indexed by their raw value.
            pub const ALL: [SyntaxKind; <[()]>::len(&[$(kinds!(@unit $name)),*])] =
                [$(SyntaxKind::$name),*];
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
    /// `'name`
    LABEL,
    UNDERSCORE,
    /// Input the lexer does not recognise.
    ERROR_TOKEN,

    // Strict keywords.
    AS_KW,
    BREAK_KW,
    CONST_KW,
    CONTINUE_KW,
    CRATE_KW,
    ELSE_KW,
    ENUM_KW,
    FALSE_KW,
    FN_KW,
    FOR_KW,
    IF_KW,
    IMPL_KW,
    IN_KW,
    LET_KW,
    LOOP_KW,
    MATCH_KW,
    MOD_KW,
    MUT_KW,
    PUB_KW,
    RETURN_KW,
    /// `self`
    SELF_KW,
    /// `Self`
    SELF_TYPE_KW,
    STATIC_KW,
    STRUCT_KW,
    SUPER_KW,
    TRAIT_KW,
    TRUE_KW,
    TYPE_KW,
    USE_KW,
    WHILE_KW,

    // Contextual keywords: lexed as `IDENT`, re-kinded by the parser.
    PUSHDOWN_KW,
    SCENE_KW,

    // Single-character punctuation (the only punctuation the lexer emits).
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
    DOT,
    EQ,
    BANG,
    PLUS,
    MINUS,
    STAR,
    SLASH,
    PERCENT,
    AMP,
    PIPE,
    CARET,
    POUND,
    QUESTION,

    // Composite punctuation, glued by the parser from adjacent single characters.
    COLON2,
    THIN_ARROW,
    FAT_ARROW,
    EQ2,
    NEQ,
    LTEQ,
    GTEQ,
    AMP2,
    PIPE2,
    SHL,
    SHR,
    PLUS_EQ,
    MINUS_EQ,
    STAR_EQ,
    SLASH_EQ,
    PERCENT_EQ,
    CARET_EQ,
    AMP_EQ,
    PIPE_EQ,
    SHL_EQ,
    SHR_EQ,
    DOT2,
    DOT2EQ,

    // Root.
    SOURCE_FILE,

    // Items.
    ATTR,
    /// The balanced delimited arguments of an attribute.
    TOKEN_TREE,
    VISIBILITY,
    FN,
    PARAM_LIST,
    SELF_PARAM,
    PARAM,
    RET_TYPE,
    STRUCT,
    RECORD_FIELD_LIST,
    RECORD_FIELD,
    TUPLE_FIELD_LIST,
    TUPLE_FIELD,
    ENUM,
    VARIANT_LIST,
    VARIANT,
    TYPE_ALIAS,
    CONST,
    STATIC,
    MOD,
    /// `{ Item* }` of an inline `mod`.
    ITEM_LIST,
    USE,
    USE_TREE,
    USE_TREE_LIST,
    RENAME,
    IMPL,
    TRAIT,
    /// `{ AssocItem* }` of an `impl` or `trait`.
    ASSOC_ITEM_LIST,
    GENERIC_PARAM_LIST,
    TYPE_PARAM,
    CONST_PARAM,
    TYPE_BOUND_LIST,
    TYPE_BOUND,

    // Names and paths.
    NAME,
    NAME_REF,
    PATH,
    PATH_SEGMENT,
    GENERIC_ARG_LIST,
    TYPE_ARG,
    CONST_ARG,

    // Types.
    PATH_TYPE,
    TUPLE_TYPE,
    PAREN_TYPE,
    ARRAY_TYPE,
    REF_TYPE,
    INFER_TYPE,
    FN_PTR_TYPE,
    CLOSURE_TYPE,

    // Patterns.
    WILDCARD_PAT,
    IDENT_PAT,
    PATH_PAT,
    LITERAL_PAT,
    RANGE_PAT,
    TUPLE_PAT,
    TUPLE_STRUCT_PAT,
    RECORD_PAT,
    RECORD_PAT_FIELD_LIST,
    RECORD_PAT_FIELD,
    REST_PAT,
    OR_PAT,
    PAREN_PAT,

    // Statements.
    BLOCK_EXPR,
    LET_STMT,
    EXPR_STMT,

    // Expressions.
    LITERAL,
    PATH_EXPR,
    PAREN_EXPR,
    TUPLE_EXPR,
    ARRAY_EXPR,
    RECORD_EXPR,
    RECORD_EXPR_FIELD_LIST,
    RECORD_EXPR_FIELD,
    IF_EXPR,
    /// `let PAT = EXPR` in an `if`/`while` condition.
    LET_EXPR,
    MATCH_EXPR,
    MATCH_ARM_LIST,
    MATCH_ARM,
    MATCH_GUARD,
    LOOP_EXPR,
    WHILE_EXPR,
    FOR_EXPR,
    /// `'name:` before a loop.
    LABEL_DECL,
    BREAK_EXPR,
    CONTINUE_EXPR,
    RETURN_EXPR,
    CALL_EXPR,
    ARG_LIST,
    METHOD_CALL_EXPR,
    FIELD_EXPR,
    INDEX_EXPR,
    TRY_EXPR,
    CAST_EXPR,
    PREFIX_EXPR,
    REF_EXPR,
    BIN_EXPR,
    RANGE_EXPR,
    CLOSURE_EXPR,

    // Forms.
    /// `name!(…)`, `name![…]` or `name!{…}` other than `t![…]`.
    FORM_EXPR,
    /// `key = expr` inside a form.
    FORM_ATTR,
    /// `text [if guard] => value` inside a form.
    FORM_ARM,
    FORM_GUARD,
    /// `t![…]`
    TEXT_FORM,
    TEXT_STRING,
    /// `["base"]("reading" | …)`
    TEXT_RUBY,
    RUBY_READINGS,
    /// `![base](effect)`
    TEXT_EFFECT,
    /// `{expr}`
    TEXT_INTERP,

    /// Malformed input, kept for losslessness.
    ERROR,
}

impl SyntaxKind {
    pub fn from_u16(raw: u16) -> Option<SyntaxKind> {
        SyntaxKind::ALL.get(usize::from(raw)).copied()
    }

    pub fn is_trivia(self) -> bool {
        matches!(
            self,
            SyntaxKind::WHITESPACE | SyntaxKind::LINE_COMMENT | SyntaxKind::BLOCK_COMMENT
        )
    }

    /// The strict keyword spelled `text`, if any. Contextual keywords are not included.
    pub fn from_keyword(text: &str) -> Option<SyntaxKind> {
        use SyntaxKind::*;
        Some(match text {
            "as" => AS_KW,
            "break" => BREAK_KW,
            "const" => CONST_KW,
            "continue" => CONTINUE_KW,
            "crate" => CRATE_KW,
            "else" => ELSE_KW,
            "enum" => ENUM_KW,
            "false" => FALSE_KW,
            "fn" => FN_KW,
            "for" => FOR_KW,
            "if" => IF_KW,
            "impl" => IMPL_KW,
            "in" => IN_KW,
            "let" => LET_KW,
            "loop" => LOOP_KW,
            "match" => MATCH_KW,
            "mod" => MOD_KW,
            "mut" => MUT_KW,
            "pub" => PUB_KW,
            "return" => RETURN_KW,
            "self" => SELF_KW,
            "Self" => SELF_TYPE_KW,
            "static" => STATIC_KW,
            "struct" => STRUCT_KW,
            "super" => SUPER_KW,
            "trait" => TRAIT_KW,
            "true" => TRUE_KW,
            "type" => TYPE_KW,
            "use" => USE_KW,
            "while" => WHILE_KW,
            _ => return None,
        })
    }

    /// The single-character parts of a composite punctuation kind; `None` for other kinds.
    pub fn composite_parts(self) -> Option<&'static [SyntaxKind]> {
        use SyntaxKind::*;
        Some(match self {
            COLON2 => &[COLON, COLON],
            THIN_ARROW => &[MINUS, R_ANGLE],
            FAT_ARROW => &[EQ, R_ANGLE],
            EQ2 => &[EQ, EQ],
            NEQ => &[BANG, EQ],
            LTEQ => &[L_ANGLE, EQ],
            GTEQ => &[R_ANGLE, EQ],
            AMP2 => &[AMP, AMP],
            PIPE2 => &[PIPE, PIPE],
            SHL => &[L_ANGLE, L_ANGLE],
            SHR => &[R_ANGLE, R_ANGLE],
            PLUS_EQ => &[PLUS, EQ],
            MINUS_EQ => &[MINUS, EQ],
            STAR_EQ => &[STAR, EQ],
            SLASH_EQ => &[SLASH, EQ],
            PERCENT_EQ => &[PERCENT, EQ],
            CARET_EQ => &[CARET, EQ],
            AMP_EQ => &[AMP, EQ],
            PIPE_EQ => &[PIPE, EQ],
            SHL_EQ => &[L_ANGLE, L_ANGLE, EQ],
            SHR_EQ => &[R_ANGLE, R_ANGLE, EQ],
            DOT2 => &[DOT, DOT],
            DOT2EQ => &[DOT, DOT, EQ],
            _ => return None,
        })
    }

    /// How the kind is written in "expected …" messages.
    pub fn describe(self) -> &'static str {
        use SyntaxKind::*;
        match self {
            IDENT => "an identifier",
            INT_NUMBER | DECIMAL_NUMBER => "a number",
            STRING => "a string literal",
            LABEL => "a label",
            UNDERSCORE => "`_`",
            AS_KW => "`as`",
            BREAK_KW => "`break`",
            CONST_KW => "`const`",
            CONTINUE_KW => "`continue`",
            CRATE_KW => "`crate`",
            ELSE_KW => "`else`",
            ENUM_KW => "`enum`",
            FALSE_KW => "`false`",
            FN_KW => "`fn`",
            FOR_KW => "`for`",
            IF_KW => "`if`",
            IMPL_KW => "`impl`",
            IN_KW => "`in`",
            LET_KW => "`let`",
            LOOP_KW => "`loop`",
            MATCH_KW => "`match`",
            MOD_KW => "`mod`",
            MUT_KW => "`mut`",
            PUB_KW => "`pub`",
            RETURN_KW => "`return`",
            SELF_KW => "`self`",
            SELF_TYPE_KW => "`Self`",
            STATIC_KW => "`static`",
            STRUCT_KW => "`struct`",
            SUPER_KW => "`super`",
            TRAIT_KW => "`trait`",
            TRUE_KW => "`true`",
            TYPE_KW => "`type`",
            USE_KW => "`use`",
            WHILE_KW => "`while`",
            PUSHDOWN_KW => "`pushdown`",
            SCENE_KW => "`scene`",
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
            DOT => "`.`",
            EQ => "`=`",
            BANG => "`!`",
            PLUS => "`+`",
            MINUS => "`-`",
            STAR => "`*`",
            SLASH => "`/`",
            PERCENT => "`%`",
            AMP => "`&`",
            PIPE => "`|`",
            CARET => "`^`",
            POUND => "`#`",
            QUESTION => "`?`",
            COLON2 => "`::`",
            THIN_ARROW => "`->`",
            FAT_ARROW => "`=>`",
            EQ2 => "`==`",
            NEQ => "`!=`",
            LTEQ => "`<=`",
            GTEQ => "`>=`",
            AMP2 => "`&&`",
            PIPE2 => "`||`",
            SHL => "`<<`",
            SHR => "`>>`",
            PLUS_EQ => "`+=`",
            MINUS_EQ => "`-=`",
            STAR_EQ => "`*=`",
            SLASH_EQ => "`/=`",
            PERCENT_EQ => "`%=`",
            CARET_EQ => "`^=`",
            AMP_EQ => "`&=`",
            PIPE_EQ => "`|=`",
            SHL_EQ => "`<<=`",
            SHR_EQ => "`>>=`",
            DOT2 => "`..`",
            DOT2EQ => "`..=`",
            _ => "a token",
        }
    }
}

/// The rowan language of Kagari trees.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum KagariLanguage {}

impl rowan::Language for KagariLanguage {
    type Kind = SyntaxKind;

    fn kind_from_raw(raw: rowan::SyntaxKind) -> SyntaxKind {
        SyntaxKind::from_u16(raw.0).unwrap_or(SyntaxKind::ERROR)
    }

    fn kind_to_raw(kind: SyntaxKind) -> rowan::SyntaxKind {
        rowan::SyntaxKind(kind as u16)
    }
}

pub type SyntaxNode = rowan::SyntaxNode<KagariLanguage>;
pub type SyntaxToken = rowan::SyntaxToken<KagariLanguage>;
pub type SyntaxElement = rowan::SyntaxElement<KagariLanguage>;
