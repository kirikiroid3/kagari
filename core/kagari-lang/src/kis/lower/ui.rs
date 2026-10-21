//! Lowering of the system modes: `<page>`, `<dialog>` and `<game-mode>`, their UI trees,
//! handler attributes and `<on-exit>`.

use kagari_syntax::script::ast::{Element, decode_content};
use la_arena::Idx;
use smol_str::SmolStr;

use super::system::{Binding, Expect, OpArg, OpKind, is_reserved};
use super::{Attr, Child, Lowerer, attr, children, name_span, tag_span};
use crate::diagnostic::{DiagnosticCode, Span};
use crate::input::SourceFile;
use crate::kis::hir::MatchArm;
use crate::kis::schema::Tag;
use crate::kis::system_hir::{
    ModeDecl, ModeKind, SettingDecl, SysCall, SysStmt, SysStmtKind, SysType, UiKind, UiNode,
};
use crate::kis::value::{Value, VarType, check_name, parse_value};

/// Where a UI element is being lowered.
#[derive(Clone, Copy, Default)]
struct UiCtx {
    /// Inside a `<layer>` of `<game-mode>`.
    game_layer: bool,
    /// Inside a `<for-*>`.
    repetition: bool,
    /// Inside `<for-history>`.
    history: bool,
}

/// The input payload passed to an `on-keyboard`/`on-controller` handler.
struct Payload {
    attr: &'static str,
    /// The payload types, as written in `args`.
    shown: &'static str,
    enum_name: &'static str,
    vec2: bool,
}

const KEYBOARD: Payload = Payload {
    attr: "on-keyboard",
    shown: "Key",
    enum_name: "Key",
    vec2: false,
};

const CONTROLLER: Payload = Payload {
    attr: "on-controller",
    shown: "vec2 PadButton",
    enum_name: "PadButton",
    vec2: true,
};

fn is_ui(tag: Tag) -> bool {
    matches!(
        tag,
        Tag::Row
            | Tag::Col
            | Tag::Button
            | Tag::Slider
            | Tag::Toggle
            | Tag::Select
            | Tag::TextFrame
            | Tag::BacklogLine
            | Tag::ForSlots
            | Tag::ForHistory
            | Tag::ForChoices
    )
}

/// Split a handler call `ident ( [arg {, arg}] )` into its name and trimmed arguments.
fn split_call(text: &str) -> Option<(&str, Vec<&str>)> {
    let (name, rest) = text.trim().split_once('(')?;
    let name = name.trim();
    let inner = rest.strip_suffix(')')?;
    if !check_name(name) || inner.contains(['(', ')']) {
        return None;
    }
    if inner.trim().is_empty() {
        return Some((name, Vec::new()));
    }
    let args: Vec<&str> = inner.split(',').map(str::trim).collect();
    if args.iter().any(|arg| arg.is_empty()) {
        return None;
    }
    Some((name, args))
}

impl Lowerer<'_> {
    // ---- modes ----------------------------------------------------------------------------

    /// Allocate a mode with its kind, so operations can refer to it before its body is
    /// lowered.
    pub(super) fn mode_shell(
        &mut self,
        file: SourceFile,
        element: &Element,
        tag: Tag,
    ) -> Idx<ModeDecl> {
        self.check_attrs(file, element, tag);
        let name = self.name_attr(file, element, "name");
        let span = tag_span(file, element);
        let name = name
            .map(|attr| SmolStr::new(&attr.text))
            .unwrap_or_default();
        let kind = match tag {
            Tag::GameMode => ModeKind::Game,
            Tag::Dialog => ModeKind::Dialog,
            _ => {
                let root = attr(element, "root").is_some_and(|root| {
                    match parse_value(VarType::Bool, &self.hir.enums, &root.text) {
                        Ok(value) => value == Value::Bool(true),
                        Err(err) => {
                            self.report_value(Span::new(file, root.range), &err);
                            false
                        }
                    }
                });
                if root {
                    ModeKind::RootPage
                } else {
                    ModeKind::Page
                }
            }
        };
        let requires_game = match attr(element, "requires") {
            None => false,
            Some(requires) if requires.text != "game" => {
                self.report(
                    Span::new(file, requires.range),
                    DiagnosticCode::KisBadValue,
                    format!(
                        "`{}` is not a valid `requires`; expected `game`",
                        requires.text
                    ),
                    Vec::new(),
                );
                false
            }
            Some(requires) if kind == ModeKind::RootPage => {
                self.report(
                    Span::new(file, requires.range),
                    DiagnosticCode::KisBadValue,
                    "a root page cannot require a running game",
                    Vec::new(),
                );
                false
            }
            Some(_) => true,
        };
        let mut register = !name.is_empty();
        if register
            && let Some(first) = self
                .modes
                .get(&name)
                .map(|idx| self.hir.system.modes[*idx].span)
        {
            let msg = format!("mode `{name}` is already declared");
            self.duplicate(span, first, DiagnosticCode::KisDuplicateMode, msg);
            register = false;
        }
        let second_game = match self.hir.system.game_mode {
            Some(first) if kind == ModeKind::Game => Some(self.hir.system.modes[first].span),
            _ => None,
        };
        if let Some(first) = second_game {
            let msg = "`<system>` already has a `<game-mode>`".to_owned();
            self.duplicate(span, first, DiagnosticCode::KisDuplicateMode, msg);
        }
        let idx = self.hir.system.modes.alloc(ModeDecl {
            name: name.clone(),
            span,
            kind,
            requires_game,
            on_keyboard: None,
            on_controller: None,
            ui: Vec::new(),
            on_exit: Vec::new(),
        });
        if register {
            self.modes.insert(name, idx);
        }
        if kind == ModeKind::Game && self.hir.system.game_mode.is_none() {
            self.hir.system.game_mode = Some(idx);
        }
        idx
    }

    pub(super) fn mode_body(
        &mut self,
        idx: Idx<ModeDecl>,
        file: SourceFile,
        element: &Element,
        tag: Tag,
    ) {
        self.ss.bindings.clear();
        let on_keyboard = attr(element, KEYBOARD.attr)
            .and_then(|handler| self.event_handler(file, &handler, &KEYBOARD));
        let on_controller = attr(element, CONTROLLER.attr)
            .and_then(|handler| self.event_handler(file, &handler, &CONTROLLER));
        let kind = self.hir.system.modes[idx].kind;
        let game = kind == ModeKind::Game;
        let place = format!("inside `<{}>`", tag.name());
        let mut ui = Vec::new();
        let mut on_exit = None;
        let mut screen = false;
        for child in children(element) {
            let child = match child {
                Child::Text(run) => {
                    self.whitespace_only(file, run, &place);
                    continue;
                }
                Child::Element(child) => child,
            };
            let Some(child_tag) = self.classify(file, &child) else {
                continue;
            };
            match child_tag {
                Tag::Layer => ui.extend(self.layer(file, &child, game)),
                Tag::GameScreen if game && !screen => {
                    screen = true;
                    self.check_attrs(file, &child, Tag::GameScreen);
                    self.leaf(file, &child, Tag::GameScreen);
                    ui.push(UiNode {
                        kind: UiKind::GameScreen,
                        span: tag_span(file, &child),
                        style: None,
                        on_click: None,
                        bind: None,
                        children: Vec::new(),
                    });
                }
                Tag::OnExit if game && on_exit.is_none() => {
                    on_exit = Some(self.on_exit(file, &child));
                }
                Tag::GameScreen | Tag::OnExit if game => {
                    self.not_allowed(file, &child, child_tag, "more than once in `<game-mode>`");
                }
                _ if !game && is_ui(child_tag) => {
                    ui.extend(self.ui_node(file, &child, child_tag, UiCtx::default()));
                }
                _ => self.not_allowed(file, &child, child_tag, &place),
            }
        }
        if game && on_exit.is_none() {
            self.report(
                name_span(file, element),
                DiagnosticCode::KisMissingOnExit,
                "`<game-mode>` needs an `<on-exit>` to decide what follows the story",
                Vec::new(),
            );
        }
        let mode = &mut self.hir.system.modes[idx];
        mode.on_keyboard = on_keyboard;
        mode.on_controller = on_controller;
        mode.ui = ui;
        mode.on_exit = on_exit.unwrap_or_default();
    }

    /// `<on-exit>`: `<cases>` matched against the game's `Ending`.
    fn on_exit(&mut self, file: SourceFile, element: &Element) -> Vec<MatchArm<Vec<SysStmt>>> {
        self.check_attrs(file, element, Tag::OnExit);
        let tys: Option<Vec<VarType>> = self
            .enums
            .get("Ending")
            .map(|ending| vec![VarType::Enum(*ending)]);
        let mut lower_do = |this: &mut Self, file: SourceFile, body: &Element| {
            let mut stmts = Vec::new();
            this.sys_body(file, body, false, &mut stmts);
            stmts
        };
        let mut arms = Vec::new();
        let mut valid = true;
        for child in children(element) {
            let child = match child {
                Child::Text(run) => {
                    self.whitespace_only(file, run, "inside `<on-exit>`");
                    continue;
                }
                Child::Element(child) => child,
            };
            match self.classify(file, &child) {
                Some(Tag::Cases) => match self.cases(file, &child, tys.as_deref(), &mut lower_do) {
                    Some(arm) => arms.push(arm),
                    None => valid = false,
                },
                Some(other) => self.not_allowed(file, &child, other, "inside `<on-exit>`"),
                None => {}
            }
        }
        if let Some(tys) = tys.filter(|_| valid) {
            self.check_exhaustive(name_span(file, element), "`<on-exit>`", &tys, &arms);
        }
        arms
    }

    // ---- UI -------------------------------------------------------------------------------

    fn layer(&mut self, file: SourceFile, element: &Element, game: bool) -> Option<UiNode> {
        self.check_attrs(file, element, Tag::Layer);
        let name = self.name_attr(file, element, "name");
        let ctx = UiCtx {
            game_layer: game,
            ..UiCtx::default()
        };
        let mut nodes = Vec::new();
        self.ui_children(file, element, ctx, "inside `<layer>`", &mut nodes);
        Some(UiNode {
            kind: UiKind::Layer(name.map(|n| SmolStr::new(n.text)).unwrap_or_default()),
            span: tag_span(file, element),
            style: attr(element, "style").map(|style| SmolStr::new(style.text)),
            on_click: None,
            bind: None,
            children: nodes,
        })
    }

    fn ui_children(
        &mut self,
        file: SourceFile,
        parent: &Element,
        ctx: UiCtx,
        place: &str,
        out: &mut Vec<UiNode>,
    ) {
        for child in children(parent) {
            let child = match child {
                Child::Text(run) => {
                    self.whitespace_only(file, run, place);
                    continue;
                }
                Child::Element(child) => child,
            };
            let Some(tag) = self.classify(file, &child) else {
                continue;
            };
            if is_ui(tag) {
                out.extend(self.ui_node(file, &child, tag, ctx));
            } else {
                self.not_allowed(file, &child, tag, place);
            }
        }
    }

    fn ui_node(
        &mut self,
        file: SourceFile,
        element: &Element,
        tag: Tag,
        ctx: UiCtx,
    ) -> Option<UiNode> {
        let misplaced = match tag {
            Tag::TextFrame if !ctx.game_layer => Some("outside a `<layer>` of `<game-mode>`"),
            Tag::BacklogLine if !ctx.history => Some("outside `<for-history>`"),
            Tag::ForSlots | Tag::ForHistory | Tag::ForChoices if ctx.repetition => {
                Some("inside another `<for-slots>`, `<for-history>` or `<for-choices>`")
            }
            _ => None,
        };
        if let Some(place) = misplaced {
            self.not_allowed(file, element, tag, place);
            return None;
        }
        self.check_attrs(file, element, tag);
        let place = format!("inside `<{}>`", tag.name());
        let mut node = UiNode {
            kind: UiKind::Row,
            span: tag_span(file, element),
            style: attr(element, "style").map(|style| SmolStr::new(style.text)),
            on_click: None,
            bind: None,
            children: Vec::new(),
        };
        node.kind = match tag {
            Tag::Row | Tag::Col => {
                self.ui_children(file, element, ctx, &place, &mut node.children);
                if tag == Tag::Row {
                    UiKind::Row
                } else {
                    UiKind::Col
                }
            }
            Tag::Button => {
                node.on_click =
                    attr(element, "on-click").and_then(|handler| self.handler_call(file, &handler));
                UiKind::Button(self.button_label(file, element))
            }
            Tag::TextFrame => {
                self.leaf(file, element, tag);
                node.on_click =
                    attr(element, "on-click").and_then(|handler| self.handler_call(file, &handler));
                UiKind::TextFrame
            }
            Tag::Slider | Tag::Toggle | Tag::Select => {
                self.leaf(file, element, tag);
                node.bind = self.bind(file, element, tag);
                match tag {
                    Tag::Slider => UiKind::Slider,
                    Tag::Toggle => UiKind::Toggle,
                    _ => UiKind::Select,
                }
            }
            Tag::BacklogLine => {
                self.leaf(file, element, tag);
                UiKind::BacklogLine
            }
            Tag::ForSlots | Tag::ForHistory | Tag::ForChoices => {
                let (binding, kind) = match tag {
                    Tag::ForSlots => (
                        Binding::Slot(self.slot_count(name_span(file, element))),
                        UiKind::ForSlots,
                    ),
                    Tag::ForHistory => (Binding::History, UiKind::ForHistory),
                    _ => (Binding::Choice, UiKind::ForChoices),
                };
                let inner = UiCtx {
                    repetition: true,
                    history: ctx.history || tag == Tag::ForHistory,
                    ..ctx
                };
                self.ss.bindings.push(binding);
                self.ui_children(file, element, inner, &place, &mut node.children);
                self.ss.bindings.pop();
                kind
            }
            _ => return None,
        };
        Some(node)
    }

    /// The text label of a `<button>`.
    fn button_label(&mut self, file: SourceFile, element: &Element) -> String {
        let mut label = String::new();
        for child in children(element) {
            match child {
                Child::Text(run) => label.push_str(&decode_content(run.into_iter()).text),
                Child::Element(child) => {
                    if let Some(tag) = self.classify(file, &child) {
                        self.not_allowed(
                            file,
                            &child,
                            tag,
                            "inside `<button>`; a button label is text only",
                        );
                    }
                }
            }
        }
        label.trim().to_owned()
    }

    /// The setting a `<slider>`, `<toggle>` or `<select>` is bound to.
    fn bind(&mut self, file: SourceFile, element: &Element, tag: Tag) -> Option<Idx<SettingDecl>> {
        let bind = attr(element, "bind")?;
        let span = Span::new(file, bind.range);
        let Some(slot) = self.settings.get(bind.text.as_str()).copied() else {
            let message = if self.metas.contains_key(bind.text.as_str()) {
                format!(
                    "`{}` is a meta field, but `bind` needs a setting",
                    bind.text
                )
            } else {
                format!("unknown setting `{}`", bind.text)
            };
            self.report(span, DiagnosticCode::KisUnknownSetting, message, Vec::new());
            return None;
        };
        let decl = slot.decl?;
        let setting = &self.hir.system.settings[decl];
        let have = match setting.ty {
            VarType::Bool => "setting-bool",
            VarType::Enum(_) => "setting-enum",
            VarType::Char | VarType::Int { .. } => "setting-int",
        };
        let want = match tag {
            Tag::Slider => "setting-int",
            Tag::Toggle => "setting-bool",
            _ => "setting-enum",
        };
        if have == want {
            return Some(decl);
        }
        let note = setting.span.related(self.db, "setting declared here");
        self.report(
            span,
            DiagnosticCode::KisBindMismatch,
            format!(
                "`<{}>` needs a `<{want}>`, but `{}` is a `<{have}>`",
                tag.name(),
                bind.text
            ),
            vec![note],
        );
        None
    }

    // ---- handlers -------------------------------------------------------------------------

    /// Parse a handler attribute into its callee name and arguments.
    fn parse_call(&mut self, span: Span, text: &str) -> Option<(String, Vec<OpArg>)> {
        let Some((name, args)) = split_call(text) else {
            self.report(
                span,
                DiagnosticCode::KisBadHandler,
                "expected a call such as `f()` or `metafn__save(3)`",
                Vec::new(),
            );
            return None;
        };
        let args = args
            .into_iter()
            .map(|text| OpArg {
                text: text.to_owned(),
                span,
            })
            .collect();
        Some((name.to_owned(), args))
    }

    /// An `on-click` call: an operation or a system function, with explicit arguments.
    fn handler_call(&mut self, file: SourceFile, handler: &Attr) -> Option<SysStmt> {
        let span = Span::new(file, handler.range);
        let (name, args) = self.parse_call(span, &handler.text)?;
        let kind = if is_reserved(&name) {
            let Some(kind) = OpKind::by_name(&name) else {
                self.report(
                    span,
                    DiagnosticCode::KisUnknownOperation,
                    format!("unknown operation `{name}`"),
                    Vec::new(),
                );
                return None;
            };
            if !self.check_arity(span, &name, kind.arity(), args.len()) {
                return None;
            }
            SysStmtKind::Op(self.op(kind, &args, span)?)
        } else {
            let func = self.lookup_sysfn(span, &name)?;
            let expected = self.hir.system.sysfns[func].args.clone();
            if self.sig_is_bad(func) || !self.check_arity(span, &name, expected.len(), args.len()) {
                return None;
            }
            let mut operands = Vec::with_capacity(args.len());
            for (arg, ty) in args.iter().zip(expected) {
                operands.push(self.operand(arg.span, &arg.text, Expect::Ty(ty)));
            }
            let args = operands.into_iter().collect::<Option<Vec<_>>>()?;
            SysStmtKind::Call(SysCall {
                func,
                args,
                save: None,
            })
        };
        Some(SysStmt { kind, span })
    }

    fn check_arity(&mut self, span: Span, name: &str, want: usize, got: usize) -> bool {
        if want == got {
            return true;
        }
        self.report(
            span,
            DiagnosticCode::KisSysfnArity,
            format!("`{name}` takes {want} argument(s), but {got} were given"),
            Vec::new(),
        );
        false
    }

    /// An `on-keyboard`/`on-controller` handler: a system function written with empty
    /// parentheses whose `args` are exactly the payload.
    fn event_handler(
        &mut self,
        file: SourceFile,
        handler: &Attr,
        payload: &Payload,
    ) -> Option<SysStmt> {
        let span = Span::new(file, handler.range);
        let (name, args) = self.parse_call(span, &handler.text)?;
        let which = payload.attr;
        if is_reserved(&name) {
            self.report(
                span,
                DiagnosticCode::KisHandlerSignature,
                format!(
                    "`{which}` must call a system function taking `{}`, not the operation `{name}`",
                    payload.shown
                ),
                Vec::new(),
            );
            return None;
        }
        let func = self.lookup_sysfn(span, &name)?;
        if !args.is_empty() {
            self.report(
                span,
                DiagnosticCode::KisHandlerSignature,
                format!(
                    "`{which}` passes `{}` itself; write `{name}()` without arguments",
                    payload.shown
                ),
                Vec::new(),
            );
            return None;
        }
        let expected = self.enums.get(payload.enum_name).map(|idx| {
            let input = SysType::Var(VarType::Enum(*idx));
            if payload.vec2 {
                vec![SysType::Vec2, input]
            } else {
                vec![input]
            }
        });
        let decl = &self.hir.system.sysfns[func];
        if let Some(expected) = expected
            && !self.sig_is_bad(func)
            && decl.args != expected
        {
            let note = decl.span.related(self.db, "system function declared here");
            let message = format!(
                "`{which}` passes `{}`, but `{name}` takes {}",
                payload.shown,
                self.sys_args_name(&decl.args)
            );
            self.report(
                span,
                DiagnosticCode::KisHandlerSignature,
                message,
                vec![note],
            );
            return None;
        }
        Some(SysStmt {
            kind: SysStmtKind::Call(SysCall {
                func,
                args: Vec::new(),
                save: None,
            }),
            span,
        })
    }
}
