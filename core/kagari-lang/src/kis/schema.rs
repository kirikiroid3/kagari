//! The `.kis` element and attribute schema.

/// Every element known to the game and system scopes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Tag {
    Kagari,
    System,
    Game,
    KagariGame,
    KagariSystem,
    Include,
    Import,
    DeclareEnum,
    Enum,
    VarBool,
    VarChar,
    VarEnum,
    VarInt,
    Template,
    Slot,
    X,
    Snip,
    Scene,
    S,
    M,
    R,
    Represent,
    Choice,
    ChoiceArm,
    Label,
    Goto,
    Call,
    Return,
    Match,
    Cases,
    Case,
    Do,
    Set,
    Add,
    Sub,
    MetaSet,
    MetaBool,
    MetaChar,
    MetaEnum,
    MetaInt,
}

impl Tag {
    pub const ALL: [Tag; 40] = [
        Tag::Kagari,
        Tag::System,
        Tag::Game,
        Tag::KagariGame,
        Tag::KagariSystem,
        Tag::Include,
        Tag::Import,
        Tag::DeclareEnum,
        Tag::Enum,
        Tag::VarBool,
        Tag::VarChar,
        Tag::VarEnum,
        Tag::VarInt,
        Tag::Template,
        Tag::Slot,
        Tag::X,
        Tag::Snip,
        Tag::Scene,
        Tag::S,
        Tag::M,
        Tag::R,
        Tag::Represent,
        Tag::Choice,
        Tag::ChoiceArm,
        Tag::Label,
        Tag::Goto,
        Tag::Call,
        Tag::Return,
        Tag::Match,
        Tag::Cases,
        Tag::Case,
        Tag::Do,
        Tag::Set,
        Tag::Add,
        Tag::Sub,
        Tag::MetaSet,
        Tag::MetaBool,
        Tag::MetaChar,
        Tag::MetaEnum,
        Tag::MetaInt,
    ];

    /// The element name as written.
    pub fn name(self) -> &'static str {
        match self {
            Tag::Kagari => "kagari",
            Tag::System => "system",
            Tag::Game => "game",
            Tag::KagariGame => "kagari-game",
            Tag::KagariSystem => "kagari-system",
            Tag::Include => "include",
            Tag::Import => "import",
            Tag::DeclareEnum => "declare-enum",
            Tag::Enum => "enum",
            Tag::VarBool => "var-bool",
            Tag::VarChar => "var-char",
            Tag::VarEnum => "var-enum",
            Tag::VarInt => "var-int",
            Tag::Template => "template",
            Tag::Slot => "slot",
            Tag::X => "x",
            Tag::Snip => "snip",
            Tag::Scene => "scene",
            Tag::S => "s",
            Tag::M => "m",
            Tag::R => "r",
            Tag::Represent => "represent",
            Tag::Choice => "choice",
            Tag::ChoiceArm => "choice-arm",
            Tag::Label => "label",
            Tag::Goto => "goto",
            Tag::Call => "call",
            Tag::Return => "return",
            Tag::Match => "match",
            Tag::Cases => "cases",
            Tag::Case => "case",
            Tag::Do => "do",
            Tag::Set => "set",
            Tag::Add => "add",
            Tag::Sub => "sub",
            Tag::MetaSet => "meta-set",
            Tag::MetaBool => "meta-bool",
            Tag::MetaChar => "meta-char",
            Tag::MetaEnum => "meta-enum",
            Tag::MetaInt => "meta-int",
        }
    }

    /// Whether the element declares a variable (`<var-*>`).
    pub fn is_var(self) -> bool {
        matches!(
            self,
            Tag::VarBool | Tag::VarChar | Tag::VarEnum | Tag::VarInt
        )
    }

    /// Whether the element declares a meta field (`<meta-*>`, but not `<meta-set>`).
    pub fn is_meta(self) -> bool {
        matches!(
            self,
            Tag::MetaBool | Tag::MetaChar | Tag::MetaEnum | Tag::MetaInt
        )
    }
}

/// The tag called `name`.
pub fn tag(name: &str) -> Option<Tag> {
    Tag::ALL.into_iter().find(|tag| tag.name() == name)
}

/// The attributes of `tag` and whether each is required. `<match>` lists `var` and `vars` as
/// optional; exactly one of the two must be present, which the lowering checks. `<return>` lists
/// `var` and `literal`; at most one may be present.
pub fn attrs(tag: Tag) -> &'static [(&'static str, bool)] {
    match tag {
        Tag::Include | Tag::Import | Tag::Represent => &[("src", true)],
        Tag::DeclareEnum | Tag::Template => &[("name", true)],
        Tag::Scene => &[("name", true), ("returns", false), ("entry", false)],
        Tag::Enum => &[("name", true), ("repr", true)],
        Tag::VarBool | Tag::VarChar => &[("name", true), ("init", true), ("from-meta", false)],
        Tag::VarEnum => &[
            ("name", true),
            ("type", true),
            ("init", true),
            ("from-meta", false),
        ],
        Tag::VarInt => &[
            ("name", true),
            ("min", true),
            ("max", true),
            ("init", true),
            ("from-meta", false),
        ],
        Tag::Slot => &[("pos", true)],
        Tag::X => &[("tpl", true)],
        Tag::M => &[("who", true), ("voice", false)],
        Tag::Choice | Tag::Label => &[("id", true)],
        Tag::ChoiceArm => &[("id", true), ("text", true)],
        Tag::Goto => &[("label", true)],
        Tag::Call => &[("scene", true), ("assign", false)],
        Tag::Return => &[("var", false), ("literal", false)],
        Tag::Match => &[("var", false), ("vars", false)],
        Tag::Case => &[("val", true)],
        Tag::Set | Tag::Add | Tag::Sub | Tag::MetaSet => &[("var", true), ("val", true)],
        Tag::MetaBool | Tag::MetaChar => &[("name", true), ("init", true)],
        Tag::MetaEnum => &[("name", true), ("type", true), ("init", true)],
        Tag::MetaInt => &[("name", true), ("min", true), ("max", true), ("init", true)],
        Tag::Snip
        | Tag::S
        | Tag::R
        | Tag::Cases
        | Tag::Do
        | Tag::Kagari
        | Tag::System
        | Tag::Game
        | Tag::KagariGame
        | Tag::KagariSystem => &[],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_round_trip() {
        for t in Tag::ALL {
            assert_eq!(tag(t.name()), Some(t));
        }
        assert_eq!(tag("foo"), None);
    }
}
