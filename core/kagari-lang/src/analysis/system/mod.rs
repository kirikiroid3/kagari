//! The mode-graph checks of the game system
//! (`docs/reference/en/src/concepts/system.md#system-checks`).
//!
//! The graph is built from the program description returned by `fn main` (`System::start`,
//! `System::game`, the bindings it builds) and the mode components: every non-generic game
//! `fn` whose signature is `fn() -> RootPage`, `fn() -> Overlay`, `fn() -> Dialog` or
//! `fn(Narrative) -> GameOverlay`, plus the game mode (the `System::game` view).
//!
//! The code of a mode is its component's body plus, transitively, the bodies of the functions
//! (and constants and statics) it calls or refers to as values; closures belong to the body
//! they are written in. A function named directly as the target of a navigation operation is
//! an edge, not code. The exit handler of `nav::start` runs when the story ends, so its code
//! belongs to the game mode.
//!
//! - Reachability: breadth-first from the start page over the operations of each reached mode
//!   and the bindings. An operation whose target is not a named mode reaches every mode of its
//!   kind.
//! - No return from a base: `nav::back()` in the code of a root page or of the game mode.
//! - No trap: a reachable overlay, game overlay or dialog whose code never runs `nav::back`,
//!   `nav::goto`, `nav::start`, `nav::load` or `nav::quit`, unless a `Back` binding exists.
//! - Bounded stack: cycles of pushes (`nav::call`, `nav::dialog`, `Narrative::call` with named
//!   targets) between overlays, game overlays and dialogs. Bindings are player input and never
//!   count as pushes.

pub(crate) mod code;
mod summary;

use std::collections::VecDeque;

use rustc_hash::FxHashSet;

use crate::db::Db;
use crate::def_map::{CrateId, def_maps};
use crate::diagnostic::{Anchor, Diag, DiagnosticCode};
use crate::hir::Name;
use crate::input::{Program, SourceFile};
use crate::item_tree::ItemIndex;
use crate::item_tree::{ItemId, ItemKind, item_tree};
use crate::signatures::{ProgramSignatures, program_signatures};

use code::{BindingAction, Code, Ctx, SystemRoot};

pub(super) fn check(db: &dyn Db, program: Program) -> Vec<Diag> {
    let maps = def_maps(db, program);
    let sigs = program_signatures(db, program);
    let Some(std) = summary::system_std(db, program).as_ref() else {
        return Vec::new();
    };
    let mut ctx = Ctx::new(db, program, maps, std);
    let root = ctx.system_root(sigs);
    let graph = Graph::new(&mut ctx, sigs, root.as_ref());
    let mut diagnostics = Vec::new();
    graph.back_from_base(&mut diagnostics);
    graph.unbounded_stack(&mut diagnostics);
    if let Some(root) = &root {
        graph.reachability(root, &mut diagnostics);
    }
    diagnostics
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ModeKind {
    RootPage,
    Overlay,
    GameOverlay,
    Dialog,
    Game,
}

impl ModeKind {
    fn noun(self) -> &'static str {
        match self {
            ModeKind::RootPage => "root page",
            ModeKind::Overlay => "overlay",
            ModeKind::GameOverlay => "game overlay",
            ModeKind::Dialog => "dialog",
            ModeKind::Game => "game mode",
        }
    }

    /// Overlays, game overlays and dialogs are pushed; root pages and the game are bases.
    fn is_pushed(self) -> bool {
        matches!(
            self,
            ModeKind::Overlay | ModeKind::GameOverlay | ModeKind::Dialog
        )
    }
}

/// A navigation operation of `std::system::nav`, or `Narrative::call`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OpKind {
    Call,
    Dialog,
    Back,
    Goto,
    Start,
    Load,
    Quit,
    GameCall,
}

impl OpKind {
    /// The kind of mode the operation shows.
    fn target_kind(self) -> Option<ModeKind> {
        match self {
            OpKind::Call => Some(ModeKind::Overlay),
            OpKind::Dialog => Some(ModeKind::Dialog),
            OpKind::Goto => Some(ModeKind::RootPage),
            OpKind::Start | OpKind::Load => Some(ModeKind::Game),
            OpKind::GameCall => Some(ModeKind::GameOverlay),
            OpKind::Back | OpKind::Quit => None,
        }
    }

    /// Whether the operation leaves the current mode: pops it or resets the stack.
    fn leaves(self) -> bool {
        matches!(
            self,
            OpKind::Back | OpKind::Goto | OpKind::Start | OpKind::Load | OpKind::Quit
        )
    }

    fn pushes(self) -> bool {
        matches!(self, OpKind::Call | OpKind::Dialog | OpKind::GameCall)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Op {
    kind: OpKind,
    /// The function named as the target, when the argument is a path to one.
    target: Option<ItemId>,
    anchor: Anchor,
    /// Where the operation sits in its file (item index, source order); orders the notes
    /// of a diagnostic.
    order: (ItemIndex, u32),
}

/// A mode of the graph: a component, or the game mode.
struct Node {
    item: ItemId,
    kind: ModeKind,
    name: Name,
    /// Whether the component is game code (diagnostics are reported only there).
    game_code: bool,
    code: Code,
}

struct Graph {
    /// The components, and the game mode when `main` names it, in source order.
    nodes: Vec<Node>,
    /// The start page.
    start: Option<usize>,
    /// Every loaded file with its crate, in load order.
    files: Vec<(SourceFile, CrateId)>,
}

impl Graph {
    fn new(ctx: &mut Ctx<'_>, sigs: &ProgramSignatures, root: Option<&SystemRoot>) -> Graph {
        let files = ctx.maps.files.clone();
        let mut items = Vec::new();
        for &(file, krate) in &files {
            if krate != CrateId::Game {
                continue;
            }
            let tree = item_tree(ctx.db, file);
            for (index, data) in tree.items.iter().enumerate() {
                let ItemKind::Fn { has_body: true, .. } = data.kind else {
                    continue;
                };
                let Some(index) = ItemIndex::from_usize(index) else {
                    continue;
                };
                let item = ItemId { file, index };
                if let Some(kind) = sigs.fn_sig(item).and_then(|sig| ctx.std.mode_kind(sig)) {
                    items.push((item, kind));
                }
            }
        }
        if let Some(root) = root {
            items.push((root.game, ModeKind::Game));
        }
        let order = |item: &ItemId| {
            let file = files
                .iter()
                .position(|(source, _)| *source == item.file)
                .unwrap_or(usize::MAX);
            (file, item.index)
        };
        items.sort_by_key(|(item, _)| order(item));
        let mut nodes = Vec::new();
        for (item, kind) in items {
            let Some(name) = ctx.item_name(item) else {
                continue;
            };
            nodes.push(Node {
                item,
                kind,
                name,
                game_code: files.contains(&(item.file, CrateId::Game)),
                code: ctx.code(&[item]),
            });
        }
        let start = root.and_then(|root| {
            nodes
                .iter()
                .position(|node| node.item == root.start && node.kind == ModeKind::RootPage)
        });
        Graph {
            nodes,
            start,
            files,
        }
    }

    fn node_of(&self, item: ItemId, kind: ModeKind) -> Option<usize> {
        self.nodes
            .iter()
            .position(|node| node.item == item && node.kind == kind)
    }

    fn is_game_file(&self, file: SourceFile) -> bool {
        self.files.contains(&(file, CrateId::Game))
    }

    fn file_order(&self, file: SourceFile) -> usize {
        self.files
            .iter()
            .position(|(source, _)| *source == file)
            .unwrap_or(usize::MAX)
    }

    /// A diagnostic at a mode's name, when the mode is game code.
    fn at_node(&self, node: &Node, code: DiagnosticCode, message: String) -> Option<Diag> {
        node.game_code
            .then(|| Diag::new(Anchor::ItemName(node.item), code, message))
    }

    /// `nav::back()` in the code of a root page or of the game mode, once per call site and
    /// named after the first such mode in source order. Exit handlers run in the game mode.
    fn back_from_base(&self, diagnostics: &mut Vec<Diag>) {
        let game_message = "`nav::back()` in the game mode does nothing: the game mode is a base \
                            of the mode stack";
        let backs = |ops: &[Op]| -> Vec<Op> {
            ops.iter()
                .filter(|op| op.kind == OpKind::Back)
                .copied()
                .collect()
        };
        let mut sites: Vec<(Op, String)> = Vec::new();
        let mut exit_backs = Vec::new();
        for node in &self.nodes {
            match node.kind {
                ModeKind::RootPage => {
                    let message = format!(
                        "`nav::back()` in the root page `{}` does nothing: a root page is a base \
                         of the mode stack",
                        node.name
                    );
                    for op in backs(&node.code.ops) {
                        sites.push((op, message.clone()));
                    }
                    exit_backs.extend(backs(&node.code.exit_ops));
                }
                ModeKind::Game => {
                    for op in backs(&node.code.ops)
                        .into_iter()
                        .chain(backs(&node.code.exit_ops))
                    {
                        sites.push((op, game_message.to_string()));
                    }
                }
                _ => exit_backs.extend(backs(&node.code.exit_ops)),
            }
        }
        sites.extend(
            exit_backs
                .into_iter()
                .map(|op| (op, game_message.to_string())),
        );
        let mut seen = FxHashSet::default();
        for (op, message) in sites {
            if !self.is_game_file(op.anchor.file()) || !seen.insert(op.anchor) {
                continue;
            }
            diagnostics.push(Diag::new(
                op.anchor,
                DiagnosticCode::SystemBackFromBase,
                message,
            ));
        }
    }

    /// Cycles of pushes between overlays, game overlays and dialogs, one warning per
    /// strongly connected component at its first mode in source order.
    fn unbounded_stack(&self, diagnostics: &mut Vec<Diag>) {
        let mut edges: Vec<Vec<(usize, Op)>> = self.nodes.iter().map(|_| Vec::new()).collect();
        for (from, node) in self.nodes.iter().enumerate() {
            if !node.kind.is_pushed() {
                continue;
            }
            for op in node.code.ops.iter().filter(|op| op.kind.pushes()) {
                let (Some(target), Some(kind)) = (op.target, op.kind.target_kind()) else {
                    continue;
                };
                if let Some(to) = self.node_of(target, kind)
                    && let Some(out) = edges.get_mut(from)
                {
                    out.push((to, *op));
                }
            }
        }
        let mut tarjan = Tarjan {
            edges: &edges,
            index: vec![None; self.nodes.len()],
            low: vec![0; self.nodes.len()],
            on_stack: vec![false; self.nodes.len()],
            stack: Vec::new(),
            next: 0,
            components: Vec::new(),
        };
        for node in 0..self.nodes.len() {
            if tarjan.index[node].is_none() {
                tarjan.visit(node);
            }
        }
        let mut components = tarjan.components;
        components.sort();
        for component in components {
            let Some(&first) = component.first() else {
                continue;
            };
            let self_loop = edges[first].iter().any(|(to, _)| *to == first);
            if component.len() < 2 && !self_loop {
                continue;
            }
            let node = &self.nodes[first];
            let message = if component.len() < 2 {
                format!(
                    "the {} `{}` opens itself: the mode stack can grow without bound",
                    node.kind.noun(),
                    node.name
                )
            } else {
                let same_kind = component.iter().all(|&m| self.nodes[m].kind == node.kind);
                let plural = if same_kind {
                    format!("{}s", node.kind.noun())
                } else {
                    "modes".to_string()
                };
                let names: Vec<String> = component
                    .iter()
                    .map(|&m| format!("`{}`", self.nodes[m].name))
                    .collect();
                let names = match names.split_last() {
                    Some((last, rest)) if !rest.is_empty() => {
                        format!("{} and {last}", rest.join(", "))
                    }
                    _ => names.concat(),
                };
                format!(
                    "the {plural} {names} open each other: the mode stack can grow without bound"
                )
            };
            let mut notes: Vec<(usize, (ItemIndex, u32), Anchor, String)> = Vec::new();
            for &from in &component {
                for (to, op) in &edges[from] {
                    if !component.contains(to) || !self.is_game_file(op.anchor.file()) {
                        continue;
                    }
                    let from_name = &self.nodes[from].name;
                    let note = if *to == from {
                        format!("`{from_name}` opens itself here")
                    } else {
                        format!("`{from_name}` opens `{}` here", self.nodes[*to].name)
                    };
                    notes.push((self.file_order(op.anchor.file()), op.order, op.anchor, note));
                }
            }
            notes.sort_by(|a, b| (a.0, a.1, &a.3).cmp(&(b.0, b.1, &b.3)));
            notes.dedup();
            let Some(mut diagnostic) =
                self.at_node(node, DiagnosticCode::SystemUnboundedStack, message)
            else {
                continue;
            };
            for (_, _, anchor, note) in notes {
                diagnostic = diagnostic.with_related(anchor, note);
            }
            diagnostics.push(diagnostic);
        }
    }

    /// Modes no path from the start page shows, and reachable pushed modes that cannot be
    /// left.
    fn reachability(&self, root: &SystemRoot, diagnostics: &mut Vec<Diag>) {
        let Some(start) = self.start else {
            return;
        };
        let mut reached = vec![false; self.nodes.len()];
        let mut queue = VecDeque::new();
        mark(&mut reached, &mut queue, start);
        let (mut opened, mut opened_in_game) = (false, false);
        while let Some(current) = queue.pop_front() {
            let node = &self.nodes[current];
            if !opened {
                opened = true;
                for binding in &root.bindings {
                    if let BindingAction::Open(target) = binding {
                        self.reach(ModeKind::Overlay, *target, &mut reached, &mut queue);
                    }
                }
            }
            if !opened_in_game && matches!(node.kind, ModeKind::Game | ModeKind::GameOverlay) {
                opened_in_game = true;
                for binding in &root.bindings {
                    if let BindingAction::OpenInGame(target) = binding {
                        self.reach(ModeKind::GameOverlay, *target, &mut reached, &mut queue);
                    }
                }
            }
            for op in node.code.ops.iter().chain(&node.code.exit_ops) {
                if let Some(kind) = op.kind.target_kind() {
                    self.reach(kind, op.target, &mut reached, &mut queue);
                }
            }
        }
        let back_binding = root
            .bindings
            .iter()
            .any(|binding| matches!(binding, BindingAction::Back));
        for (node, &reached) in self.nodes.iter().zip(&reached) {
            let diagnostic = if !reached {
                let message = match node.kind {
                    ModeKind::Game => "the game is never started: no reachable mode calls \
                                       `nav::start` or `nav::load`"
                        .to_string(),
                    ModeKind::RootPage => format!(
                        "the root page `{}` is never shown: no reachable mode goes to it",
                        node.name
                    ),
                    kind => format!(
                        "the {} `{}` is never shown: no reachable mode opens it",
                        kind.noun(),
                        node.name
                    ),
                };
                self.at_node(node, DiagnosticCode::SystemUnreachableMode, message)
            } else if node.kind.is_pushed()
                && !back_binding
                && !node.code.ops.iter().any(|op| op.kind.leaves())
            {
                let message = format!(
                    "the {} `{}` cannot be left: none of its handlers calls `nav::back`, \
                     `nav::goto`, `nav::start`, `nav::load` or `nav::quit`",
                    node.kind.noun(),
                    node.name
                );
                self.at_node(node, DiagnosticCode::SystemTrappedMode, message)
            } else {
                None
            };
            diagnostics.extend(diagnostic);
        }
    }

    /// Shows the mode `target` of `kind`, or every mode of `kind` when the target is not a
    /// named mode.
    fn reach(
        &self,
        kind: ModeKind,
        target: Option<ItemId>,
        reached: &mut [bool],
        queue: &mut VecDeque<usize>,
    ) {
        if let Some(node) = target.and_then(|target| self.node_of(target, kind)) {
            mark(reached, queue, node);
            return;
        }
        for (index, node) in self.nodes.iter().enumerate() {
            if node.kind == kind {
                mark(reached, queue, index);
            }
        }
    }
}

fn mark(reached: &mut [bool], queue: &mut VecDeque<usize>, node: usize) {
    if let Some(slot) = reached.get_mut(node)
        && !*slot
    {
        *slot = true;
        queue.push_back(node);
    }
}

/// Tarjan's strongly connected components over the push graph.
struct Tarjan<'a> {
    edges: &'a [Vec<(usize, Op)>],
    index: Vec<Option<usize>>,
    low: Vec<usize>,
    on_stack: Vec<bool>,
    stack: Vec<usize>,
    next: usize,
    /// Each component's members in source order.
    components: Vec<Vec<usize>>,
}

impl Tarjan<'_> {
    fn visit(&mut self, node: usize) {
        self.index[node] = Some(self.next);
        self.low[node] = self.next;
        self.next = self.next.saturating_add(1);
        self.stack.push(node);
        self.on_stack[node] = true;
        let edges = self.edges;
        for &(to, _) in &edges[node] {
            match self.index[to] {
                None => {
                    self.visit(to);
                    self.low[node] = self.low[node].min(self.low[to]);
                }
                Some(index) if self.on_stack[to] => {
                    self.low[node] = self.low[node].min(index);
                }
                Some(_) => {}
            }
        }
        if self.index[node] == Some(self.low[node]) {
            let mut component = Vec::new();
            while let Some(member) = self.stack.pop() {
                self.on_stack[member] = false;
                component.push(member);
                if member == node {
                    break;
                }
            }
            component.sort_unstable();
            self.components.push(component);
        }
    }
}
