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
    SettingInt,
    SettingEnum,
    SettingBool,
    SaveSlots,
    Sysfn,
    SysfnArg,
    SysfnParam,
    SysfnReturn,
    SysfnCall,
    MetafnStart,
    MetafnLoad,
    MetafnSave,
    MetafnAdvance,
    MetafnSelect,
    MetafnQuit,
    InterfaceCall,
    InterfaceReturn,
    InterfaceGoto,
    InterfaceFocusMove,
    InterfaceFocusActivate,
    VarI16,
    VarVec2,
    Page,
    Dialog,
    GameMode,
    Layer,
    GameScreen,
    OnExit,
    Button,
    Slider,
    Toggle,
    Select,
    Row,
    Col,
    TextFrame,
    BacklogLine,
    ForSlots,
    ForHistory,
    ForChoices,
}

impl Tag {
    pub const ALL: [Tag; 79] = [
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
        Tag::SettingInt,
        Tag::SettingEnum,
        Tag::SettingBool,
        Tag::SaveSlots,
        Tag::Sysfn,
        Tag::SysfnArg,
        Tag::SysfnParam,
        Tag::SysfnReturn,
        Tag::SysfnCall,
        Tag::MetafnStart,
        Tag::MetafnLoad,
        Tag::MetafnSave,
        Tag::MetafnAdvance,
        Tag::MetafnSelect,
        Tag::MetafnQuit,
        Tag::InterfaceCall,
        Tag::InterfaceReturn,
        Tag::InterfaceGoto,
        Tag::InterfaceFocusMove,
        Tag::InterfaceFocusActivate,
        Tag::VarI16,
        Tag::VarVec2,
        Tag::Page,
        Tag::Dialog,
        Tag::GameMode,
        Tag::Layer,
        Tag::GameScreen,
        Tag::OnExit,
        Tag::Button,
        Tag::Slider,
        Tag::Toggle,
        Tag::Select,
        Tag::Row,
        Tag::Col,
        Tag::TextFrame,
        Tag::BacklogLine,
        Tag::ForSlots,
        Tag::ForHistory,
        Tag::ForChoices,
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
            Tag::SettingInt => "setting-int",
            Tag::SettingEnum => "setting-enum",
            Tag::SettingBool => "setting-bool",
            Tag::SaveSlots => "save-slots",
            Tag::Sysfn => "sysfn",
            Tag::SysfnArg => "sysfn-arg",
            Tag::SysfnParam => "sysfn-param",
            Tag::SysfnReturn => "sysfn-return",
            Tag::SysfnCall => "sysfn-call",
            Tag::MetafnStart => "metafn-start",
            Tag::MetafnLoad => "metafn-load",
            Tag::MetafnSave => "metafn-save",
            Tag::MetafnAdvance => "metafn-advance",
            Tag::MetafnSelect => "metafn-select",
            Tag::MetafnQuit => "metafn-quit",
            Tag::InterfaceCall => "interface-call",
            Tag::InterfaceReturn => "interface-return",
            Tag::InterfaceGoto => "interface-goto",
            Tag::InterfaceFocusMove => "interface-focus-move",
            Tag::InterfaceFocusActivate => "interface-focus-activate",
            Tag::VarI16 => "var-i16",
            Tag::VarVec2 => "var-vec2",
            Tag::Page => "page",
            Tag::Dialog => "dialog",
            Tag::GameMode => "game-mode",
            Tag::Layer => "layer",
            Tag::GameScreen => "game-screen",
            Tag::OnExit => "on-exit",
            Tag::Button => "button",
            Tag::Slider => "slider",
            Tag::Toggle => "toggle",
            Tag::Select => "select",
            Tag::Row => "row",
            Tag::Col => "col",
            Tag::TextFrame => "text-frame",
            Tag::BacklogLine => "backlog-line",
            Tag::ForSlots => "for-slots",
            Tag::ForHistory => "for-history",
            Tag::ForChoices => "for-choices",
        }
    }

    /// Whether the element declares a variable (`<var-*>`).
    pub fn is_var(self) -> bool {
        matches!(
            self,
            Tag::VarBool | Tag::VarChar | Tag::VarEnum | Tag::VarInt
        )
    }

    /// Whether the element declares a system function local: a game `<var-*>`, `<var-i16>`
    /// or `<var-vec2>`.
    pub fn is_sys_var(self) -> bool {
        self.is_var() || matches!(self, Tag::VarI16 | Tag::VarVec2)
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
        Tag::MetaBool | Tag::MetaChar | Tag::SettingBool | Tag::VarI16 | Tag::VarVec2 => {
            &[("name", true), ("init", true)]
        }
        Tag::MetaEnum | Tag::SettingEnum => &[("name", true), ("type", true), ("init", true)],
        Tag::MetaInt | Tag::SettingInt => {
            &[("name", true), ("min", true), ("max", true), ("init", true)]
        }
        Tag::System => &[("start", true)],
        Tag::SaveSlots => &[("count", true)],
        Tag::Sysfn => &[("name", true), ("args", false), ("return", false)],
        Tag::SysfnArg => &[("position", true), ("var", true)],
        Tag::SysfnParam => &[("var", false), ("literal", false)],
        Tag::SysfnReturn => &[("val", false)],
        Tag::SysfnCall => &[("name", true), ("save", false)],
        Tag::MetafnStart | Tag::MetafnLoad | Tag::MetafnSave | Tag::MetafnSelect => {
            &[("param", true)]
        }
        Tag::InterfaceCall | Tag::InterfaceGoto => &[("page", true)],
        Tag::InterfaceFocusMove => &[("direction", false), ("target-idx", false)],
        Tag::Page => &[
            ("name", true),
            ("root", false),
            ("requires", false),
            ("on-keyboard", false),
            ("on-controller", false),
        ],
        Tag::Dialog => &[
            ("name", true),
            ("requires", false),
            ("on-keyboard", false),
            ("on-controller", false),
        ],
        Tag::GameMode => &[
            ("name", true),
            ("on-keyboard", false),
            ("on-controller", false),
        ],
        Tag::Layer => &[("name", true), ("style", false)],
        Tag::Button | Tag::TextFrame => &[("style", false), ("on-click", false)],
        Tag::Slider | Tag::Toggle | Tag::Select => &[("style", false), ("bind", true)],
        Tag::Row | Tag::Col | Tag::BacklogLine => &[("style", false)],
        Tag::Snip
        | Tag::S
        | Tag::R
        | Tag::Cases
        | Tag::Do
        | Tag::Kagari
        | Tag::Game
        | Tag::KagariGame
        | Tag::KagariSystem
        | Tag::MetafnAdvance
        | Tag::MetafnQuit
        | Tag::InterfaceReturn
        | Tag::InterfaceFocusActivate
        | Tag::GameScreen
        | Tag::OnExit
        | Tag::ForSlots
        | Tag::ForHistory
        | Tag::ForChoices => &[],
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
