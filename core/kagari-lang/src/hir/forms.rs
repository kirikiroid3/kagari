//! Desugaring of forms: `s!(…)` and `choice!{…}` become `push_step` calls, `t![…]` becomes
//! calls of the `std::narrative::text` builtins. Synthesized nodes get the whole form's range.

use kagari_syntax::TextRange;
use kagari_syntax::ast::{self, AstNode, HasName};

use super::lower::Lowerer;
use super::{ArrayExpr, Expr, ExprId, LangItem, Literal, Name, Path};
use crate::diagnostic::DiagnosticCode;

const UI_FORMS: &[&str] = &[
    "column",
    "row",
    "layer",
    "text",
    "image",
    "game_screen",
    "button",
    "slider",
    "toggle",
    "select",
];

/// The attributes and elements of a form, in source order.
struct Entries {
    attrs: Vec<(Name, Option<ast::Expr>)>,
    elements: Vec<ast::Expr>,
    arms: Vec<ast::FormArm>,
}

fn entries(form: &ast::FormExpr) -> Entries {
    let mut out = Entries {
        attrs: Vec::new(),
        elements: Vec::new(),
        arms: Vec::new(),
    };
    for entry in form.entries() {
        match entry {
            ast::FormEntry::FormAttr(attr) => {
                if let Some(name) = attr.name() {
                    out.attrs.push((Name::new(name.text()), attr.value()));
                }
            }
            ast::FormEntry::FormArm(arm) => out.arms.push(arm),
            ast::FormEntry::Expr(expr) => out.elements.push(expr),
        }
    }
    out
}

impl Lowerer<'_> {
    fn lang_path(&mut self, item: LangItem, range: TextRange) -> ExprId {
        self.alloc(Expr::Path(Path::lang(item)), range)
    }

    fn lang_call(&mut self, item: LangItem, args: Vec<ExprId>, range: TextRange) -> ExprId {
        let callee = self.lang_path(item, range);
        self.alloc(Expr::Call { callee, args }, range)
    }

    /// `Some(value)` when the attribute is present, `None` otherwise.
    fn optional(&mut self, value: Option<ExprId>, range: TextRange) -> ExprId {
        match value {
            Some(value) => self.lang_call(LangItem::OptionSome, vec![value], range),
            None => self.lang_path(LangItem::OptionNone, range),
        }
    }

    fn attr(&mut self, entries: &Entries, key: &str, range: TextRange) -> Option<ExprId> {
        let (_, value) = entries.attrs.iter().find(|(name, _)| name == key)?;
        Some(self.lower_expr_opt(value.clone(), range))
    }

    pub(super) fn lower_form(&mut self, form: &ast::FormExpr) -> ExprId {
        let range = form.syntax().text_range();
        let Some(name) = form.name_text() else {
            return self.missing(range);
        };
        match name.as_str() {
            "s" => self.lower_say(form, range),
            "choice" => self.lower_choice(form, range),
            name if UI_FORMS.contains(&name) => {
                self.diag(
                    range,
                    DiagnosticCode::LowerUiForm,
                    "UI forms are not designed yet".to_owned(),
                );
                self.missing(range)
            }
            _ => self.missing(range),
        }
    }

    fn lower_say(&mut self, form: &ast::FormExpr, range: TextRange) -> ExprId {
        let entries = entries(form);
        let who = self.attr(&entries, "who", range);
        let voice = self.attr(&entries, "voice", range);
        let render = self.attr(&entries, "render", range);
        let text = entries
            .elements
            .first()
            .map(|element| self.lower_expr(element.clone()));
        let fields = vec![
            (Name::new_static("who"), self.optional(who, range)),
            (Name::new_static("voice"), self.optional(voice, range)),
            (Name::new_static("render"), self.optional(render, range)),
            (Name::new_static("text"), self.optional(text, range)),
        ];
        let say = self.alloc(
            Expr::Record {
                path: Path::lang(LangItem::Say),
                fields,
            },
            range,
        );
        self.lang_call(LangItem::PushStep, vec![say], range)
    }

    fn lower_choice(&mut self, form: &ast::FormExpr, range: TextRange) -> ExprId {
        let entries = entries(form);
        let id = match self.attr(&entries, "id", range) {
            Some(id) => id,
            None => self.missing(range),
        };
        let render = self.attr(&entries, "render", range);
        let render = self.optional(render, range);
        let mut arms = Vec::new();
        for arm in &entries.arms {
            let text = self.lower_expr_opt(arm.text(), range);
            let shown = match arm.guard() {
                Some(guard) => self.lower_expr_opt(guard.condition(), range),
                None => self.alloc(Expr::Literal(Literal::Bool(true)), range),
            };
            let value = self.lower_expr_opt(arm.value(), range);
            arms.push(self.alloc(
                Expr::Record {
                    path: Path::lang(LangItem::ChoiceArm),
                    fields: vec![
                        (Name::new_static("text"), text),
                        (Name::new_static("shown"), shown),
                        (Name::new_static("value"), value),
                    ],
                },
                range,
            ));
        }
        let arms = self.alloc(Expr::Array(ArrayExpr::List(arms)), range);
        let choice = self.alloc(
            Expr::Record {
                path: Path::lang(LangItem::Choice),
                fields: vec![
                    (Name::new_static("id"), id),
                    (Name::new_static("render"), render),
                    (Name::new_static("arms"), arms),
                ],
            },
            range,
        );
        self.lang_call(LangItem::PushStep, vec![choice], range)
    }

    pub(super) fn lower_text_form(&mut self, form: &ast::TextForm) -> ExprId {
        let range = form.syntax().text_range();
        let mut items: Vec<ExprId> = Vec::new();
        for item in form.items() {
            let lowered = match item {
                ast::TextItem::TextString(it) => self.text_string(it.token().as_ref(), range),
                ast::TextItem::TextRuby(it) => self.text_ruby(&it, range),
                ast::TextItem::TextEffect(it) => {
                    let base = match it.base() {
                        Some(ast::EffectBase::TextString(base)) => {
                            self.text_string(base.token().as_ref(), range)
                        }
                        Some(ast::EffectBase::TextRuby(base)) => self.text_ruby(&base, range),
                        None => self.missing(range),
                    };
                    let effect = self.lower_expr_opt(it.effect(), range);
                    self.lang_call(LangItem::Effect, vec![base, effect], range)
                }
                ast::TextItem::TextInterp(it) => self.lower_expr_opt(it.expr(), range),
            };
            items.push(lowered);
        }
        match items.len() {
            0 => self.alloc(Expr::Literal(Literal::String(String::new())), range),
            1 => items.pop().unwrap_or_else(|| self.missing(range)),
            _ => {
                let array = self.alloc(Expr::Array(ArrayExpr::List(items)), range);
                self.lang_call(LangItem::Concat, vec![array], range)
            }
        }
    }

    /// A string-literal token as a `Literal::String`.
    fn text_string(
        &mut self,
        token: Option<&kagari_syntax::SyntaxToken>,
        range: TextRange,
    ) -> ExprId {
        match token.and_then(string_token_value) {
            Some(value) => self.alloc(Expr::Literal(Literal::String(value)), range),
            None => self.missing(range),
        }
    }

    fn text_ruby(&mut self, ruby: &ast::TextRuby, range: TextRange) -> ExprId {
        let base = self.text_string(ruby.base().as_ref(), range);
        let readings: Vec<ExprId> = ruby
            .readings()
            .into_iter()
            .flat_map(|readings| readings.strings())
            .map(|token| self.text_string(Some(&token), range))
            .collect();
        match <[ExprId; 1]>::try_from(readings) {
            Ok([reading]) => self.lang_call(LangItem::Ruby, vec![base, reading], range),
            Err(readings) => {
                let array = self.alloc(Expr::Array(ArrayExpr::List(readings)), range);
                self.lang_call(LangItem::MonoRuby, vec![base, array], range)
            }
        }
    }
}

/// The unescaped value of a `STRING` token (`"…"`), if it is well-formed.
fn string_token_value(token: &kagari_syntax::SyntaxToken) -> Option<String> {
    ast::unescape_string(token.text()).ok()
}
