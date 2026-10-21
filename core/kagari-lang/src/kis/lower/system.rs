//! Lowering of the system scope: the top of `<system>`, settings, save slots, system functions
//! and their statements, and the meta-function and interface operations.

use kagari_syntax::script::ast::Element;
use la_arena::Idx;
use rustc_hash::{FxHashMap, FxHashSet};
use smol_str::SmolStr;

use super::{Attr, Child, Lowerer, attr, children, default_value, name_span, tag, tag_span};
use crate::collect::TopElement;
use crate::diagnostic::{DiagnosticCode, Span};
use crate::input::SourceFile;
use crate::kis::schema::Tag;
use crate::kis::system_hir::{
    FocusTarget, ModeDecl, ModeKind, Operand, SaveSlots, SettingDecl, SysCall, SysFnDecl, SysLocal,
    SysOp, SysStmt, SysStmtKind, SysType, SysValue, SysVar,
};
use crate::kis::value::{
    I16, ValueError, VarType, int_assignable, int_domain, parse_amount, parse_repr, parse_value,
    parse_vec2, split_list,
};
use crate::represent::check::tarjan;

/// A declared setting. `decl` is `None` when its declaration is invalid.
#[derive(Clone, Copy)]
pub(super) struct SettingSlot {
    pub(super) decl: Option<Idx<SettingDecl>>,
    pub(super) span: Span,
}

/// A declared system function local. `decl` is `None` when its declaration is invalid.
#[derive(Clone, Copy)]
struct LocalSlot {
    decl: Option<Idx<SysLocal>>,
    span: Span,
}

/// A repetition element whose binding is in scope.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Binding {
    /// `<for-slots>`; the slot count, `None` when `<save-slots>` is missing or invalid.
    Slot(Option<u16>),
    Choice,
    History,
}

/// State of the system scope being lowered.
#[derive(Default)]
pub(super) struct SysState {
    /// The system function whose body is being lowered; `None` in `<on-exit>` and attributes.
    func: Option<Idx<SysFnDecl>>,
    locals: FxHashMap<SmolStr, LocalSlot>,
    pub(super) bindings: Vec<Binding>,
    /// `(caller, callee, call site)` of every `<sysfn-call>` inside a system function.
    calls: Vec<(Idx<SysFnDecl>, Idx<SysFnDecl>, Span)>,
    /// System functions whose `args` or `return` is invalid; calls are not checked against them.
    bad_sigs: FxHashSet<Idx<SysFnDecl>>,
    /// Whether a `<save-slots>` was written, valid or not.
    slots_declared: bool,
}

/// Top-of-`<system>` elements whose lowering waits for later passes.
#[derive(Default)]
pub(super) struct SystemTop {
    /// Settings and meta fields, in document order.
    decls: Vec<(SourceFile, Element, Tag)>,
    save_slots: Vec<(SourceFile, Element)>,
    sysfns: Vec<(Idx<SysFnDecl>, SourceFile, Element)>,
    modes: Vec<(Idx<ModeDecl>, SourceFile, Element, Tag)>,
}

/// A meta-function or interface operation, as written in a tag or an attribute call.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum OpKind {
    Start,
    Load,
    Save,
    Advance,
    Select,
    Quit,
    Revert,
    PlayVoice,
    Call,
    Return,
    Goto,
    FocusMove,
    FocusActivate,
}

impl OpKind {
    const ALL: [OpKind; 13] = [
        OpKind::Start,
        OpKind::Load,
        OpKind::Save,
        OpKind::Advance,
        OpKind::Select,
        OpKind::Quit,
        OpKind::Revert,
        OpKind::PlayVoice,
        OpKind::Call,
        OpKind::Return,
        OpKind::Goto,
        OpKind::FocusMove,
        OpKind::FocusActivate,
    ];

    /// The name of the attribute-call form.
    pub(super) fn name(self) -> &'static str {
        match self {
            OpKind::Start => "metafn__start",
            OpKind::Load => "metafn__load",
            OpKind::Save => "metafn__save",
            OpKind::Advance => "metafn__advance",
            OpKind::Select => "metafn__select",
            OpKind::Quit => "metafn__quit",
            OpKind::Revert => "metafn__revert",
            OpKind::PlayVoice => "metafn__play_voice",
            OpKind::Call => "interface__call",
            OpKind::Return => "interface__return",
            OpKind::Goto => "interface__goto",
            OpKind::FocusMove => "interface__focus_move",
            OpKind::FocusActivate => "interface__focus_activate",
        }
    }

    pub(super) fn by_name(name: &str) -> Option<OpKind> {
        OpKind::ALL.into_iter().find(|kind| kind.name() == name)
    }

    pub(super) fn arity(self) -> usize {
        match self {
            OpKind::Advance | OpKind::Quit | OpKind::Return | OpKind::FocusActivate => 0,
            _ => 1,
        }
    }

    fn from_tag(tag: Tag) -> Option<OpKind> {
        Some(match tag {
            Tag::MetafnStart => OpKind::Start,
            Tag::MetafnLoad => OpKind::Load,
            Tag::MetafnSave => OpKind::Save,
            Tag::MetafnAdvance => OpKind::Advance,
            Tag::MetafnSelect => OpKind::Select,
            Tag::MetafnQuit => OpKind::Quit,
            Tag::InterfaceCall => OpKind::Call,
            Tag::InterfaceReturn => OpKind::Return,
            Tag::InterfaceGoto => OpKind::Goto,
            Tag::InterfaceFocusMove => OpKind::FocusMove,
            Tag::InterfaceFocusActivate => OpKind::FocusActivate,
            _ => return None,
        })
    }
}

/// An operation or call argument and where to report it.
pub(super) struct OpArg {
    pub(super) text: String,
    pub(super) span: Span,
}

/// What an operand must be.
#[derive(Clone, Copy)]
pub(super) enum Expect {
    /// A value assignable to this type.
    Ty(SysType),
    /// Any counter variable, or a literal of this type.
    Numeric(VarType),
}

/// One `<sysfn-param>`.
enum Param {
    Var(OpArg),
    Literal(OpArg),
}

/// Whether `name` is reserved for operations.
pub(super) fn is_reserved(name: &str) -> bool {
    name.starts_with("metafn__") || name.starts_with("interface__")
}

pub(super) fn idx_usize<T>(idx: Idx<T>) -> usize {
    usize::try_from(u32::from(idx.into_raw())).unwrap_or(usize::MAX)
}

/// Whether every path through `stmts` ends in a `<sysfn-return>`.
fn returns_on_all_paths(stmts: &[SysStmt]) -> bool {
    stmts.iter().any(|stmt| match &stmt.kind {
        SysStmtKind::Return(_) => true,
        SysStmtKind::Match(m) => {
            !m.arms.is_empty() && m.arms.iter().all(|arm| returns_on_all_paths(&arm.body))
        }
        _ => false,
    })
}

fn default_sys_value(ty: SysType) -> SysValue {
    match ty {
        SysType::Var(ty) => SysValue::Var(default_value(ty)),
        SysType::Vec2 => SysValue::Vec2(0, 0),
    }
}

impl Lowerer<'_> {
    // ---- top of <system> ------------------------------------------------------------------

    pub(super) fn system_shell(&mut self, item: &TopElement, top: &mut SystemTop) {
        let file = item.file;
        let Some(element) = item.element(self.db) else {
            return;
        };
        let Some(tag) = self.classify(file, &element) else {
            return;
        };
        match tag {
            Tag::DeclareEnum => self.declare_enum(file, &element),
            Tag::Sysfn => {
                let idx = self.sysfn_shell(file, &element);
                top.sysfns.push((idx, file, element));
            }
            Tag::Page | Tag::Dialog | Tag::GameMode => {
                let idx = self.mode_shell(file, &element, tag);
                top.modes.push((idx, file, element, tag));
            }
            Tag::SettingInt
            | Tag::SettingEnum
            | Tag::SettingBool
            | Tag::MetaBool
            | Tag::MetaChar
            | Tag::MetaEnum
            | Tag::MetaInt => top.decls.push((file, element, tag)),
            Tag::SaveSlots => top.save_slots.push((file, element)),
            // Consumed by stage 1.
            Tag::Include | Tag::Import => {}
            _ => self.not_allowed(file, &element, tag, "at the top of `<system>`"),
        }
    }

    /// Settings and meta fields, save slots and system function signatures. Runs after every
    /// enum of both scopes is declared.
    pub(super) fn system_decls(&mut self, top: &SystemTop) {
        for (file, element, tag) in &top.decls {
            if tag.is_meta() {
                self.meta_decl(*file, element, *tag);
            } else {
                self.setting_decl(*file, element, *tag);
            }
        }
        for (index, (file, element)) in top.save_slots.iter().enumerate() {
            if index == 0 {
                self.save_slots(*file, element);
            } else {
                self.not_allowed(
                    *file,
                    element,
                    Tag::SaveSlots,
                    "more than once in `<system>`",
                );
            }
        }
        for (idx, file, element) in &top.sysfns {
            self.sysfn_signature(*idx, *file, element);
        }
    }

    /// System function and mode bodies. Runs after the game, so `metafn__start` can check
    /// entry scenes.
    pub(super) fn system_bodies(&mut self, top: &SystemTop) {
        for (idx, file, element) in &top.sysfns {
            self.sysfn_body(*idx, *file, element);
        }
        for (idx, file, element, tag) in &top.modes {
            self.mode_body(*idx, *file, element, *tag);
        }
    }

    pub(super) fn system_start(&mut self, system: Option<&TopElement>) {
        let Some(item) = system else {
            return;
        };
        let Some(element) = item.element(self.db) else {
            return;
        };
        let file = item.file;
        self.check_attrs(file, &element, Tag::System);
        let Some(start) = attr(&element, "start") else {
            return;
        };
        let span = Span::new(file, start.range);
        let Some(mode) = self.lookup_mode(span, &start.text) else {
            return;
        };
        if self.hir.system.modes[mode].kind != ModeKind::RootPage {
            self.report(
                span,
                DiagnosticCode::KisNotARootPage,
                format!(
                    "`{}` is not a root page; `<system start>` needs a `<page root=\"true\">`",
                    start.text
                ),
                Vec::new(),
            );
            return;
        }
        self.hir.system.start = Some(mode);
    }

    fn setting_decl(&mut self, file: SourceFile, element: &Element, tag: Tag) {
        self.check_attrs(file, element, tag);
        self.leaf(file, element, tag);
        let Some(name) = self.name_attr(file, element, "name") else {
            return;
        };
        let name = SmolStr::new(&name.text);
        let span = tag_span(file, element);
        let first = self
            .settings
            .get(&name)
            .map(|slot| slot.span)
            .or_else(|| self.metas.get(&name).map(|slot| slot.span));
        if let Some(first) = first {
            let msg = format!("`{name}` is already declared as a setting or meta field");
            self.duplicate(span, first, DiagnosticCode::KisDuplicateSetting, msg);
            return;
        }
        let decl = self.decl_type(file, element, tag).map(|ty| {
            let init = match attr(element, "init") {
                Some(init) => match parse_value(ty, &self.hir.enums, &init.text) {
                    Ok(value) => value,
                    Err(err) => {
                        self.report_value(Span::new(file, init.range), &err);
                        default_value(ty)
                    }
                },
                None => default_value(ty),
            };
            self.hir.system.settings.alloc(SettingDecl {
                name: name.clone(),
                span,
                ty,
                init,
            })
        });
        self.settings.insert(name, SettingSlot { decl, span });
    }

    fn save_slots(&mut self, file: SourceFile, element: &Element) {
        self.check_attrs(file, element, Tag::SaveSlots);
        self.leaf(file, element, Tag::SaveSlots);
        self.ss.slots_declared = true;
        let Some(count) = attr(element, "count") else {
            return;
        };
        let ty = VarType::Int {
            min: 1,
            max: i16::MAX,
        };
        match parse_value(ty, &self.hir.enums, &count.text) {
            Ok(value) => {
                let count = match value {
                    crate::kis::value::Value::Int(n) => u16::try_from(n).ok(),
                    _ => None,
                };
                self.hir.system.save_slots = count.map(|count| SaveSlots {
                    count,
                    span: tag_span(file, element),
                });
            }
            Err(err) => self.report_value(Span::new(file, count.range), &err),
        }
    }

    // ---- system functions -----------------------------------------------------------------

    /// Allocate a system function. Functions without a valid, unique name are still lowered,
    /// but cannot be called.
    fn sysfn_shell(&mut self, file: SourceFile, element: &Element) -> Idx<SysFnDecl> {
        self.check_attrs(file, element, Tag::Sysfn);
        let name = self.name_attr(file, element, "name");
        let span = tag_span(file, element);
        let text = name
            .as_ref()
            .map(|attr| SmolStr::new(&attr.text))
            .unwrap_or_default();
        let mut register = !text.is_empty();
        if let Some(name) = &name
            && is_reserved(&name.text)
        {
            self.report(
                Span::new(file, name.range),
                DiagnosticCode::KisReservedName,
                format!(
                    "`{}` is reserved: names starting with `metafn__` or `interface__` are operations",
                    name.text
                ),
                Vec::new(),
            );
            register = false;
        }
        if register
            && let Some(first) = self
                .sysfns
                .get(&text)
                .map(|idx| self.hir.system.sysfns[*idx].span)
        {
            let msg = format!("system function `{text}` is already declared");
            self.duplicate(span, first, DiagnosticCode::KisDuplicateSysfn, msg);
            register = false;
        }
        let idx = self.hir.system.sysfns.alloc(SysFnDecl {
            name: text.clone(),
            span,
            args: Vec::new(),
            returns: None,
            locals: Vec::new(),
            body: Vec::new(),
        });
        if register {
            self.sysfns.insert(text, idx);
        }
        idx
    }

    fn sysfn_signature(&mut self, idx: Idx<SysFnDecl>, file: SourceFile, element: &Element) {
        let mut valid = true;
        let mut args = Vec::new();
        if let Some(list) = attr(element, "args") {
            let span = Span::new(file, list.range);
            for part in split_list(&list.text) {
                match self.sys_type(span, part) {
                    Some(ty) => args.push(ty),
                    None => valid = false,
                }
            }
            if args.len() > usize::from(u8::MAX) {
                self.report(
                    span,
                    DiagnosticCode::KisValueOutOfRange,
                    "a system function takes at most 255 arguments",
                    Vec::new(),
                );
                valid = false;
            }
        }
        let returns = match attr(element, "return") {
            Some(ret) => {
                let ty = self.sys_type(Span::new(file, ret.range), ret.text.trim());
                valid &= ty.is_some();
                ty
            }
            None => None,
        };
        let decl = &mut self.hir.system.sysfns[idx];
        decl.args = args;
        decl.returns = returns;
        if !valid {
            self.ss.bad_sigs.insert(idx);
        }
    }

    /// A type of `args`/`return`: `bool`, `char`, `i16`, `vec2` or an enum.
    fn sys_type(&mut self, span: Span, text: &str) -> Option<SysType> {
        match text {
            "bool" => Some(SysType::Var(VarType::Bool)),
            "char" => Some(SysType::Var(VarType::Char)),
            "i16" => Some(SysType::Var(I16)),
            "vec2" => Some(SysType::Vec2),
            "int" => {
                self.report(
                    span,
                    DiagnosticCode::KisBadSysfnType,
                    "`int` needs a range; use `i16` or an enum",
                    Vec::new(),
                );
                None
            }
            name => {
                let found = self.enums.get(name).copied();
                if found.is_none() {
                    self.report(
                        span,
                        DiagnosticCode::KisUnknownEnum,
                        format!("unknown enum `{name}`"),
                        Vec::new(),
                    );
                }
                found.map(|idx| SysType::Var(VarType::Enum(idx)))
            }
        }
    }

    pub(super) fn sys_type_name(&self, ty: SysType) -> String {
        match ty {
            SysType::Var(ty) => self.type_name(ty),
            SysType::Vec2 => "`vec2`".to_owned(),
        }
    }

    /// The argument list of a system function, for messages.
    pub(super) fn sys_args_name(&self, args: &[SysType]) -> String {
        if args.is_empty() {
            return "no arguments".to_owned();
        }
        let names: Vec<String> = args.iter().map(|ty| self.sys_type_name(*ty)).collect();
        names.join(" ")
    }

    pub(super) fn sys_assignable(&self, src: SysType, dst: SysType) -> bool {
        match (src, dst) {
            (SysType::Var(src), SysType::Var(dst)) => int_assignable(src, dst),
            (src, dst) => src == dst,
        }
    }

    /// Whether `func`'s `args` or `return` is invalid, so uses are not checked against it.
    pub(super) fn sig_is_bad(&self, func: Idx<SysFnDecl>) -> bool {
        self.ss.bad_sigs.contains(&func)
    }

    fn parse_sys_value(&self, ty: SysType, text: &str) -> Result<SysValue, ValueError> {
        match ty {
            SysType::Var(ty) => parse_value(ty, &self.hir.enums, text).map(SysValue::Var),
            SysType::Vec2 => parse_vec2(text).map(|(x, y)| SysValue::Vec2(x, y)),
        }
    }

    fn sysfn_body(&mut self, idx: Idx<SysFnDecl>, file: SourceFile, element: &Element) {
        self.ss.func = Some(idx);
        self.ss.locals.clear();
        let mut locals = Vec::new();
        for child in element.child_elements() {
            let Some(tag) =
                tag(&child.name_text().unwrap_or_default()).filter(|tag| tag.is_sys_var())
            else {
                continue;
            };
            locals.extend(self.sys_local(file, &child, tag));
        }
        let mut body = Vec::new();
        self.sys_body(file, element, true, &mut body);
        let decl = &self.hir.system.sysfns[idx];
        if decl.returns.is_some()
            && !self.ss.bad_sigs.contains(&idx)
            && !returns_on_all_paths(&body)
        {
            let name = decl.name.clone();
            self.report(
                name_span(file, element),
                DiagnosticCode::KisMissingReturn,
                format!("system function `{name}` can reach its end without `<sysfn-return>`"),
                Vec::new(),
            );
        }
        let decl = &mut self.hir.system.sysfns[idx];
        decl.locals = locals;
        decl.body = body;
        self.ss.func = None;
        self.ss.locals.clear();
    }

    fn sys_local(
        &mut self,
        file: SourceFile,
        element: &Element,
        tag: Tag,
    ) -> Option<Idx<SysLocal>> {
        self.check_attrs(file, element, tag);
        self.leaf(file, element, tag);
        if tag.is_var()
            && let Some(meta) = element.attribute("from-meta")
        {
            let range = meta
                .name_range()
                .unwrap_or_else(|| element.start_tag_range());
            self.report(
                Span::new(file, range),
                DiagnosticCode::KisUnknownAttribute,
                "`from-meta` is ignored on a system function local",
                Vec::new(),
            );
        }
        let name = self.name_attr(file, element, "name")?;
        let name = SmolStr::new(&name.text);
        let span = tag_span(file, element);
        if let Some(first) = self.ss.locals.get(&name).map(|slot| slot.span) {
            let msg = format!("variable `{name}` is already declared in this system function");
            self.duplicate(span, first, DiagnosticCode::KisDuplicateVariable, msg);
            return None;
        }
        let shadowed = match (self.settings.get(&name), self.metas.get(&name)) {
            (Some(setting), _) => Some(("setting", setting.span)),
            (None, Some(meta)) => Some(("meta field", meta.span)),
            (None, None) => None,
        };
        if let Some((what, first)) = shadowed {
            let note = first.related(self.db, format!("{what} declared here"));
            self.report(
                span,
                DiagnosticCode::KisLocalShadowsGlobal,
                format!("local variable `{name}` shadows {what} `{name}`"),
                vec![note],
            );
        }
        let decl = self.sys_local_decl(file, element, tag, &name, span);
        self.ss.locals.insert(name, LocalSlot { decl, span });
        decl
    }

    fn sys_local_decl(
        &mut self,
        file: SourceFile,
        element: &Element,
        tag: Tag,
        name: &SmolStr,
        span: Span,
    ) -> Option<Idx<SysLocal>> {
        let ty = if tag == Tag::VarVec2 {
            SysType::Vec2
        } else {
            SysType::Var(self.decl_type(file, element, tag)?)
        };
        let init_attr = attr(element, "init")?;
        let init = match self.parse_sys_value(ty, &init_attr.text) {
            Ok(value) => value,
            Err(err) => {
                self.report_value(Span::new(file, init_attr.range), &err);
                default_sys_value(ty)
            }
        };
        Some(self.hir.system.locals.alloc(SysLocal {
            name: name.clone(),
            span,
            ty,
            init,
        }))
    }

    /// A system function body, or the `<do>` of a `<match>` or `<on-exit>` case. `direct` is
    /// set for the direct children of `<sysfn>`, where locals are declared.
    pub(super) fn sys_body(
        &mut self,
        file: SourceFile,
        parent: &Element,
        direct: bool,
        out: &mut Vec<SysStmt>,
    ) {
        let place = if self.ss.func.is_some() {
            "in a system function body"
        } else {
            "inside `<on-exit>`"
        };
        for child in children(parent) {
            let element = match child {
                Child::Text(run) => {
                    self.whitespace_only(file, run, place);
                    continue;
                }
                Child::Element(element) => element,
            };
            let Some(tag) = self.classify(file, &element) else {
                continue;
            };
            let kind = match tag {
                Tag::SysfnArg => self.sysfn_arg(file, &element),
                Tag::SysfnReturn => self.sysfn_return(file, &element),
                Tag::SysfnCall => self.sysfn_call(file, &element).map(SysStmtKind::Call),
                Tag::Match => self
                    .match_stmt(file, &element, Self::sys_match_var, |this, file, body| {
                        let mut stmts = Vec::new();
                        this.sys_body(file, body, false, &mut stmts);
                        stmts
                    })
                    .map(SysStmtKind::Match),
                Tag::Set | Tag::Add | Tag::Sub => self.sys_assign(file, &element, tag),
                Tag::MetaSet => self
                    .meta_assignment(file, &element)
                    .map(|(meta, value)| SysStmtKind::MetaSet { meta, value }),
                _ if OpKind::from_tag(tag).is_some() => {
                    self.op_tag(file, &element, tag).map(SysStmtKind::Op)
                }
                // Lowered before the body.
                _ if tag.is_sys_var() && direct && self.ss.func.is_some() => None,
                _ if tag.is_sys_var() && self.ss.func.is_some() => {
                    self.not_allowed(
                        file,
                        &element,
                        tag,
                        "here; local variables must be direct children of `<sysfn>`",
                    );
                    None
                }
                _ => {
                    self.not_allowed(file, &element, tag, place);
                    None
                }
            };
            if let Some(kind) = kind {
                out.push(SysStmt {
                    kind,
                    span: tag_span(file, &element),
                });
            }
        }
    }

    /// The system function being lowered, or `None` after reporting `tag` outside one.
    fn current_func(
        &mut self,
        file: SourceFile,
        element: &Element,
        tag: Tag,
    ) -> Option<Idx<SysFnDecl>> {
        if self.ss.func.is_none() {
            self.not_allowed(file, element, tag, "outside `<sysfn>`");
        }
        self.ss.func
    }

    fn sysfn_arg(&mut self, file: SourceFile, element: &Element) -> Option<SysStmtKind> {
        let func = self.current_func(file, element, Tag::SysfnArg)?;
        self.check_attrs(file, element, Tag::SysfnArg);
        self.leaf(file, element, Tag::SysfnArg);
        let position_attr = attr(element, "position")?;
        let var_attr = attr(element, "var")?;
        let position_span = Span::new(file, position_attr.range);
        let position = match parse_repr(&position_attr.text) {
            Ok(position) => Some(position),
            Err(err) => {
                self.report_value(position_span, &err);
                None
            }
        };
        let var_span = Span::new(file, var_attr.range);
        let local = self.local_var(var_span, &var_attr.text);
        let (position, (var, var_ty)) = (position?, local?);
        if self.ss.bad_sigs.contains(&func) {
            return Some(SysStmtKind::Arg { position, var });
        }
        let decl = &self.hir.system.sysfns[func];
        let Some(arg_ty) = decl.args.get(usize::from(position)).copied() else {
            let message = format!(
                "argument position {position} is out of range; `{}` takes {} argument(s)",
                decl.name,
                decl.args.len()
            );
            self.report(
                position_span,
                DiagnosticCode::KisValueOutOfRange,
                message,
                Vec::new(),
            );
            return None;
        };
        if !self.sys_assignable(arg_ty, var_ty) {
            let message = format!(
                "argument {position} has type {}, but `{}` has type {}",
                self.sys_type_name(arg_ty),
                var_attr.text,
                self.sys_type_name(var_ty)
            );
            self.report(
                var_span,
                DiagnosticCode::KisAssignMismatch,
                message,
                Vec::new(),
            );
            return None;
        }
        Some(SysStmtKind::Arg { position, var })
    }

    fn sysfn_return(&mut self, file: SourceFile, element: &Element) -> Option<SysStmtKind> {
        let func = self.current_func(file, element, Tag::SysfnReturn)?;
        self.check_attrs(file, element, Tag::SysfnReturn);
        self.leaf(file, element, Tag::SysfnReturn);
        let span = name_span(file, element);
        let val = attr(element, "val");
        let local = val
            .as_ref()
            .and_then(|val| self.local_var(Span::new(file, val.range), &val.text));
        if self.ss.bad_sigs.contains(&func) {
            return Some(SysStmtKind::Return(local.map(|(var, _)| var)));
        }
        let decl = &self.hir.system.sysfns[func];
        let name = decl.name.clone();
        let returns = decl.returns;
        let message = match (returns, &val, local) {
            (None, None, _) => return Some(SysStmtKind::Return(None)),
            (None, Some(_), _) => (
                span,
                format!(
                    "system function `{name}` returns no value; declare `return` on the `<sysfn>` to return one"
                ),
            ),
            (Some(ty), None, _) => (
                span,
                format!(
                    "system function `{name}` returns {}; `<sysfn-return>` needs `val`",
                    self.sys_type_name(ty)
                ),
            ),
            (Some(_), Some(_), None) => return Some(SysStmtKind::Return(None)),
            (Some(ty), Some(val), Some((var, var_ty))) => {
                if self.sys_assignable(var_ty, ty) {
                    return Some(SysStmtKind::Return(Some(var)));
                }
                (
                    Span::new(file, val.range),
                    format!(
                        "`{}` has type {}, but `{name}` returns {}",
                        val.text,
                        self.sys_type_name(var_ty),
                        self.sys_type_name(ty)
                    ),
                )
            }
        };
        self.report(
            message.0,
            DiagnosticCode::KisReturnMismatch,
            message.1,
            Vec::new(),
        );
        // An invalid `<sysfn-return>` still ends the function.
        Some(SysStmtKind::Return(None))
    }

    /// A local of the system function being lowered, and its type.
    fn local_var(&mut self, span: Span, name: &str) -> Option<(Idx<SysLocal>, SysType)> {
        match self.ss.locals.get(name).copied() {
            Some(slot) => slot
                .decl
                .map(|decl| (decl, self.hir.system.locals[decl].ty)),
            None => {
                self.report(
                    span,
                    DiagnosticCode::KisNotALocal,
                    format!("`{name}` is not a local of this system function"),
                    Vec::new(),
                );
                None
            }
        }
    }

    fn sysfn_call(&mut self, file: SourceFile, element: &Element) -> Option<SysCall> {
        self.check_attrs(file, element, Tag::SysfnCall);
        let mut params = Vec::new();
        for child in children(element) {
            let child = match child {
                Child::Text(run) => {
                    self.whitespace_only(file, run, "inside `<sysfn-call>`");
                    continue;
                }
                Child::Element(child) => child,
            };
            match self.classify(file, &child) {
                Some(Tag::SysfnParam) => params.push(self.sysfn_param(file, &child)),
                Some(other) => self.not_allowed(file, &child, other, "inside `<sysfn-call>`"),
                None => {}
            }
        }
        let name = attr(element, "name")?;
        let name_span_ = Span::new(file, name.range);
        let func = self.lookup_sysfn(name_span_, &name.text)?;
        if let Some(caller) = self.ss.func {
            self.ss.calls.push((caller, func, name_span_));
        }
        let save = attr(element, "save").and_then(|save| self.call_save(file, &save, func));
        let bad_sig = self.ss.bad_sigs.contains(&func);
        let expected = self.hir.system.sysfns[func].args.clone();
        if !bad_sig && params.len() != expected.len() {
            let message = format!(
                "`{}` takes {} argument(s), but {} were given",
                name.text,
                expected.len(),
                params.len()
            );
            self.report(
                name_span(file, element),
                DiagnosticCode::KisSysfnArity,
                message,
                Vec::new(),
            );
            return None;
        }
        let mut args = Vec::with_capacity(params.len());
        let mut ok = true;
        for (param, ty) in params.into_iter().zip(expected) {
            let operand = match param {
                Some(Param::Var(arg)) => self.var_operand(arg.span, &arg.text, Expect::Ty(ty)),
                Some(Param::Literal(arg)) => {
                    self.literal_operand(arg.span, &arg.text, Expect::Ty(ty))
                }
                None => None,
            };
            match operand {
                Some(operand) => args.push(operand),
                None => ok = false,
            }
        }
        (ok && !bad_sig).then_some(SysCall { func, args, save })
    }

    fn sysfn_param(&mut self, file: SourceFile, element: &Element) -> Option<Param> {
        self.check_attrs(file, element, Tag::SysfnParam);
        self.leaf(file, element, Tag::SysfnParam);
        let arg = |attr: Attr| OpArg {
            text: attr.text,
            span: Span::new(file, attr.range),
        };
        match (attr(element, "var"), attr(element, "literal")) {
            (Some(var), None) => Some(Param::Var(arg(var))),
            (None, Some(literal)) => Some(Param::Literal(arg(literal))),
            _ => {
                self.report(
                    name_span(file, element),
                    DiagnosticCode::KisVarConflict,
                    "`<sysfn-param>` needs exactly one of `var` and `literal`",
                    Vec::new(),
                );
                None
            }
        }
    }

    /// The `save` of a `<sysfn-call>`: a writable variable the callee's return value fits in.
    fn call_save(&mut self, file: SourceFile, save: &Attr, func: Idx<SysFnDecl>) -> Option<SysVar> {
        let span = Span::new(file, save.range);
        let (var, ty) = self.sys_lookup(span, &save.text)?;
        if !self.check_writable(span, &save.text, var) {
            return None;
        }
        if self.ss.bad_sigs.contains(&func) {
            return Some(var);
        }
        let decl = &self.hir.system.sysfns[func];
        let callee = decl.name.clone();
        let note = decl.span.related(self.db, "system function declared here");
        let message = match (decl.returns, ty) {
            (None, _) => format!("system function `{callee}` returns no value to save"),
            (Some(ret), Some(ty)) if !self.sys_assignable(ret, ty) => format!(
                "`{callee}` returns {}, but `{}` has type {}",
                self.sys_type_name(ret),
                save.text,
                self.sys_type_name(ty)
            ),
            (Some(_), _) => return Some(var),
        };
        self.report(span, DiagnosticCode::KisAssignMismatch, message, vec![note]);
        None
    }

    pub(super) fn lookup_sysfn(&mut self, span: Span, name: &str) -> Option<Idx<SysFnDecl>> {
        let found = self.sysfns.get(name).copied();
        if found.is_none() {
            self.report(
                span,
                DiagnosticCode::KisUnknownSysfn,
                format!("unknown system function `{name}`"),
                Vec::new(),
            );
        }
        found
    }

    pub(super) fn lookup_mode(&mut self, span: Span, name: &str) -> Option<Idx<ModeDecl>> {
        let found = self.modes.get(name).copied();
        if found.is_none() {
            self.report(
                span,
                DiagnosticCode::KisUnknownMode,
                format!("unknown mode `{name}`"),
                Vec::new(),
            );
        }
        found
    }

    /// `<sysfn-call>` in a scene body: system functions cannot run in the game scope.
    pub(super) fn game_sysfn_call(&mut self, file: SourceFile, element: &Element) {
        self.check_attrs(file, element, Tag::SysfnCall);
        let Some(name) = attr(element, "name") else {
            return;
        };
        let span = Span::new(file, name.range);
        if self.sysfns.contains_key(name.text.as_str()) {
            self.report(
                span,
                DiagnosticCode::KisSysfnCallInGame,
                format!(
                    "system function `{}` can only be called from the system scope",
                    name.text
                ),
                Vec::new(),
            );
        } else {
            self.lookup_sysfn(span, &name.text);
        }
    }

    // ---- system variables -----------------------------------------------------------------

    /// Resolve a system variable: locals, loop bindings, settings, then meta fields. The type
    /// is `None` when it is unknown (already reported) or for `entry.*`, which only
    /// `metafn__revert` and `metafn__play_voice` take.
    pub(super) fn sys_lookup(
        &mut self,
        span: Span,
        name: &str,
    ) -> Option<(SysVar, Option<SysType>)> {
        if let Some(slot) = self.ss.locals.get(name).copied() {
            let decl = slot.decl?;
            return Some((SysVar::Local(decl), Some(self.hir.system.locals[decl].ty)));
        }
        if let Some(found) = self.binding_var(name) {
            return Some(found);
        }
        if let Some(slot) = self.settings.get(name).copied() {
            let decl = slot.decl?;
            let ty = self.hir.system.settings[decl].ty;
            return Some((SysVar::Setting(decl), Some(SysType::Var(ty))));
        }
        if let Some(slot) = self.metas.get(name).copied() {
            let decl = slot.decl?;
            let ty = self.hir.metas[decl].ty;
            return Some((SysVar::Meta(decl), Some(SysType::Var(ty))));
        }
        self.report(
            span,
            DiagnosticCode::KisUnknownVariable,
            format!("unknown variable `{name}`"),
            Vec::new(),
        );
        None
    }

    fn binding_var(&self, name: &str) -> Option<(SysVar, Option<SysType>)> {
        match name {
            "entry.index" => return Some((SysVar::EntryIndex, None)),
            "entry.voice" => return Some((SysVar::EntryVoice, None)),
            _ => {}
        }
        self.ss
            .bindings
            .iter()
            .rev()
            .find_map(|binding| match (binding, name) {
                (Binding::Slot(count), "slot.index") => {
                    let ty = count.map(|count| {
                        let max = i16::try_from(count.saturating_sub(1)).unwrap_or(i16::MAX);
                        SysType::Var(VarType::Int { min: 0, max })
                    });
                    Some((SysVar::SlotIndex, ty))
                }
                (Binding::Choice, "choice.index") => {
                    Some((SysVar::ChoiceIndex, Some(SysType::Var(VarType::Char))))
                }
                _ => None,
            })
    }

    /// Report a use of `entry.*` other than as the argument of its operation. Returns whether
    /// `var` is such a binding.
    fn entry_misuse(&mut self, span: Span, var: SysVar) -> bool {
        let (name, op) = match var {
            SysVar::EntryIndex => ("entry.index", OpKind::Revert),
            SysVar::EntryVoice => ("entry.voice", OpKind::PlayVoice),
            _ => return false,
        };
        self.report(
            span,
            DiagnosticCode::KisBadBinding,
            format!("`{name}` can only be the argument of `{}`", op.name()),
            Vec::new(),
        );
        true
    }

    /// Whether system code may write `var`; reports when it cannot.
    fn check_writable(&mut self, span: Span, name: &str, var: SysVar) -> bool {
        let message = match var {
            SysVar::Local(_) | SysVar::Setting(_) => return true,
            SysVar::EntryIndex | SysVar::EntryVoice => {
                self.entry_misuse(span, var);
                return false;
            }
            SysVar::Meta(_) => {
                format!("meta field `{name}` is read-only here; write it with `<meta-set>`")
            }
            SysVar::SlotIndex | SysVar::ChoiceIndex => format!("`{name}` is read-only"),
        };
        self.report(
            span,
            DiagnosticCode::KisReadOnlyVariable,
            message,
            Vec::new(),
        );
        false
    }

    fn sys_match_var(&mut self, span: Span, name: &str) -> Option<(SysVar, VarType)> {
        let (var, ty) = self.sys_lookup(span, name)?;
        if self.entry_misuse(span, var) {
            return None;
        }
        match ty? {
            SysType::Var(ty) => Some((var, ty)),
            SysType::Vec2 => {
                self.report(
                    span,
                    DiagnosticCode::KisNotMatchable,
                    format!("`{name}` has type `vec2`, which `<match>` cannot read"),
                    Vec::new(),
                );
                None
            }
        }
    }

    fn sys_assign(&mut self, file: SourceFile, element: &Element, tag: Tag) -> Option<SysStmtKind> {
        self.check_attrs(file, element, tag);
        self.leaf(file, element, tag);
        let var_attr = attr(element, "var")?;
        let val = attr(element, "val")?;
        let span = Span::new(file, var_attr.range);
        let (var, ty) = self.sys_lookup(span, &var_attr.text)?;
        if !self.check_writable(span, &var_attr.text, var) {
            return None;
        }
        let ty = ty?;
        let val_span = Span::new(file, val.range);
        if tag == Tag::Set {
            return match self.parse_sys_value(ty, &val.text) {
                Ok(value) => Some(SysStmtKind::Set { var, value }),
                Err(err) => {
                    self.report_value(val_span, &err);
                    None
                }
            };
        }
        let counter = match ty {
            SysType::Var(ty) if int_domain(ty).is_some() => ty,
            _ => {
                let message = format!(
                    "`<{}>` needs a `char`, `i16` or `int` variable, but `{}` has type {}",
                    tag.name(),
                    var_attr.text,
                    self.sys_type_name(ty)
                );
                self.report(span, DiagnosticCode::KisNotACounter, message, Vec::new());
                return None;
            }
        };
        match parse_amount(counter, &val.text) {
            Ok(amount) if tag == Tag::Add => Some(SysStmtKind::Add { var, amount }),
            Ok(amount) => Some(SysStmtKind::Sub { var, amount }),
            Err(err) => {
                self.report_value(val_span, &err);
                None
            }
        }
    }

    // ---- operands -------------------------------------------------------------------------

    /// An operand written as text: a literal when it starts with a digit or `-`, contains
    /// whitespace, is `true`/`false` or a variant of the expected enum; else a variable.
    pub(super) fn operand(&mut self, span: Span, text: &str, expected: Expect) -> Option<Operand> {
        let enum_variant = match expected {
            Expect::Ty(SysType::Var(VarType::Enum(idx))) => self.hir.enums[idx]
                .variants
                .iter()
                .any(|variant| variant.name == text),
            _ => false,
        };
        let literal = text
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_digit() || c == '-')
            || text.chars().any(char::is_whitespace)
            || text == "true"
            || text == "false"
            || enum_variant;
        if literal {
            self.literal_operand(span, text, expected)
        } else {
            self.var_operand(span, text, expected)
        }
    }

    fn literal_operand(&mut self, span: Span, text: &str, expected: Expect) -> Option<Operand> {
        let parsed = match expected {
            Expect::Ty(ty) => self.parse_sys_value(ty, text),
            Expect::Numeric(ty) => parse_value(ty, &self.hir.enums, text).map(SysValue::Var),
        };
        match parsed {
            Ok(value) => Some(Operand::Literal(value)),
            Err(err) => {
                self.report_value(span, &err);
                None
            }
        }
    }

    fn var_operand(&mut self, span: Span, name: &str, expected: Expect) -> Option<Operand> {
        let (var, ty) = self.sys_lookup(span, name)?;
        if self.entry_misuse(span, var) {
            return None;
        }
        let Some(ty) = ty else {
            return Some(Operand::Var(var));
        };
        let (ok, want) = match expected {
            Expect::Ty(want) => (self.sys_assignable(ty, want), self.sys_type_name(want)),
            Expect::Numeric(_) => (
                matches!(ty, SysType::Var(ty) if int_domain(ty).is_some()),
                "a `char`, `i16` or `int`".to_owned(),
            ),
        };
        if !ok {
            let message = format!(
                "`{name}` has type {}, but {want} is expected",
                self.sys_type_name(ty)
            );
            self.report(span, DiagnosticCode::KisArgMismatch, message, Vec::new());
            return None;
        }
        Some(Operand::Var(var))
    }

    // ---- operations -----------------------------------------------------------------------

    /// A `<metafn-*>` or `<interface-*>` tag.
    fn op_tag(&mut self, file: SourceFile, element: &Element, tag: Tag) -> Option<SysOp> {
        self.check_attrs(file, element, tag);
        self.leaf(file, element, tag);
        let kind = OpKind::from_tag(tag)?;
        let site = name_span(file, element);
        if kind == OpKind::FocusMove {
            return self.focus_move_tag(file, element, site);
        }
        let param = match kind {
            OpKind::Start | OpKind::Load | OpKind::Save | OpKind::Select => Some("param"),
            OpKind::Call | OpKind::Goto => Some("page"),
            _ => None,
        };
        let args = match param {
            Some(param) => {
                let value = attr(element, param)?;
                vec![OpArg {
                    text: value.text.trim().to_owned(),
                    span: Span::new(file, value.range),
                }]
            }
            None => Vec::new(),
        };
        self.op(kind, &args, site)
    }

    fn focus_move_tag(&mut self, file: SourceFile, element: &Element, site: Span) -> Option<SysOp> {
        let target = match (attr(element, "direction"), attr(element, "target-idx")) {
            (Some(direction), None) => {
                let span = Span::new(file, direction.range);
                let operand =
                    self.operand(span, direction.text.trim(), Expect::Ty(SysType::Vec2))?;
                FocusTarget::Direction(operand)
            }
            (None, Some(index)) => match index.text.trim() {
                "next" => FocusTarget::Next,
                "prev" => FocusTarget::Prev,
                text => {
                    let span = Span::new(file, index.range);
                    FocusTarget::Index(self.operand(span, text, Expect::Numeric(VarType::Char))?)
                }
            },
            _ => {
                self.report(
                    site,
                    DiagnosticCode::KisVarConflict,
                    "`<interface-focus-move>` needs exactly one of `direction` and `target-idx`",
                    Vec::new(),
                );
                return None;
            }
        };
        Some(SysOp::FocusMove(target))
    }

    /// An operation whose arity is already checked. `site` is the tag or attribute value.
    pub(super) fn op(&mut self, kind: OpKind, args: &[OpArg], site: Span) -> Option<SysOp> {
        let arg = args.first();
        match kind {
            OpKind::Advance => Some(SysOp::Advance),
            OpKind::Quit => Some(SysOp::Quit),
            OpKind::Return => Some(SysOp::Pop),
            OpKind::FocusActivate => Some(SysOp::FocusActivate),
            OpKind::Start => {
                let arg = arg?;
                let scene = self.lookup_scene(arg.span, &arg.text)?;
                if !self.hir.scenes[scene].entry {
                    let note = self.hir.scenes[scene]
                        .span
                        .related(self.db, "scene declared here");
                    self.report(
                        arg.span,
                        DiagnosticCode::KisNotAnEntry,
                        format!(
                            "scene `{}` is not an entry scene; mark it `entry=\"true\"`",
                            arg.text
                        ),
                        vec![note],
                    );
                    return None;
                }
                self.require_game_mode(site, kind)
                    .then_some(SysOp::Start(scene))
            }
            OpKind::Load => {
                let operand = self.slot_operand(arg?);
                let game = self.require_game_mode(site, kind);
                operand.filter(|_| game).map(SysOp::Load)
            }
            OpKind::Save => self.slot_operand(arg?).map(SysOp::Save),
            OpKind::Select => {
                let arg = arg?;
                self.operand(arg.span, &arg.text, Expect::Numeric(VarType::Char))
                    .map(SysOp::Select)
            }
            OpKind::Revert => self
                .history_arg(arg?, "entry.index", kind)
                .then_some(SysOp::Revert),
            OpKind::PlayVoice => self
                .history_arg(arg?, "entry.voice", kind)
                .then_some(SysOp::PlayVoice),
            OpKind::Call => {
                let arg = arg?;
                let mode = self.lookup_mode(arg.span, &arg.text)?;
                let what = match self.hir.system.modes[mode].kind {
                    ModeKind::Page | ModeKind::Dialog => return Some(SysOp::Push(mode)),
                    ModeKind::RootPage => "a root page",
                    ModeKind::Game => "the game mode",
                };
                self.report(
                    arg.span,
                    DiagnosticCode::KisNotAnOverlay,
                    format!(
                        "`{}` is {what}; `{}` needs a non-root `<page>` or a `<dialog>`",
                        arg.text,
                        kind.name()
                    ),
                    Vec::new(),
                );
                None
            }
            OpKind::Goto => {
                let arg = arg?;
                let mode = self.lookup_mode(arg.span, &arg.text)?;
                if self.hir.system.modes[mode].kind == ModeKind::RootPage {
                    return Some(SysOp::Goto(mode));
                }
                self.report(
                    arg.span,
                    DiagnosticCode::KisNotARootPage,
                    format!(
                        "`{}` is not a root page; `{}` needs a `<page root=\"true\">`",
                        arg.text,
                        kind.name()
                    ),
                    Vec::new(),
                );
                None
            }
            OpKind::FocusMove => {
                let arg = arg?;
                let target = match arg.text.as_str() {
                    "next" => FocusTarget::Next,
                    "prev" => FocusTarget::Prev,
                    text if text.chars().any(char::is_whitespace) => {
                        let operand =
                            self.literal_operand(arg.span, text, Expect::Ty(SysType::Vec2))?;
                        FocusTarget::Direction(operand)
                    }
                    text if text.chars().next().is_some_and(|c| c.is_ascii_digit()) => {
                        let operand =
                            self.literal_operand(arg.span, text, Expect::Numeric(VarType::Char))?;
                        FocusTarget::Index(operand)
                    }
                    text => {
                        self.report(
                            arg.span,
                            DiagnosticCode::KisBadValue,
                            format!(
                                "`{text}` is not a focus target; expected `next`, `prev`, an index or a direction such as `0 -1`"
                            ),
                            Vec::new(),
                        );
                        return None;
                    }
                };
                Some(SysOp::FocusMove(target))
            }
        }
    }

    fn require_game_mode(&mut self, site: Span, kind: OpKind) -> bool {
        if self.hir.system.game_mode.is_some() {
            return true;
        }
        self.report(
            site,
            DiagnosticCode::KisNoGameMode,
            format!(
                "`{}` shows the game, but `<system>` declares no `<game-mode>`",
                kind.name()
            ),
            Vec::new(),
        );
        false
    }

    /// A save slot: a literal below the slot count, or a counter variable.
    fn slot_operand(&mut self, arg: &OpArg) -> Option<Operand> {
        let Some(slots) = self.hir.system.save_slots else {
            // Inside `<for-slots>`, the repetition already reported the missing slots.
            let in_for_slots = self
                .ss
                .bindings
                .iter()
                .any(|binding| matches!(binding, Binding::Slot(_)));
            if !self.ss.slots_declared && !in_for_slots {
                self.no_save_slots(arg.span);
            }
            return None;
        };
        let max = i16::try_from(slots.count.saturating_sub(1)).unwrap_or(i16::MAX);
        self.operand(
            arg.span,
            &arg.text,
            Expect::Numeric(VarType::Int { min: 0, max }),
        )
    }

    pub(super) fn no_save_slots(&mut self, span: Span) {
        self.report(
            span,
            DiagnosticCode::KisNoSaveSlots,
            "there are no save slots; declare `<save-slots count>` in `<system>`",
            Vec::new(),
        );
    }

    /// The slot count of `<for-slots>`; `None` (reported once at `span`) without valid
    /// `<save-slots>`.
    pub(super) fn slot_count(&mut self, span: Span) -> Option<u16> {
        let count = self.hir.system.save_slots.map(|slots| slots.count);
        if count.is_none() && !self.ss.slots_declared {
            self.no_save_slots(span);
        }
        count
    }

    /// The argument of `metafn__revert`/`metafn__play_voice`: exactly `expected`, inside
    /// `<for-history>`.
    fn history_arg(&mut self, arg: &OpArg, expected: &str, kind: OpKind) -> bool {
        let message = if arg.text != expected {
            format!("`{}` takes `{expected}`", kind.name())
        } else if !self.ss.bindings.contains(&Binding::History) {
            format!("`{expected}` is only available inside `<for-history>`")
        } else {
            return true;
        };
        self.report(arg.span, DiagnosticCode::KisBadBinding, message, Vec::new());
        false
    }

    // ---- recursion ------------------------------------------------------------------------

    /// Report every cycle of system function calls at its smallest call site.
    pub(super) fn sysfn_recursion(&mut self) {
        let count = self.hir.system.sysfns.len();
        let mut adjacency: Vec<Vec<usize>> = vec![Vec::new(); count];
        for (caller, callee, _) in &self.ss.calls {
            if let Some(successors) = adjacency.get_mut(idx_usize(*caller)) {
                successors.push(idx_usize(*callee));
            }
        }
        let names: Vec<SmolStr> = self
            .hir
            .system
            .sysfns
            .iter()
            .map(|(_, decl)| decl.name.clone())
            .collect();
        for component in tarjan(&adjacency) {
            let inside = |idx: Idx<SysFnDecl>| component.contains(&idx_usize(idx));
            let db = self.db;
            let site = self
                .ss
                .calls
                .iter()
                .filter(|(caller, callee, _)| inside(*caller) && inside(*callee))
                .map(|(_, _, span)| *span)
                .min_by(|a, b| {
                    (a.file.path(db), a.range.start()).cmp(&(b.file.path(db), b.range.start()))
                });
            let Some(site) = site else {
                continue;
            };
            let mut shown: Vec<String> = component
                .iter()
                .filter_map(|index| names.get(*index))
                .map(|name| format!("`{name}`"))
                .collect();
            shown.sort();
            self.report(
                site,
                DiagnosticCode::KisSysfnRecursion,
                format!("recursive system functions: {}", shown.join(", ")),
                Vec::new(),
            );
        }
    }
}
