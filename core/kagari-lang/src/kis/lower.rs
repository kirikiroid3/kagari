//! Lowering of the collected game elements to [`GameHir`]: the schema and content checks,
//! name resolution, value parsing and template expansion of stage 2.

use kagari_syntax::script::ast::{Content, DecodedText, Element, decode_content};
use kagari_syntax::{TextRange, TextSize};
use la_arena::Idx;
use rowan::ast::AstNode;
use rustc_hash::FxHashMap;
use smol_str::SmolStr;

use crate::collect::{TopElement, collect};
use crate::db::Db;
use crate::diagnostic::{Diagnostic, DiagnosticCode, Related, Span};
use crate::exhaustive::{self, TyKind, Types, Witness};
use crate::input::{Program, SourceFile, resolve_src};
use crate::kis::hir::{
    Block, BlockId, ChoiceArm, ChoiceItem, EntryDecl, EnumDecl, GameHir, LabelDecl, MatchArm,
    MatchStmt, MetaDecl, MetaType, Render, SceneDecl, Segment, Stmt, StmtKind, TextPiece, VarDecl,
    VarScope, VariantDecl,
};
use crate::kis::schema::{Tag, attrs, tag};
use crate::kis::value::{
    Pattern, Value, ValueError, VarType, check_name, int_domain, parse_amount, parse_i16,
    parse_pattern, parse_repr, parse_value, split_list,
};

/// Stage 2 for the whole game scope.
#[salsa::tracked(returns(ref))]
pub fn game_hir(db: &dyn Db, program: Program) -> GameHir {
    let collected = collect(db, program);
    let mut lowerer = Lowerer::new(db);
    for container in &collected.game_containers {
        lowerer.container_text(container);
    }
    let mut top = TopLevel::default();
    for item in &collected.game_top {
        lowerer.top_element(item, &mut top);
    }
    for item in &collected.system_meta {
        lowerer.meta_decl(item);
    }
    for (file, element, tag) in &top.globals {
        lowerer.global_var(*file, element, *tag);
    }
    for (scene, file, element) in &top.scenes {
        lowerer.scene(*scene, *file, element);
    }
    for (file, element) in &top.entries {
        lowerer.entry(*file, element);
    }
    if top.entries.is_empty()
        && let Some(game) = &collected.game
        && let Some(element) = game.element(db)
    {
        let span = name_span(game.file, &element);
        lowerer.report(
            span,
            DiagnosticCode::KisNoEntry,
            "`<game>` has no `<entry>`; declare at least one start scene",
            Vec::new(),
        );
    }
    lowerer.hir
}

/// Top-level elements whose lowering waits for later passes.
#[derive(Default)]
struct TopLevel {
    globals: Vec<(SourceFile, Element, Tag)>,
    scenes: Vec<(Idx<SceneDecl>, SourceFile, Element)>,
    entries: Vec<(SourceFile, Element)>,
}

/// A declared variable. `decl` is `None` when its declaration is invalid; uses of it are
/// dropped silently to avoid cascading errors.
#[derive(Clone, Copy)]
struct VarSlot {
    decl: Option<Idx<VarDecl>>,
    span: Span,
}

#[derive(Clone, Copy)]
struct MetaSlot {
    decl: Option<Idx<MetaDecl>>,
    span: Span,
}

struct Template {
    element: TopElement,
    span: Span,
    /// The highest valid slot `pos`.
    max_pos: u32,
}

/// One active `<x>` expansion.
struct Frame {
    template: SmolStr,
    snips: Vec<Element>,
    /// The file of the `<x>` element (and its snips).
    file: SourceFile,
    /// The environment the `<x>` was lowered in.
    parent: Option<usize>,
    use_span: Span,
}

struct PendingGoto {
    label: String,
    span: Span,
    ancestry: Vec<BlockId>,
    notes: Vec<Related>,
}

/// State of the scene being lowered.
#[derive(Default)]
struct SceneState {
    locals: FxHashMap<SmolStr, VarSlot>,
    labels: Vec<LabelDecl>,
    label_ids: FxHashMap<SmolStr, u32>,
    choice_ids: FxHashMap<SmolStr, Span>,
    next_block: u32,
    ancestry: Vec<BlockId>,
    gotos: Vec<PendingGoto>,
    frames: Vec<Frame>,
    /// The innermost template expansion whose body is being lowered.
    env: Option<usize>,
}

#[derive(Default)]
struct SayBuilder {
    segments: Vec<Segment>,
    renders: Vec<Idx<Render>>,
    narration: Vec<TextPiece>,
}

impl SayBuilder {
    fn flush(&mut self) {
        if !self.narration.is_empty() {
            self.segments
                .push(Segment::Narration(std::mem::take(&mut self.narration)));
        }
    }
}

#[derive(Default)]
struct ChoiceBuilder {
    renders: Vec<Idx<Render>>,
    items: Vec<ChoiceItem>,
    arm_ids: FxHashMap<SmolStr, Span>,
}

/// A child of an element: a run of character data (text, references, CDATA, comments) or an
/// element.
enum Child {
    Text(Vec<Content>),
    Element(Element),
}

/// The value of an attribute and the range to report it at.
struct Attr {
    text: String,
    range: TextRange,
}

struct Lowerer<'db> {
    db: &'db dyn Db,
    hir: GameHir,
    enums: FxHashMap<SmolStr, Idx<EnumDecl>>,
    globals: FxHashMap<SmolStr, VarSlot>,
    metas: FxHashMap<SmolStr, MetaSlot>,
    scenes: FxHashMap<SmolStr, Idx<SceneDecl>>,
    templates: FxHashMap<SmolStr, Template>,
    entries: FxHashMap<SmolStr, Span>,
    inline_renders: FxHashMap<(SourceFile, TextRange), Idx<Render>>,
    file_renders: FxHashMap<SourceFile, Idx<Render>>,
    sc: SceneState,
}

impl<'db> Lowerer<'db> {
    fn new(db: &'db dyn Db) -> Self {
        Lowerer {
            db,
            hir: GameHir::default(),
            enums: FxHashMap::default(),
            globals: FxHashMap::default(),
            metas: FxHashMap::default(),
            scenes: FxHashMap::default(),
            templates: FxHashMap::default(),
            entries: FxHashMap::default(),
            inline_renders: FxHashMap::default(),
            file_renders: FxHashMap::default(),
            sc: SceneState::default(),
        }
    }

    // ---- diagnostics ----------------------------------------------------------------------

    /// Report a diagnostic, adding an "in expansion of template" note for every active
    /// expansion.
    fn report(
        &mut self,
        span: Span,
        code: DiagnosticCode,
        message: impl Into<String>,
        related: Vec<Related>,
    ) {
        let mut diag: Diagnostic = span.diagnostic(self.db, code, message);
        diag.related = related;
        diag.related.extend(self.expansion_notes());
        self.hir.diagnostics.push(diag);
    }

    fn report_value(&mut self, span: Span, err: &ValueError) {
        self.report(span, err.code(), err.message().to_owned(), Vec::new());
    }

    fn duplicate(&mut self, span: Span, first: Span, code: DiagnosticCode, message: String) {
        let note = first.related(self.db, "first defined here");
        self.report(span, code, message, vec![note]);
    }

    fn expansion_notes(&self) -> Vec<Related> {
        self.frame_chain()
            .map(|frame| {
                frame.use_span.related(
                    self.db,
                    format!("in expansion of template `{}`", frame.template),
                )
            })
            .collect()
    }

    fn expansion(&self) -> Vec<Span> {
        self.frame_chain().map(|frame| frame.use_span).collect()
    }

    /// The active expansions, innermost first.
    fn frame_chain(&self) -> impl Iterator<Item = &Frame> {
        let frames = &self.sc.frames;
        std::iter::successors(self.sc.env.and_then(|i| frames.get(i)), move |frame| {
            frame.parent.and_then(|i| frames.get(i))
        })
    }

    // ---- schema helpers -------------------------------------------------------------------

    /// The known tag of `element`, warning about unknown ones.
    fn classify(&mut self, file: SourceFile, element: &Element) -> Option<Tag> {
        let name = element.name_text().unwrap_or_default();
        let found = tag(&name);
        if found.is_none() {
            self.report(
                name_span(file, element),
                DiagnosticCode::KisUnknownElement,
                format!("unknown element `<{name}>` is ignored"),
                Vec::new(),
            );
        }
        found
    }

    fn not_allowed(&mut self, file: SourceFile, element: &Element, tag: Tag, place: &str) {
        let place = if tag == Tag::Slot && self.sc.env.is_none() {
            "outside a template"
        } else {
            place
        };
        self.report(
            name_span(file, element),
            DiagnosticCode::KisElementNotAllowed,
            format!("`<{}>` is not allowed {place}", tag.name()),
            Vec::new(),
        );
    }

    /// Report missing required and unknown attributes. Returns whether every required
    /// attribute is present.
    fn check_attrs(&mut self, file: SourceFile, element: &Element, tag: Tag) -> bool {
        let schema = attrs(tag);
        let mut complete = true;
        for (name, required) in schema {
            if *required && element.attribute(name).is_none() {
                complete = false;
                self.report(
                    name_span(file, element),
                    DiagnosticCode::KisMissingAttribute,
                    format!("`<{}>` requires the `{name}` attribute", tag.name()),
                    Vec::new(),
                );
            }
        }
        for attribute in element.attributes() {
            let Some(name) = attribute.name_text() else {
                continue;
            };
            if !schema.iter().any(|(known, _)| *known == name) {
                let range = attribute
                    .name_range()
                    .unwrap_or_else(|| element.start_tag_range());
                self.report(
                    Span::new(file, range),
                    DiagnosticCode::KisUnknownAttribute,
                    format!(
                        "unknown attribute `{name}` on `<{}>` is ignored",
                        tag.name()
                    ),
                    Vec::new(),
                );
            }
        }
        complete
    }

    /// Report a run of character data that is not whitespace.
    fn whitespace_only(&mut self, file: SourceFile, run: Vec<Content>, place: &str) {
        let decoded = decode_content(run.into_iter());
        if let Some(range) = significant_range(&decoded) {
            self.report(
                Span::new(file, range),
                DiagnosticCode::KisUnexpectedText,
                format!("text is not allowed {place}"),
                Vec::new(),
            );
        }
    }

    /// Check the content of an element that takes no children.
    fn leaf(&mut self, file: SourceFile, element: &Element, tag: Tag) {
        let place = format!("inside `<{}>`", tag.name());
        for child in children(element) {
            match child {
                Child::Text(run) => self.whitespace_only(file, run, &place),
                Child::Element(child) => {
                    if let Some(child_tag) = self.classify(file, &child) {
                        self.not_allowed(file, &child, child_tag, &place);
                    }
                }
            }
        }
    }

    /// A name-like attribute: present (else `None`, already reported) and checked by
    /// [`check_name`].
    fn name_attr(&mut self, file: SourceFile, element: &Element, attr_name: &str) -> Option<Attr> {
        let attr = attr(element, attr_name)?;
        if !check_name(&attr.text) {
            self.report(
                Span::new(file, attr.range),
                DiagnosticCode::KisBadName,
                format!("`{}` is not a valid name", attr.text),
                Vec::new(),
            );
        }
        Some(attr)
    }

    // ---- top level ------------------------------------------------------------------------

    fn container_text(&mut self, container: &TopElement) {
        let Some(element) = container.element(self.db) else {
            return;
        };
        for child in children(&element) {
            if let Child::Text(run) = child {
                self.whitespace_only(container.file, run, "at the top level of the game");
            }
        }
    }

    fn top_element(&mut self, item: &TopElement, top: &mut TopLevel) {
        let file = item.file;
        let Some(element) = item.element(self.db) else {
            return;
        };
        let Some(tag) = self.classify(file, &element) else {
            return;
        };
        match tag {
            Tag::DeclareEnum => self.declare_enum(file, &element),
            Tag::VarBool | Tag::VarChar | Tag::VarEnum | Tag::VarInt => {
                top.globals.push((file, element, tag));
            }
            Tag::Template => self.template(item, &element),
            Tag::Scene => {
                if let Some(scene) = self.scene_shell(file, &element) {
                    top.scenes.push((scene, file, element));
                }
            }
            Tag::Entry => {
                self.check_attrs(file, &element, tag);
                self.leaf(file, &element, tag);
                top.entries.push((file, element));
            }
            // Consumed by stage 1.
            Tag::Include | Tag::Import => {}
            _ => self.not_allowed(file, &element, tag, "at the top level of the game"),
        }
    }

    fn declare_enum(&mut self, file: SourceFile, element: &Element) {
        self.check_attrs(file, element, Tag::DeclareEnum);
        let name = self.name_attr(file, element, "name");
        let span = tag_span(file, element);
        let mut variants: Vec<VariantDecl> = Vec::new();
        for child in children(element) {
            let child = match child {
                Child::Text(run) => {
                    self.whitespace_only(file, run, "inside `<declare-enum>`");
                    continue;
                }
                Child::Element(child) => child,
            };
            match self.classify(file, &child) {
                Some(Tag::Enum) => {
                    if let Some(variant) = self.variant(file, &child, &variants) {
                        variants.push(variant);
                    }
                }
                Some(other) => self.not_allowed(file, &child, other, "inside `<declare-enum>`"),
                None => {}
            }
        }
        let Some(name) = name else {
            return;
        };
        let name = SmolStr::new(&name.text);
        if let Some(first) = self.enums.get(&name).map(|idx| self.hir.enums[*idx].span) {
            let msg = format!("enum `{name}` is already declared");
            self.duplicate(span, first, DiagnosticCode::KisDuplicateEnum, msg);
            return;
        }
        let idx = self.hir.enums.alloc(EnumDecl {
            name: name.clone(),
            span,
            variants,
        });
        self.enums.insert(name, idx);
    }

    fn variant(
        &mut self,
        file: SourceFile,
        element: &Element,
        variants: &[VariantDecl],
    ) -> Option<VariantDecl> {
        self.check_attrs(file, element, Tag::Enum);
        self.leaf(file, element, Tag::Enum);
        let name = self.name_attr(file, element, "name")?;
        let repr = attr(element, "repr")?;
        let span = tag_span(file, element);
        let repr_value = match parse_repr(&repr.text) {
            Ok(value) => value,
            Err(err) => {
                self.report_value(Span::new(file, repr.range), &err);
                return None;
            }
        };
        if let Some(first) = variants.iter().find(|v| v.name == name.text) {
            let msg = format!("variant `{}` is already declared", name.text);
            self.duplicate(span, first.span, DiagnosticCode::KisDuplicateVariant, msg);
            return None;
        }
        if let Some(first) = variants.iter().find(|v| v.repr == repr_value) {
            let msg = format!("repr {repr_value} is already used by `{}`", first.name);
            let first_span = first.span;
            self.duplicate(
                Span::new(file, repr.range),
                first_span,
                DiagnosticCode::KisDuplicateRepr,
                msg,
            );
        }
        Some(VariantDecl {
            name: SmolStr::new(&name.text),
            repr: repr_value,
            span,
        })
    }

    fn template(&mut self, item: &TopElement, element: &Element) {
        let file = item.file;
        self.check_attrs(file, element, Tag::Template);
        let name = self.name_attr(file, element, "name");
        let mut max_pos: u32 = 0;
        for slot in element
            .syntax()
            .descendants()
            .filter_map(Element::cast)
            .filter(|el| el.is("slot"))
        {
            let Some(pos) = attr(&slot, "pos") else {
                continue;
            };
            match slot_pos(&pos.text) {
                Some(value) => max_pos = max_pos.max(value),
                None => self.report(
                    Span::new(file, pos.range),
                    DiagnosticCode::KisBadValue,
                    format!("slot position `{}` is not a positive integer", pos.text),
                    Vec::new(),
                ),
            }
        }
        let Some(name) = name else {
            return;
        };
        let name = SmolStr::new(&name.text);
        let span = tag_span(file, element);
        if let Some(first) = self.templates.get(&name).map(|t| t.span) {
            let msg = format!("template `{name}` is already declared");
            self.duplicate(span, first, DiagnosticCode::KisDuplicateTemplate, msg);
            return;
        }
        self.templates.insert(
            name,
            Template {
                element: item.clone(),
                span,
                max_pos,
            },
        );
    }

    /// Allocate a scene. Scenes without a name or with a duplicate name are still lowered,
    /// but cannot be referenced.
    fn scene_shell(&mut self, file: SourceFile, element: &Element) -> Option<Idx<SceneDecl>> {
        self.check_attrs(file, element, Tag::Scene);
        let name = self.name_attr(file, element, "name");
        let span = tag_span(file, element);
        let name = name
            .map(|attr| SmolStr::new(&attr.text))
            .unwrap_or_default();
        let duplicate = self.scenes.get(&name).map(|idx| self.hir.scenes[*idx].span);
        if let Some(first) = duplicate {
            let msg = format!("scene `{name}` is already declared");
            self.duplicate(span, first, DiagnosticCode::KisDuplicateScene, msg);
        }
        let idx = self.hir.scenes.alloc(SceneDecl {
            name: name.clone(),
            span,
            locals: Vec::new(),
            labels: Vec::new(),
            body: Block {
                id: BlockId(0),
                stmts: Vec::new(),
            },
        });
        if duplicate.is_none() && !name.is_empty() {
            self.scenes.insert(name, idx);
        }
        Some(idx)
    }

    fn meta_decl(&mut self, item: &TopElement) {
        let file = item.file;
        let Some(element) = item.element(self.db) else {
            return;
        };
        let Some(tag) = tag(&element.name_text().unwrap_or_default()) else {
            return;
        };
        self.check_attrs(file, &element, tag);
        self.leaf(file, &element, tag);
        let Some(name) = self.name_attr(file, &element, "name") else {
            return;
        };
        let name = SmolStr::new(&name.text);
        let span = tag_span(file, &element);
        if let Some(first) = self.metas.get(&name).map(|slot| slot.span) {
            let msg = format!("meta field `{name}` is already declared");
            self.duplicate(span, first, DiagnosticCode::KisDuplicateMeta, msg);
            return;
        }
        let ty = match tag {
            Tag::MetaBool => Some(MetaType::Bool),
            Tag::MetaChar => Some(MetaType::Char),
            Tag::MetaInt => self
                .int_bounds(file, &element)
                .map(|(min, max)| MetaType::Int { min, max }),
            _ => Some(MetaType::Enum),
        };
        let decl = ty.map(|ty| {
            if let (Some(var_ty), Some(init)) = (meta_var_type(ty), attr(&element, "init"))
                && let Err(err) = parse_value(var_ty, &self.hir.enums, &init.text)
            {
                self.report_value(Span::new(file, init.range), &err);
            }
            self.hir.metas.alloc(MetaDecl {
                name: name.clone(),
                span,
                ty,
            })
        });
        self.metas.insert(name, MetaSlot { decl, span });
    }

    /// The `min`/`max` of a `<var-int>`/`<meta-int>`.
    fn int_bounds(&mut self, file: SourceFile, element: &Element) -> Option<(i16, i16)> {
        let min_attr = attr(element, "min")?;
        let max_attr = attr(element, "max")?;
        let min = parse_i16(&min_attr.text);
        let max = parse_i16(&max_attr.text);
        let (min, max) = match (min, max) {
            (Ok(min), Ok(max)) => (min, max),
            (min, max) => {
                for (result, range) in [(min, min_attr.range), (max, max_attr.range)] {
                    if let Err(err) = result {
                        self.report_value(Span::new(file, range), &err);
                    }
                }
                return None;
            }
        };
        if min > max {
            self.report(
                Span::new(file, min_attr.range),
                DiagnosticCode::KisBadRange,
                format!("`min` ({min}) is greater than `max` ({max})"),
                Vec::new(),
            );
            return None;
        }
        Some((min, max))
    }

    // ---- variables ------------------------------------------------------------------------

    fn global_var(&mut self, file: SourceFile, element: &Element, tag: Tag) {
        self.check_attrs(file, element, tag);
        self.leaf(file, element, tag);
        let Some(name) = self.name_attr(file, element, "name") else {
            return;
        };
        let name = SmolStr::new(&name.text);
        let span = tag_span(file, element);
        if let Some(first) = self.globals.get(&name).map(|slot| slot.span) {
            let msg = format!("variable `{name}` is already declared");
            self.duplicate(span, first, DiagnosticCode::KisDuplicateVariable, msg);
            return;
        }
        let decl = self.var_decl(file, element, tag, &name, VarScope::Global);
        self.globals.insert(name, VarSlot { decl, span });
    }

    /// Type, initial value and `from-meta` of a variable declaration.
    fn var_decl(
        &mut self,
        file: SourceFile,
        element: &Element,
        tag: Tag,
        name: &SmolStr,
        scope: VarScope,
    ) -> Option<Idx<VarDecl>> {
        let span = tag_span(file, element);
        let ty = match tag {
            Tag::VarBool => VarType::Bool,
            Tag::VarChar => VarType::Char,
            Tag::VarInt => {
                let (min, max) = self.int_bounds(file, element)?;
                VarType::Int { min, max }
            }
            _ => {
                let type_attr = attr(element, "type")?;
                let Some(idx) = self.enums.get(type_attr.text.as_str()).copied() else {
                    self.report(
                        Span::new(file, type_attr.range),
                        DiagnosticCode::KisUnknownEnum,
                        format!("unknown enum `{}`", type_attr.text),
                        Vec::new(),
                    );
                    return None;
                };
                VarType::Enum(idx)
            }
        };
        let init_attr = attr(element, "init")?;
        let init = match parse_value(ty, &self.hir.enums, &init_attr.text) {
            Ok(value) => value,
            Err(err) => {
                self.report_value(Span::new(file, init_attr.range), &err);
                default_value(ty)
            }
        };
        let from_meta =
            attr(element, "from-meta").and_then(|meta| self.copied_meta(file, &meta, ty));
        Some(self.hir.vars.alloc(VarDecl {
            name: name.clone(),
            span,
            ty,
            init,
            scope,
            from_meta,
        }))
    }

    /// Resolve the `from-meta` of a variable of type `ty`.
    fn copied_meta(&mut self, file: SourceFile, meta: &Attr, ty: VarType) -> Option<Idx<MetaDecl>> {
        let span = Span::new(file, meta.range);
        let idx = self.lookup_meta(span, &meta.text)?;
        let meta_decl = &self.hir.metas[idx];
        let meta_ty = meta_decl.ty;
        let meta_span = meta_decl.span;
        let Some(meta_var_ty) = meta_var_type(meta_ty) else {
            self.report(
                span,
                DiagnosticCode::KisMetaEnumUnsupported,
                format!("meta enum `{}` cannot be used from the game yet", meta.text),
                Vec::new(),
            );
            return None;
        };
        if meta_var_ty != ty {
            let note = meta_span.related(self.db, "meta field declared here");
            self.report(
                span,
                DiagnosticCode::KisMetaTypeMismatch,
                format!(
                    "meta field `{}` has type {}, but the variable has type {}",
                    meta.text,
                    self.type_name(meta_var_ty),
                    self.type_name(ty)
                ),
                vec![note],
            );
            return None;
        }
        Some(idx)
    }

    fn lookup_meta(&mut self, span: Span, name: &str) -> Option<Idx<MetaDecl>> {
        match self.metas.get(name) {
            Some(slot) => slot.decl,
            None => {
                self.report(
                    span,
                    DiagnosticCode::KisUnknownMeta,
                    format!("unknown meta field `{name}`"),
                    Vec::new(),
                );
                None
            }
        }
    }

    fn type_name(&self, ty: VarType) -> String {
        match ty {
            VarType::Bool => "`bool`".to_owned(),
            VarType::Char => "`char`".to_owned(),
            VarType::Int { min, max } => format!("`int` {min}..={max}"),
            VarType::Enum(idx) => format!("enum `{}`", self.hir.enums[idx].name),
        }
    }

    /// Resolve a variable: the scene's locals, then the globals.
    fn lookup_var(&mut self, span: Span, name: &str) -> Option<Idx<VarDecl>> {
        match self.sc.locals.get(name).or_else(|| self.globals.get(name)) {
            Some(slot) => slot.decl,
            None => {
                self.report(
                    span,
                    DiagnosticCode::KisUnknownVariable,
                    format!("unknown variable `{name}`"),
                    Vec::new(),
                );
                None
            }
        }
    }

    // ---- scenes ---------------------------------------------------------------------------

    fn scene(&mut self, scene: Idx<SceneDecl>, file: SourceFile, element: &Element) {
        self.sc = SceneState::default();
        let mut locals = Vec::new();
        for child in element.child_elements() {
            let Some(tag) = tag(&child.name_text().unwrap_or_default()).filter(|t| t.is_var())
            else {
                continue;
            };
            self.check_attrs(file, &child, tag);
            self.leaf(file, &child, tag);
            let Some(name) = self.name_attr(file, &child, "name") else {
                continue;
            };
            let name = SmolStr::new(&name.text);
            let span = tag_span(file, &child);
            if let Some(first) = self.sc.locals.get(&name).map(|slot| slot.span) {
                let msg = format!("variable `{name}` is already declared in this scene");
                self.duplicate(span, first, DiagnosticCode::KisDuplicateVariable, msg);
                continue;
            }
            if let Some(global) = self.globals.get(&name).map(|slot| slot.span) {
                let note = global.related(self.db, "global declared here");
                self.report(
                    span,
                    DiagnosticCode::KisLocalShadowsGlobal,
                    format!("local variable `{name}` shadows a global variable"),
                    vec![note],
                );
            }
            let decl = self.var_decl(file, &child, tag, &name, VarScope::Local(scene));
            locals.extend(decl);
            self.sc.locals.insert(name, VarSlot { decl, span });
        }

        let mut body = self.block(file, element, true);
        let resolved = self.resolve_gotos();
        patch_gotos(&mut body.stmts, &resolved);

        let state = std::mem::take(&mut self.sc);
        let decl = &mut self.hir.scenes[scene];
        decl.locals = locals;
        decl.labels = state.labels;
        decl.body = body;
    }

    fn resolve_gotos(&mut self) -> Vec<Option<u32>> {
        let gotos = std::mem::take(&mut self.sc.gotos);
        let mut resolved = Vec::with_capacity(gotos.len());
        for goto in gotos {
            let target = self.sc.label_ids.get(goto.label.as_str()).copied();
            let label = target.and_then(|i| self.sc.labels.get(usize::try_from(i).ok()?));
            let result = match label {
                None => {
                    let mut diag = goto.span.diagnostic(
                        self.db,
                        DiagnosticCode::KisUnknownLabel,
                        format!("unknown label `{}`", goto.label),
                    );
                    diag.related = goto.notes;
                    self.hir.diagnostics.push(diag);
                    None
                }
                Some(label) if !goto.ancestry.contains(&label.block) => {
                    let note = label.span.related(self.db, "label declared here");
                    let mut diag = goto.span.diagnostic(
                        self.db,
                        DiagnosticCode::KisLabelOutOfScope,
                        format!(
                            "label `{}` is in a block that does not enclose this `<goto>`",
                            goto.label
                        ),
                    );
                    diag.related = vec![note];
                    diag.related.extend(goto.notes);
                    self.hir.diagnostics.push(diag);
                    None
                }
                Some(_) => target,
            };
            resolved.push(result);
        }
        resolved
    }

    fn new_block(&mut self) -> BlockId {
        let id = BlockId(self.sc.next_block);
        self.sc.next_block = self.sc.next_block.saturating_add(1);
        id
    }

    /// Lower the children of `element` as a fresh block nested in the current one.
    fn block(&mut self, file: SourceFile, element: &Element, direct_scene: bool) -> Block {
        let id = self.new_block();
        self.sc.ancestry.push(id);
        let mut stmts = Vec::new();
        self.body(file, element, direct_scene, &mut stmts);
        self.sc.ancestry.pop();
        Block { id, stmts }
    }

    fn stmt(&self, kind: StmtKind, file: SourceFile, element: &Element) -> Stmt {
        Stmt {
            kind,
            span: tag_span(file, element),
            expansion: self.expansion(),
        }
    }

    /// Scene-body content: steps, instant elements and `<x>`.
    fn body(
        &mut self,
        file: SourceFile,
        parent: &Element,
        direct_scene: bool,
        out: &mut Vec<Stmt>,
    ) {
        const PLACE: &str = "in a scene body";
        for child in children(parent) {
            let element = match child {
                Child::Text(run) => {
                    self.whitespace_only(file, run, PLACE);
                    continue;
                }
                Child::Element(element) => element,
            };
            let Some(tag) = self.classify(file, &element) else {
                continue;
            };
            let kind = match tag {
                Tag::S => Some(self.say(file, &element)),
                Tag::Choice => Some(self.choice(file, &element)),
                Tag::Label => self.label(file, &element),
                Tag::Goto => Some(self.goto(file, &element)),
                Tag::Call => Some(self.call(file, &element)),
                Tag::Return => {
                    self.check_attrs(file, &element, tag);
                    self.leaf(file, &element, tag);
                    Some(StmtKind::Return)
                }
                Tag::Match => self
                    .match_stmt(file, &element, |this, file, body| {
                        this.block(file, body, false)
                    })
                    .map(StmtKind::Match),
                Tag::Set | Tag::Add | Tag::Sub => self.assign(file, &element, tag),
                Tag::MetaSet => self.meta_set(file, &element),
                Tag::X => {
                    self.expand(file, &element, out);
                    None
                }
                Tag::Slot if self.sc.env.is_some() => {
                    if let Some((snip_file, snip, saved)) = self.enter_slot(file, &element) {
                        self.body(snip_file, &snip, false, out);
                        self.sc.env = saved;
                    }
                    None
                }
                // Lowered before the body.
                _ if direct_scene && tag.is_var() => None,
                _ if tag.is_var() => {
                    self.not_allowed(
                        file,
                        &element,
                        tag,
                        "here; local variables must be direct children of `<scene>`",
                    );
                    None
                }
                _ => {
                    self.not_allowed(file, &element, tag, PLACE);
                    None
                }
            };
            if let Some(kind) = kind {
                out.push(self.stmt(kind, file, &element));
            }
        }
    }

    // ---- steps ----------------------------------------------------------------------------

    fn say(&mut self, file: SourceFile, element: &Element) -> StmtKind {
        self.check_attrs(file, element, Tag::S);
        let mut builder = SayBuilder::default();
        self.step_text(file, element, &mut builder);
        builder.flush();
        StmtKind::Say {
            segments: builder.segments,
            renders: builder.renders,
        }
    }

    fn step_text(&mut self, file: SourceFile, parent: &Element, builder: &mut SayBuilder) {
        const PLACE: &str = "inside `<s>`";
        for child in children(parent) {
            let element = match child {
                Child::Text(run) => {
                    builder.narration.extend(text_piece(file, run));
                    continue;
                }
                Child::Element(element) => element,
            };
            let Some(tag) = self.classify(file, &element) else {
                continue;
            };
            match tag {
                Tag::R => builder.renders.push(self.inline_render(file, &element)),
                Tag::Represent => builder.renders.extend(self.file_render(file, &element)),
                Tag::M => {
                    builder.flush();
                    let message = self.message(file, &element);
                    builder.segments.push(message);
                }
                Tag::Slot if self.sc.env.is_some() => {
                    if let Some((snip_file, snip, saved)) = self.enter_slot(file, &element) {
                        self.step_text(snip_file, &snip, builder);
                        self.sc.env = saved;
                    }
                }
                _ => self.not_allowed(file, &element, tag, PLACE),
            }
        }
    }

    fn message(&mut self, file: SourceFile, element: &Element) -> Segment {
        self.check_attrs(file, element, Tag::M);
        let who = attr(element, "who").map(|a| a.text).unwrap_or_default();
        let voice = attr(element, "voice").map(|a| (a.text, Span::new(file, a.range)));
        let mut text = Vec::new();
        self.message_text(file, element, &mut text);
        Segment::Message {
            who,
            voice,
            text,
            span: tag_span(file, element),
        }
    }

    fn message_text(&mut self, file: SourceFile, parent: &Element, text: &mut Vec<TextPiece>) {
        for child in children(parent) {
            let element = match child {
                Child::Text(run) => {
                    text.extend(text_piece(file, run));
                    continue;
                }
                Child::Element(element) => element,
            };
            let Some(tag) = self.classify(file, &element) else {
                continue;
            };
            match tag {
                Tag::Slot if self.sc.env.is_some() => {
                    if let Some((snip_file, snip, saved)) = self.enter_slot(file, &element) {
                        self.message_text(snip_file, &snip, text);
                        self.sc.env = saved;
                    }
                }
                _ => self.not_allowed(file, &element, tag, "inside `<m>`"),
            }
        }
    }

    fn inline_render(&mut self, file: SourceFile, element: &Element) -> Idx<Render> {
        self.check_attrs(file, element, Tag::R);
        for child in element.child_elements() {
            if let Some(tag) = self.classify(file, &child) {
                self.not_allowed(file, &child, tag, "inside `<r>`");
            }
        }
        let range = element.syntax().text_range();
        if let Some(idx) = self.inline_renders.get(&(file, range)) {
            return *idx;
        }
        let idx = self.hir.renders.alloc(Render::Inline { file, range });
        self.inline_renders.insert((file, range), idx);
        idx
    }

    fn file_render(&mut self, file: SourceFile, element: &Element) -> Option<Idx<Render>> {
        self.check_attrs(file, element, Tag::Represent);
        self.leaf(file, element, Tag::Represent);
        let src = attr(element, "src")?;
        let span = Span::new(file, src.range);
        let path = match resolve_src(file.path(self.db), &src.text) {
            Ok(path) => path,
            Err(err) => {
                self.report(
                    span,
                    DiagnosticCode::CollectBadSrc,
                    err.message(),
                    Vec::new(),
                );
                return None;
            }
        };
        if path.extension() != Some("kirp") {
            self.report(
                span,
                DiagnosticCode::CollectBadExtension,
                format!("`{}`: expected a `.kirp` file", src.text),
                Vec::new(),
            );
            return None;
        }
        let kirp = self.db.source_file(path);
        if kirp.text(self.db).is_none() {
            self.report(
                span,
                DiagnosticCode::CollectFileNotFound,
                format!("file `{}` not found", src.text),
                Vec::new(),
            );
            return None;
        }
        if let Some(idx) = self.file_renders.get(&kirp) {
            return Some(*idx);
        }
        let idx = self.hir.renders.alloc(Render::File {
            kirp,
            span: tag_span(file, element),
        });
        self.file_renders.insert(kirp, idx);
        Some(idx)
    }

    fn choice(&mut self, file: SourceFile, element: &Element) -> StmtKind {
        self.check_attrs(file, element, Tag::Choice);
        let id = self.name_attr(file, element, "id");
        let id = match id {
            Some(id) => {
                let span = Span::new(file, id.range);
                let id = SmolStr::new(&id.text);
                if let Some(first) = self.sc.choice_ids.get(&id).copied() {
                    let msg = format!("choice id `{id}` is already used in this scene");
                    self.duplicate(span, first, DiagnosticCode::KisDuplicateChoiceId, msg);
                } else {
                    self.sc.choice_ids.insert(id.clone(), span);
                }
                id
            }
            None => SmolStr::default(),
        };
        let mut builder = ChoiceBuilder::default();
        self.choice_body(file, element, &mut builder);
        StmtKind::Choice {
            id,
            renders: builder.renders,
            items: builder.items,
        }
    }

    fn choice_body(&mut self, file: SourceFile, parent: &Element, builder: &mut ChoiceBuilder) {
        const PLACE: &str = "inside `<choice>`";
        for child in children(parent) {
            let element = match child {
                Child::Text(run) => {
                    self.whitespace_only(file, run, PLACE);
                    continue;
                }
                Child::Element(element) => element,
            };
            let Some(tag) = self.classify(file, &element) else {
                continue;
            };
            match tag {
                Tag::ChoiceArm => {
                    let arm = self.arm(file, &element, &mut builder.arm_ids);
                    builder.items.push(ChoiceItem::Arm(arm));
                }
                Tag::Match => {
                    let arm_ids = &mut builder.arm_ids;
                    let lowered = self.match_stmt(file, &element, |this, file, body| {
                        let mut items = Vec::new();
                        this.choice_do(file, body, &mut items, arm_ids);
                        items
                    });
                    builder.items.extend(lowered.map(ChoiceItem::Match));
                }
                Tag::R => builder.renders.push(self.inline_render(file, &element)),
                Tag::Represent => builder.renders.extend(self.file_render(file, &element)),
                _ => self.not_allowed(file, &element, tag, PLACE),
            }
        }
    }

    fn choice_do(
        &mut self,
        file: SourceFile,
        parent: &Element,
        items: &mut Vec<ChoiceItem>,
        arm_ids: &mut FxHashMap<SmolStr, Span>,
    ) {
        const PLACE: &str = "inside the `<do>` of a match in a `<choice>`";
        for child in children(parent) {
            let element = match child {
                Child::Text(run) => {
                    self.whitespace_only(file, run, PLACE);
                    continue;
                }
                Child::Element(element) => element,
            };
            let Some(tag) = self.classify(file, &element) else {
                continue;
            };
            match tag {
                Tag::ChoiceArm => {
                    let arm = self.arm(file, &element, arm_ids);
                    items.push(ChoiceItem::Arm(arm));
                }
                Tag::Slot if self.sc.env.is_some() => {
                    if let Some((snip_file, snip, saved)) = self.enter_slot(file, &element) {
                        self.choice_do(snip_file, &snip, items, arm_ids);
                        self.sc.env = saved;
                    }
                }
                _ => self.not_allowed(file, &element, tag, PLACE),
            }
        }
    }

    fn arm(
        &mut self,
        file: SourceFile,
        element: &Element,
        arm_ids: &mut FxHashMap<SmolStr, Span>,
    ) -> ChoiceArm {
        self.check_attrs(file, element, Tag::ChoiceArm);
        let id = match self.name_attr(file, element, "id") {
            Some(id) => {
                let span = Span::new(file, id.range);
                let id = SmolStr::new(&id.text);
                if let Some(first) = arm_ids.get(&id).copied() {
                    let msg = format!("arm id `{id}` is already used in this choice");
                    self.duplicate(span, first, DiagnosticCode::KisDuplicateArmId, msg);
                } else {
                    arm_ids.insert(id.clone(), span);
                }
                id
            }
            None => SmolStr::default(),
        };
        let text = attr(element, "text").map(|a| a.text).unwrap_or_default();
        let body = self.block(file, element, false);
        ChoiceArm {
            id,
            text,
            span: tag_span(file, element),
            body,
        }
    }

    // ---- instant elements -----------------------------------------------------------------

    fn label(&mut self, file: SourceFile, element: &Element) -> Option<StmtKind> {
        self.check_attrs(file, element, Tag::Label);
        self.leaf(file, element, Tag::Label);
        let id = self.name_attr(file, element, "id")?;
        let span = tag_span(file, element);
        let id = SmolStr::new(&id.text);
        if let Some(first) = self
            .sc
            .label_ids
            .get(&id)
            .and_then(|i| self.sc.labels.get(usize::try_from(*i).ok()?))
            .map(|label| label.span)
        {
            let msg = format!("label `{id}` is already declared in this scene");
            self.duplicate(span, first, DiagnosticCode::KisDuplicateLabel, msg);
            return None;
        }
        let index = u32::try_from(self.sc.labels.len()).ok()?;
        let block = self.sc.ancestry.last().copied().unwrap_or(BlockId(0));
        self.sc.labels.push(LabelDecl {
            id: id.clone(),
            span,
            block,
        });
        self.sc.label_ids.insert(id, index);
        Some(StmtKind::Label(index))
    }

    /// A `<goto>`. Its target is resolved after the scene: until then the statement holds the
    /// index of its pending entry.
    fn goto(&mut self, file: SourceFile, element: &Element) -> StmtKind {
        self.check_attrs(file, element, Tag::Goto);
        self.leaf(file, element, Tag::Goto);
        let Some(label) = attr(element, "label") else {
            return StmtKind::Goto(None);
        };
        let Ok(pending) = u32::try_from(self.sc.gotos.len()) else {
            return StmtKind::Goto(None);
        };
        let notes = self.expansion_notes();
        self.sc.gotos.push(PendingGoto {
            label: label.text,
            span: Span::new(file, label.range),
            ancestry: self.sc.ancestry.clone(),
            notes,
        });
        StmtKind::Goto(Some(pending))
    }

    fn call(&mut self, file: SourceFile, element: &Element) -> StmtKind {
        self.check_attrs(file, element, Tag::Call);
        self.leaf(file, element, Tag::Call);
        let Some(scene) = attr(element, "scene") else {
            return StmtKind::Call(None);
        };
        let target = self.lookup_scene(Span::new(file, scene.range), &scene.text);
        StmtKind::Call(target)
    }

    fn lookup_scene(&mut self, span: Span, name: &str) -> Option<Idx<SceneDecl>> {
        let found = self.scenes.get(name).copied();
        if found.is_none() {
            self.report(
                span,
                DiagnosticCode::KisUnknownScene,
                format!("unknown scene `{name}`"),
                Vec::new(),
            );
        }
        found
    }

    fn assign(&mut self, file: SourceFile, element: &Element, tag: Tag) -> Option<StmtKind> {
        self.check_attrs(file, element, tag);
        self.leaf(file, element, tag);
        let var_attr = attr(element, "var")?;
        let val = attr(element, "val")?;
        let var = self.lookup_var(Span::new(file, var_attr.range), &var_attr.text)?;
        let ty = self.hir.vars[var].ty;
        let val_span = Span::new(file, val.range);
        if tag == Tag::Set {
            return match parse_value(ty, &self.hir.enums, &val.text) {
                Ok(value) => Some(StmtKind::Set { var, value }),
                Err(err) => {
                    self.report_value(val_span, &err);
                    None
                }
            };
        }
        if int_domain(ty).is_none() {
            self.report(
                Span::new(file, var_attr.range),
                DiagnosticCode::KisNotACounter,
                format!(
                    "`<{}>` needs a `char` or `int` variable, but `{}` has type {}",
                    tag.name(),
                    var_attr.text,
                    self.type_name(ty)
                ),
                Vec::new(),
            );
            return None;
        }
        match parse_amount(ty, &val.text) {
            Ok(amount) if tag == Tag::Add => Some(StmtKind::Add { var, amount }),
            Ok(amount) => Some(StmtKind::Sub { var, amount }),
            Err(err) => {
                self.report_value(val_span, &err);
                None
            }
        }
    }

    fn meta_set(&mut self, file: SourceFile, element: &Element) -> Option<StmtKind> {
        self.check_attrs(file, element, Tag::MetaSet);
        self.leaf(file, element, Tag::MetaSet);
        let var = attr(element, "var")?;
        let val = attr(element, "val")?;
        let span = Span::new(file, var.range);
        let meta = self.lookup_meta(span, &var.text)?;
        let Some(ty) = meta_var_type(self.hir.metas[meta].ty) else {
            self.report(
                span,
                DiagnosticCode::KisMetaEnumUnsupported,
                format!("meta enum `{}` cannot be used from the game yet", var.text),
                Vec::new(),
            );
            return None;
        };
        match parse_value(ty, &self.hir.enums, &val.text) {
            Ok(value) => Some(StmtKind::MetaSet { meta, value }),
            Err(err) => {
                self.report_value(Span::new(file, val.range), &err);
                None
            }
        }
    }

    // ---- match ----------------------------------------------------------------------------

    /// A `<match>`; `lower_do` lowers each `<do>` body. `None` when the variables, a pattern or
    /// the structure is invalid (the bodies are still checked).
    fn match_stmt<B>(
        &mut self,
        file: SourceFile,
        element: &Element,
        mut lower_do: impl FnMut(&mut Self, SourceFile, &Element) -> B,
    ) -> Option<MatchStmt<B>> {
        self.check_attrs(file, element, Tag::Match);
        let mut valid = true;
        let names = match (attr(element, "var"), attr(element, "vars")) {
            (Some(var), None) => Some(vec![(var.text, var.range)]),
            (None, Some(vars)) => Some(
                split_list(&vars.text)
                    .into_iter()
                    .map(|name| (name.to_owned(), vars.range))
                    .collect(),
            ),
            _ => {
                self.report(
                    name_span(file, element),
                    DiagnosticCode::KisVarConflict,
                    "`<match>` needs exactly one of `var` and `vars`",
                    Vec::new(),
                );
                None
            }
        };
        let mut vars: Vec<Idx<VarDecl>> = Vec::new();
        let tys: Option<Vec<VarType>> = names.and_then(|names| {
            if names.is_empty() {
                self.report(
                    name_span(file, element),
                    DiagnosticCode::KisBadValue,
                    "`vars` names no variable",
                    Vec::new(),
                );
                return None;
            }
            let mut ok = true;
            for (name, range) in names {
                match self.lookup_var(Span::new(file, range), &name) {
                    Some(var) => vars.push(var),
                    None => ok = false,
                }
            }
            ok.then(|| vars.iter().map(|var| self.hir.vars[*var].ty).collect())
        });
        if tys.is_none() {
            valid = false;
        }

        let mut arms = Vec::new();
        let mut saw_cases = false;
        for child in children(element) {
            let cases = match child {
                Child::Text(run) => {
                    self.whitespace_only(file, run, "inside `<match>`");
                    continue;
                }
                Child::Element(cases) => cases,
            };
            match self.classify(file, &cases) {
                Some(Tag::Cases) => {
                    saw_cases = true;
                    match self.cases(file, &cases, tys.as_deref(), &mut lower_do) {
                        Some(arm) => arms.push(arm),
                        None => valid = false,
                    }
                }
                Some(other) => {
                    self.not_allowed(file, &cases, other, "inside `<match>`");
                }
                None => {}
            }
        }
        if !saw_cases {
            self.report(
                name_span(file, element),
                DiagnosticCode::KisBadCases,
                "`<match>` needs at least one `<cases>`",
                Vec::new(),
            );
            valid = false;
        }
        let tys = tys.filter(|_| valid)?;
        let rows: Vec<Vec<exhaustive::Pat>> = arms
            .iter()
            .flat_map(|arm| arm.rows.iter())
            .map(|row| row.iter().map(|pattern| to_pat(*pattern)).collect())
            .collect();
        let types = KisTypes {
            enums: &self.hir.enums,
        };
        if let Some(witness) = exhaustive::missing(&types, &tys, &rows) {
            let shown: Vec<String> = tys
                .iter()
                .zip(&witness)
                .map(|(ty, w)| format_witness(&self.hir.enums, *ty, w))
                .collect();
            self.report(
                name_span(file, element),
                DiagnosticCode::KisNonExhaustiveMatch,
                format!(
                    "non-exhaustive `<match>`: `{}` is not covered",
                    shown.join(" ")
                ),
                Vec::new(),
            );
        }
        Some(MatchStmt { vars, arms })
    }

    /// One `<cases>`: `<case>`… then exactly one `<do>`. `None` when it is malformed or a
    /// pattern is invalid.
    fn cases<B>(
        &mut self,
        file: SourceFile,
        element: &Element,
        tys: Option<&[VarType]>,
        lower_do: &mut impl FnMut(&mut Self, SourceFile, &Element) -> B,
    ) -> Option<MatchArm<B>> {
        self.check_attrs(file, element, Tag::Cases);
        let mut case_elements = Vec::new();
        let mut do_element: Option<Element> = None;
        let mut well_formed = true;
        for child in children(element) {
            let child = match child {
                Child::Text(run) => {
                    self.whitespace_only(file, run, "inside `<cases>`");
                    continue;
                }
                Child::Element(child) => child,
            };
            match self.classify(file, &child) {
                Some(Tag::Case) => {
                    if do_element.is_some() {
                        well_formed = false;
                    }
                    case_elements.push(child);
                }
                Some(Tag::Do) => {
                    if do_element.is_some() {
                        well_formed = false;
                    } else {
                        do_element = Some(child);
                    }
                }
                Some(other) => self.not_allowed(file, &child, other, "inside `<cases>`"),
                None => {}
            }
        }
        if case_elements.is_empty() || do_element.is_none() || !well_formed {
            well_formed = false;
            self.report(
                name_span(file, element),
                DiagnosticCode::KisBadCases,
                "`<cases>` needs one or more `<case>` followed by exactly one `<do>`",
                Vec::new(),
            );
        }

        let mut rows = Vec::new();
        let mut rows_ok = true;
        for case in &case_elements {
            self.check_attrs(file, case, Tag::Case);
            self.leaf(file, case, Tag::Case);
            let (Some(val), Some(tys)) = (attr(case, "val"), tys) else {
                rows_ok = false;
                continue;
            };
            match self.row(file, &val, tys) {
                Some(row) => rows.push(row),
                None => rows_ok = false,
            }
        }
        let body = do_element.map(|do_element| {
            self.check_attrs(file, &do_element, Tag::Do);
            lower_do(self, file, &do_element)
        });
        let body = body.filter(|_| well_formed && rows_ok)?;
        Some(MatchArm {
            rows,
            body,
            span: tag_span(file, element),
        })
    }

    fn row(&mut self, file: SourceFile, val: &Attr, tys: &[VarType]) -> Option<Vec<Pattern>> {
        let span = Span::new(file, val.range);
        let parts = split_list(&val.text);
        if parts.len() != tys.len() {
            self.report(
                span,
                DiagnosticCode::KisPatternArity,
                format!(
                    "`{}` has {} pattern(s), but the `<match>` reads {} variable(s)",
                    val.text,
                    parts.len(),
                    tys.len()
                ),
                Vec::new(),
            );
            return None;
        }
        let mut row = Vec::with_capacity(parts.len());
        let mut ok = true;
        for (part, ty) in parts.into_iter().zip(tys) {
            match parse_pattern(*ty, &self.hir.enums, part) {
                Ok(pattern) => row.push(pattern),
                Err(err) => {
                    self.report_value(span, &err);
                    ok = false;
                }
            }
        }
        ok.then_some(row)
    }

    // ---- templates ------------------------------------------------------------------------

    /// Expand an `<x>` in place.
    fn expand(&mut self, file: SourceFile, element: &Element, out: &mut Vec<Stmt>) {
        self.check_attrs(file, element, Tag::X);
        let mut snips = Vec::new();
        for child in children(element) {
            let child = match child {
                Child::Text(run) => {
                    self.whitespace_only(file, run, "inside `<x>`");
                    continue;
                }
                Child::Element(child) => child,
            };
            match self.classify(file, &child) {
                Some(Tag::Snip) => {
                    self.check_attrs(file, &child, Tag::Snip);
                    snips.push(child);
                }
                Some(other) => self.not_allowed(file, &child, other, "inside `<x>`"),
                None => {}
            }
        }
        let Some(tpl) = attr(element, "tpl") else {
            return;
        };
        let tpl_span = Span::new(file, tpl.range);
        let Some((template_element, max_pos, template_span)) = self
            .templates
            .get(tpl.text.as_str())
            .map(|t| (t.element.clone(), t.max_pos, t.span))
        else {
            self.report(
                tpl_span,
                DiagnosticCode::KisUnknownTemplate,
                format!("unknown template `{}`", tpl.text),
                Vec::new(),
            );
            return;
        };
        if self.frame_chain().any(|frame| frame.template == tpl.text) {
            self.report(
                tpl_span,
                DiagnosticCode::KisTemplateRecursion,
                format!("template `{}` uses itself", tpl.text),
                Vec::new(),
            );
            return;
        }
        if usize::try_from(max_pos).ok() != Some(snips.len()) {
            let note = template_span.related(self.db, "template declared here");
            self.report(
                name_span(file, element),
                DiagnosticCode::KisSnipCount,
                format!(
                    "template `{}` has {max_pos} slot(s), but `<x>` gives {} `<snip>`",
                    tpl.text,
                    snips.len()
                ),
                vec![note],
            );
            return;
        }
        let Some(body) = template_element.element(self.db) else {
            return;
        };
        let frame = Frame {
            template: SmolStr::new(&tpl.text),
            snips,
            file,
            parent: self.sc.env,
            use_span: tag_span(file, element),
        };
        let saved = self.sc.env;
        self.sc.env = Some(self.sc.frames.len());
        self.sc.frames.push(frame);
        self.body(template_element.file, &body, false, out);
        self.sc.env = saved;
    }

    /// Enter a `<slot>` of the current expansion: returns the snip to lower (and its file),
    /// with the environment switched to the expansion's parent, plus the environment to
    /// restore afterwards.
    fn enter_slot(
        &mut self,
        file: SourceFile,
        element: &Element,
    ) -> Option<(SourceFile, Element, Option<usize>)> {
        self.check_attrs(file, element, Tag::Slot);
        self.leaf(file, element, Tag::Slot);
        let env = self.sc.env?;
        // An invalid `pos` was reported with the template.
        let pos = slot_pos(&attr(element, "pos")?.text)?;
        let frame = self.sc.frames.get(env)?;
        let index = usize::try_from(pos.checked_sub(1)?).ok()?;
        let snip = frame.snips.get(index)?.clone();
        let snip_file = frame.file;
        let saved = self.sc.env;
        self.sc.env = frame.parent;
        Some((snip_file, snip, saved))
    }

    // ---- entries --------------------------------------------------------------------------

    fn entry(&mut self, file: SourceFile, element: &Element) {
        let name = self.name_attr(file, element, "name");
        let span = tag_span(file, element);
        if let Some(name) = &name {
            let name = SmolStr::new(&name.text);
            if let Some(first) = self.entries.get(&name).copied() {
                let msg = format!("entry `{name}` is already declared");
                self.duplicate(span, first, DiagnosticCode::KisDuplicateEntry, msg);
                return;
            }
            self.entries.insert(name, span);
        }
        let scene = attr(element, "scene")
            .and_then(|scene| self.lookup_scene(Span::new(file, scene.range), &scene.text));
        let exit = attr(element, "exit").and_then(|exit| {
            let slot = self.globals.get(exit.text.as_str()).copied();
            let decl = slot.and_then(|slot| slot.decl);
            let is_enum = decl.is_some_and(|d| matches!(self.hir.vars[d].ty, VarType::Enum(_)));
            if !is_enum && (slot.is_none() || decl.is_some()) {
                self.report(
                    Span::new(file, exit.range),
                    DiagnosticCode::KisBadEntryExit,
                    format!(
                        "`exit` must name a global `<var-enum>`, found `{}`",
                        exit.text
                    ),
                    Vec::new(),
                );
            }
            decl.filter(|_| is_enum)
        });
        if let (Some(name), Some(scene)) = (name, scene) {
            self.hir.entries.push(EntryDecl {
                name: SmolStr::new(&name.text),
                span,
                scene,
                exit,
            });
        }
    }
}

/// Patch every `<goto>` from its pending index to its resolved label.
fn patch_gotos(stmts: &mut [Stmt], resolved: &[Option<u32>]) {
    for stmt in stmts {
        match &mut stmt.kind {
            StmtKind::Goto(target) => {
                *target = target
                    .and_then(|i| resolved.get(usize::try_from(i).ok()?))
                    .copied()
                    .flatten();
            }
            StmtKind::Choice { items, .. } => patch_items(items, resolved),
            StmtKind::Match(m) => {
                for arm in &mut m.arms {
                    patch_gotos(&mut arm.body.stmts, resolved);
                }
            }
            _ => {}
        }
    }
}

fn patch_items(items: &mut [ChoiceItem], resolved: &[Option<u32>]) {
    for item in items {
        match item {
            ChoiceItem::Arm(arm) => patch_gotos(&mut arm.body.stmts, resolved),
            ChoiceItem::Match(m) => {
                for arm in &mut m.arms {
                    patch_items(&mut arm.body, resolved);
                }
            }
        }
    }
}

/// Exhaustiveness view of game variable types.
struct KisTypes<'a> {
    enums: &'a la_arena::Arena<EnumDecl>,
}

impl Types for KisTypes<'_> {
    type Ty = VarType;

    fn kind(&self, ty: &VarType) -> TyKind<VarType> {
        match *ty {
            VarType::Bool => TyKind::Bool,
            VarType::Char => TyKind::Int { min: 0, max: 255 },
            VarType::Int { min, max } => TyKind::Int {
                min: i128::from(min),
                max: i128::from(max),
            },
            VarType::Enum(idx) => TyKind::Enum(vec![Vec::new(); self.enums[idx].variants.len()]),
        }
    }
}

fn to_pat(pattern: Pattern) -> exhaustive::Pat {
    match pattern {
        Pattern::Wildcard => exhaustive::Pat::Wild,
        Pattern::Value(Value::Bool(b)) => exhaustive::Pat::Bool(b),
        Pattern::Value(Value::Variant(index)) => {
            exhaustive::Pat::Variant(usize::try_from(index).unwrap_or(usize::MAX), Vec::new())
        }
        Pattern::Value(Value::Int(v)) => exhaustive::Pat::Int(i128::from(v), i128::from(v)),
        Pattern::Range(lo, hi) => exhaustive::Pat::Int(i128::from(lo), i128::from(hi)),
    }
}

/// A witness in `val` syntax.
fn format_witness(enums: &la_arena::Arena<EnumDecl>, ty: VarType, witness: &Witness) -> String {
    match (witness, ty) {
        (Witness::Bool(b), _) => b.to_string(),
        (Witness::Int(lo, hi), _) => format!("{lo}..={hi}"),
        (Witness::Variant(index, _), VarType::Enum(idx)) => enums[idx]
            .variants
            .get(*index)
            .map_or_else(|| "_".to_owned(), |v| v.name.to_string()),
        _ => "_".to_owned(),
    }
}

fn meta_var_type(ty: MetaType) -> Option<VarType> {
    match ty {
        MetaType::Bool => Some(VarType::Bool),
        MetaType::Char => Some(VarType::Char),
        MetaType::Int { min, max } => Some(VarType::Int { min, max }),
        MetaType::Enum => None,
    }
}

/// The value used when a declaration's `init` is invalid.
fn default_value(ty: VarType) -> Value {
    match ty {
        VarType::Bool => Value::Bool(false),
        VarType::Enum(_) => Value::Variant(0),
        VarType::Char => Value::Int(0),
        VarType::Int { min, .. } => Value::Int(i32::from(min)),
    }
}

/// A slot `pos`: a positive decimal integer.
fn slot_pos(text: &str) -> Option<u32> {
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    text.parse::<u32>().ok().filter(|pos| *pos > 0)
}

fn attr(element: &Element, name: &str) -> Option<Attr> {
    let attribute = element.attribute(name)?;
    let range = attribute
        .value_range()
        .or_else(|| attribute.name_range())
        .unwrap_or_else(|| element.start_tag_range());
    let text = attribute.value().map(|v| v.text).unwrap_or_default();
    Some(Attr { text, range })
}

/// The element's name, for diagnostics about the element as a whole.
fn name_span(file: SourceFile, element: &Element) -> Span {
    Span::new(
        file,
        element
            .name_range()
            .unwrap_or_else(|| element.start_tag_range()),
    )
}

/// The element's start tag, the span recorded in the HIR.
fn tag_span(file: SourceFile, element: &Element) -> Span {
    Span::new(file, element.start_tag_range())
}

fn children(element: &Element) -> Vec<Child> {
    let mut out = Vec::new();
    let mut run = Vec::new();
    for item in element.content() {
        match item {
            Content::Element(child) => {
                if !run.is_empty() {
                    out.push(Child::Text(std::mem::take(&mut run)));
                }
                out.push(Child::Element(child));
            }
            other => run.push(other),
        }
    }
    if !run.is_empty() {
        out.push(Child::Text(run));
    }
    out
}

fn is_xml_space(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\r' | '\n')
}

/// The source range of the decoded text without surrounding whitespace, or `None` when the
/// text is whitespace only.
fn significant_range(decoded: &DecodedText) -> Option<TextRange> {
    let text = &decoded.text;
    let start = text.find(|c| !is_xml_space(c))?;
    let (last, ch) = text.char_indices().rev().find(|(_, c)| !is_xml_space(*c))?;
    let end = last.checked_add(ch.len_utf8())?;
    let start = TextSize::try_from(start).ok()?;
    let end = TextSize::try_from(end).ok()?;
    Some(decoded.map.range_to_source(TextRange::new(start, end)))
}

/// A narration or message piece, or `None` for whitespace-only text.
fn text_piece(file: SourceFile, run: Vec<Content>) -> Option<TextPiece> {
    let decoded = decode_content(run.into_iter());
    let range = significant_range(&decoded)?;
    Some(TextPiece {
        text: decoded.text,
        span: Span::new(file, range),
    })
}
