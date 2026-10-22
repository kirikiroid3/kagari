//! Forms: `name!(…)`, `name![…]`, `name!{…}` and `t![…]` text.

use rowan::ast::{AstChildren, AstNode};

use super::{Expr, NameRef, child, child_after, children, token, unescape_string};
use crate::cst::{KagariLanguage, SyntaxKind, SyntaxNode, SyntaxToken};

ast_node!(FormExpr, FORM_EXPR);
ast_node!(FormAttr, FORM_ATTR);
ast_node!(FormArm, FORM_ARM);
ast_node!(FormGuard, FORM_GUARD);
ast_node!(TextForm, TEXT_FORM);
ast_node!(TextString, TEXT_STRING);
ast_node!(TextRuby, TEXT_RUBY);
ast_node!(RubyReadings, RUBY_READINGS);
ast_node!(TextEffect, TEXT_EFFECT);
ast_node!(TextInterp, TEXT_INTERP);

ast_enum!(
    /// An item of `t![…]`.
    TextItem {
        TextString(TEXT_STRING),
        TextRuby(TEXT_RUBY),
        TextEffect(TEXT_EFFECT),
        TextInterp(TEXT_INTERP),
    }
);

ast_enum!(
    /// The base of a text effect.
    EffectBase {
        TextString(TEXT_STRING),
        TextRuby(TEXT_RUBY),
    }
);

/// An entry of a comma-separated form.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum FormEntry {
    /// `key = value`
    FormAttr(FormAttr),
    /// `text [if guard] => value`
    FormArm(FormArm),
    /// Any other element.
    Expr(Expr),
}

impl AstNode for FormEntry {
    type Language = KagariLanguage;

    fn can_cast(kind: SyntaxKind) -> bool {
        matches!(kind, SyntaxKind::FORM_ATTR | SyntaxKind::FORM_ARM) || Expr::can_cast(kind)
    }

    fn cast(node: SyntaxNode) -> Option<Self> {
        Some(match node.kind() {
            SyntaxKind::FORM_ATTR => FormEntry::FormAttr(FormAttr(node)),
            SyntaxKind::FORM_ARM => FormEntry::FormArm(FormArm(node)),
            _ => FormEntry::Expr(Expr::cast(node)?),
        })
    }

    fn syntax(&self) -> &SyntaxNode {
        match self {
            FormEntry::FormAttr(it) => it.syntax(),
            FormEntry::FormArm(it) => it.syntax(),
            FormEntry::Expr(it) => it.syntax(),
        }
    }
}

/// The delimiter of a form.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FormDelimiter {
    /// `( … )`
    Paren,
    /// `[ … ]`
    Bracket,
    /// `{ … }`
    Brace,
}

impl FormDelimiter {
    pub fn open(self) -> char {
        match self {
            FormDelimiter::Paren => '(',
            FormDelimiter::Bracket => '[',
            FormDelimiter::Brace => '{',
        }
    }

    pub fn close(self) -> char {
        match self {
            FormDelimiter::Paren => ')',
            FormDelimiter::Bracket => ']',
            FormDelimiter::Brace => '}',
        }
    }
}

/// The delimiter right after the `!` of a form node.
fn form_delimiter(node: &SyntaxNode) -> Option<FormDelimiter> {
    node.children_with_tokens()
        .filter_map(|it| it.into_token())
        .skip_while(|t| t.kind() != SyntaxKind::BANG)
        .skip(1)
        .find(|t| !t.kind().is_trivia())
        .and_then(|t| match t.kind() {
            SyntaxKind::L_PAREN => Some(FormDelimiter::Paren),
            SyntaxKind::L_BRACK => Some(FormDelimiter::Bracket),
            SyntaxKind::L_CURLY => Some(FormDelimiter::Brace),
            _ => None,
        })
}

impl FormExpr {
    pub fn name_ref(&self) -> Option<NameRef> {
        child(&self.0)
    }

    pub fn name_text(&self) -> Option<String> {
        self.name_ref().map(|name| name.text())
    }

    pub fn delimiter(&self) -> Option<FormDelimiter> {
        form_delimiter(&self.0)
    }

    pub fn entries(&self) -> AstChildren<FormEntry> {
        children(&self.0)
    }
}

impl FormAttr {
    pub fn value(&self) -> Option<Expr> {
        child(&self.0)
    }
}

impl FormArm {
    /// The option text before `if`/`=>`.
    pub fn text(&self) -> Option<Expr> {
        self.0.first_child().and_then(Expr::cast)
    }

    pub fn guard(&self) -> Option<FormGuard> {
        child(&self.0)
    }

    /// The value after `=>`.
    pub fn value(&self) -> Option<Expr> {
        child_after(&self.0, SyntaxKind::FAT_ARROW)
    }
}

impl FormGuard {
    pub fn condition(&self) -> Option<Expr> {
        child(&self.0)
    }
}

impl TextForm {
    pub fn name_ref(&self) -> Option<NameRef> {
        child(&self.0)
    }

    pub fn items(&self) -> AstChildren<TextItem> {
        children(&self.0)
    }
}

impl TextString {
    pub fn token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::STRING)
    }

    /// The unescaped text; `None` if the literal is malformed.
    pub fn value(&self) -> Option<String> {
        unescape_string(self.token()?.text()).ok()
    }
}

impl TextRuby {
    /// The base string literal (quotes included).
    pub fn base(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::STRING)
    }

    pub fn readings(&self) -> Option<RubyReadings> {
        child(&self.0)
    }
}

impl RubyReadings {
    /// The reading string literals (quotes included).
    pub fn strings(&self) -> impl Iterator<Item = SyntaxToken> + use<> {
        self.0
            .children_with_tokens()
            .filter_map(|it| it.into_token())
            .filter(|t| t.kind() == SyntaxKind::STRING)
    }
}

impl TextEffect {
    pub fn base(&self) -> Option<EffectBase> {
        child(&self.0)
    }

    /// The effect expression in `( … )`.
    pub fn effect(&self) -> Option<Expr> {
        child(&self.0)
    }
}

impl TextInterp {
    pub fn expr(&self) -> Option<Expr> {
        child(&self.0)
    }
}
