//! Stage 1: follow includes and imports from the entry file and gather the
//! top-level elements of both scopes and the kix files.

use kagari_std::{PRELUDE, SYSTEM_PRELUDE, StdScope};
use kagari_syntax::script::ScriptLanguage;
use kagari_syntax::script::ast::{Content, Document, Element};
use kagari_syntax::{TextRange, TextSize};
use rowan::ast::{AstNode, SyntaxNodePtr};

use crate::db::Db;
use crate::diagnostic::{Diagnostic, DiagnosticCode};
use crate::input::{FilePath, Program, SourceFile, resolve_src};
use crate::parse::parse_kis;

/// An element of some `.kis` file.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TopElement {
    pub file: SourceFile,
    pub ptr: SyntaxNodePtr<ScriptLanguage>,
}

impl TopElement {
    pub fn new(file: SourceFile, element: &Element) -> TopElement {
        TopElement {
            file,
            ptr: SyntaxNodePtr::new(element.syntax()),
        }
    }

    /// The element, re-resolved in the file's current parse.
    pub fn element(&self, db: &dyn Db) -> Option<Element> {
        let root = parse_kis(db, self.file).syntax_node();
        self.ptr.try_to_node(&root).and_then(Element::cast)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Collected {
    /// The entry and every loaded include (both scopes), deduped, first-seen order.
    pub kis_files: Vec<SourceFile>,
    /// The `<game>` element.
    pub game: Option<TopElement>,
    /// Top-level game children after include splicing, in document order.
    pub game_top: Vec<TopElement>,
    /// The elements whose children form `game_top`: `<game>` and every spliced
    /// `<kagari-game>` root. Stage 2 checks their non-element content.
    pub game_containers: Vec<TopElement>,
    /// The `<system>` element.
    pub system: Option<TopElement>,
    /// Top-level system children after include splicing: the system prelude's first, then
    /// the user's, in document order.
    pub system_top: Vec<TopElement>,
    /// The elements whose children form `system_top`: `<system>` and every spliced
    /// `<kagari-system>` root.
    pub system_containers: Vec<TopElement>,
    /// The prelude first, then the game imports, deduped, first-seen order.
    pub kix_files: Vec<SourceFile>,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Scope {
    Game,
    System,
}

impl Scope {
    fn include_root(self) -> &'static str {
        match self {
            Scope::Game => "kagari-game",
            Scope::System => "kagari-system",
        }
    }
}

#[salsa::tracked(returns(ref))]
pub fn collect(db: &dyn Db, program: Program) -> Collected {
    let entry = *program.entry(db);
    let mut collector = Collector {
        db,
        out: Collected::default(),
        stack: Vec::new(),
    };
    collector.out.kis_files.push(entry);
    collector
        .out
        .kix_files
        .push(db.source_file(FilePath::Std(PRELUDE.into())));
    collector.entry(entry);
    collector.out
}

/// A file on the include stack and where it was included from.
struct Frame {
    path: FilePath,
    included_at: Option<(FilePath, TextRange)>,
}

struct Collector<'db> {
    db: &'db dyn Db,
    out: Collected,
    stack: Vec<Frame>,
}

/// A resolved `src` attribute.
struct Src {
    path: FilePath,
    range: TextRange,
    /// The `src` value as written, for messages.
    text: String,
}

impl Collector<'_> {
    fn error(&mut self, file: SourceFile, range: TextRange, code: DiagnosticCode, msg: String) {
        let diag = Diagnostic::new(file.path(self.db).clone(), range, code, msg);
        self.out.diagnostics.push(diag);
    }

    fn entry(&mut self, entry: SourceFile) {
        let db = self.db;
        let path = entry.path(db).clone();
        let start = TextRange::empty(TextSize::from(0));
        if path.extension() != Some("kis") {
            let msg = "the entry file must be a `.kis` file".to_owned();
            self.error(entry, start, DiagnosticCode::CollectEntryNotKis, msg);
            return;
        }
        if entry.text(db).is_none() {
            let msg = "the entry file was not found".to_owned();
            self.error(entry, start, DiagnosticCode::CollectFileNotFound, msg);
            return;
        }
        let Some(root) = Document::cast(parse_kis(db, entry).syntax_node()).and_then(|d| d.root())
        else {
            let msg = "expected a `<kagari>` root element".to_owned();
            self.error(entry, start, DiagnosticCode::CollectBadRoot, msg);
            return;
        };
        let root_range = root.name_range().unwrap_or(start);
        if !root.is("kagari") {
            let msg = format!("expected `<kagari>`, found `<{}>`", element_name(&root));
            self.error(entry, root_range, DiagnosticCode::CollectBadRoot, msg);
            return;
        }
        let children: Vec<Element> = root.child_elements().collect();
        let systems = children.iter().filter(|el| el.is("system")).count();
        let games = children.iter().filter(|el| el.is("game")).count();
        let has_text = root.content().any(|item| is_significant(&item));
        if systems != 1 || games != 1 || children.len() != 2 || has_text {
            let msg = "`<kagari>` must contain exactly one `<system>` and one `<game>`".to_owned();
            self.error(entry, root_range, DiagnosticCode::CollectBadRoot, msg);
        }
        self.stack.push(Frame {
            path,
            included_at: None,
        });
        if let Some(system) = children.iter().find(|el| el.is("system")) {
            self.system_prelude();
            self.out.system = Some(TopElement::new(entry, system));
            self.walk(entry, system, Scope::System);
        }
        if let Some(game) = children.iter().find(|el| el.is("game")) {
            self.out.game = Some(TopElement::new(entry, game));
            self.walk(entry, game, Scope::Game);
        }
        self.stack.pop();
    }

    /// Walk the bundled input enums into the system scope. A missing or malformed prelude is
    /// an engine bug, so it is not reported.
    fn system_prelude(&mut self) {
        let db = self.db;
        let file = db.source_file(FilePath::Std(SYSTEM_PRELUDE.into()));
        let root = Document::cast(parse_kis(db, file).syntax_node()).and_then(|d| d.root());
        let Some(root) = root.filter(|root| root.is("kagari-system")) else {
            return;
        };
        if !self.out.kis_files.contains(&file) {
            self.out.kis_files.push(file);
        }
        self.walk(file, &root, Scope::System);
    }

    /// Walk the direct children of a scope element (`<game>`, `<system>` or an included root).
    fn walk(&mut self, file: SourceFile, parent: &Element, scope: Scope) {
        let container = TopElement::new(file, parent);
        let containers = match scope {
            Scope::Game => &mut self.out.game_containers,
            Scope::System => &mut self.out.system_containers,
        };
        if !containers.contains(&container) {
            containers.push(container);
        }
        for child in parent.child_elements() {
            if child.is("include") {
                self.include(file, &child, scope);
            } else if child.is("import") {
                self.import(file, &child, scope);
            } else {
                let top = match scope {
                    Scope::Game => &mut self.out.game_top,
                    Scope::System => &mut self.out.system_top,
                };
                top.push(TopElement::new(file, &child));
            }
        }
    }

    /// Read and resolve the `src` attribute of an include/import, checking its extension.
    fn src(&mut self, file: SourceFile, element: &Element, ext: &str) -> Option<Src> {
        let db = self.db;
        report_unknown_attrs(self, file, element);
        let Some(attr) = element.attribute("src") else {
            let range = element.name_range().unwrap_or(element.start_tag_range());
            let msg = format!("`<{}>` requires the `src` attribute", element_name(element));
            self.error(file, range, DiagnosticCode::KisMissingAttribute, msg);
            return None;
        };
        let range = attr
            .value_range()
            .or(attr.name_range())
            .unwrap_or(element.start_tag_range());
        let value = attr.value().map(|v| v.text).unwrap_or_default();
        let path = match resolve_src(file.path(db), &value) {
            Ok(path) => path,
            Err(err) => {
                self.error(
                    file,
                    range,
                    DiagnosticCode::CollectBadSrc,
                    err.message().to_owned(),
                );
                return None;
            }
        };
        if path.extension() != Some(ext) {
            let msg = format!("expected a `.{ext}` file");
            self.error(file, range, DiagnosticCode::CollectBadExtension, msg);
            return None;
        }
        Some(Src {
            path,
            range,
            text: value,
        })
    }

    fn include(&mut self, file: SourceFile, element: &Element, scope: Scope) {
        let db = self.db;
        let Some(Src { path, range, text }) = self.src(file, element, "kis") else {
            return;
        };
        if let Some(frame) = self.stack.iter().find(|frame| frame.path == path) {
            let mut diag = Diagnostic::new(
                file.path(db).clone(),
                range,
                DiagnosticCode::CollectIncludeCycle,
                format!("`{text}` includes itself"),
            );
            if let Some((outer_file, outer_range)) = &frame.included_at {
                diag = diag.with_related(outer_file.clone(), *outer_range, "first included here");
            }
            self.out.diagnostics.push(diag);
            return;
        }
        let included = db.source_file(path.clone());
        if included.text(db).is_none() {
            let msg = format!("file `{text}` not found");
            self.error(file, range, DiagnosticCode::CollectFileNotFound, msg);
            return;
        }
        if !self.out.kis_files.contains(&included) {
            self.out.kis_files.push(included);
        }
        let expected = scope.include_root();
        let root = Document::cast(parse_kis(db, included).syntax_node()).and_then(|d| d.root());
        let Some(root) = root.clone().filter(|root| root.is(expected)) else {
            let found = root
                .as_ref()
                .map(|root| format!("`<{}>`", element_name(root)))
                .unwrap_or_else(|| "no root element".to_owned());
            let msg = format!("expected `<{expected}>`, found {found}");
            self.error(file, range, DiagnosticCode::CollectIncludeRoot, msg);
            return;
        };
        self.stack.push(Frame {
            path,
            included_at: Some((file.path(db).clone(), range)),
        });
        self.walk(included, &root, scope);
        self.stack.pop();
    }

    fn import(&mut self, file: SourceFile, element: &Element, scope: Scope) {
        let db = self.db;
        let Some(Src { path, range, text }) = self.src(file, element, "kix") else {
            return;
        };
        let std_scope = path.std_name().and_then(kagari_std::scope);
        let wrong = match scope {
            Scope::Game => std_scope == Some(StdScope::System),
            Scope::System => std_scope == Some(StdScope::Game),
        };
        if wrong {
            let (this, other) = match scope {
                Scope::Game => ("game", "system"),
                Scope::System => ("system", "game"),
            };
            let msg = format!(
                "`{text}` belongs to the {other} scope and cannot be imported into the {this} scope"
            );
            self.error(file, range, DiagnosticCode::CollectImportScope, msg);
            return;
        }
        if scope == Scope::System {
            return;
        }
        let imported = db.source_file(path.clone());
        if self.out.kix_files.contains(&imported) {
            return;
        }
        self.out.kix_files.push(imported);
        if imported.text(db).is_none() {
            let msg = format!("file `{text}` not found");
            self.error(file, range, DiagnosticCode::CollectFileNotFound, msg);
        }
    }
}

/// Report unknown attributes on `<include>`/`<import>`; these elements never reach stage 2.
fn report_unknown_attrs(collector: &mut Collector<'_>, file: SourceFile, element: &Element) {
    for attr in element.attributes() {
        let Some(name) = attr.name_text() else {
            continue;
        };
        if name != "src" {
            let range = attr.name_range().unwrap_or(element.start_tag_range());
            let msg = format!(
                "unknown attribute `{name}` on `<{}>`",
                element_name(element)
            );
            collector.error(file, range, DiagnosticCode::KisUnknownAttribute, msg);
        }
    }
}

fn element_name(element: &Element) -> String {
    element
        .name_text()
        .map(|name| name.to_string())
        .unwrap_or_default()
}

/// Content other than whitespace text and comments.
fn is_significant(item: &Content) -> bool {
    match item {
        Content::Element(_) | Content::Comment(_) => false,
        Content::Text(token) => !token.text().trim().is_empty(),
        Content::EntityRef(_) | Content::CData(_) => true,
    }
}
