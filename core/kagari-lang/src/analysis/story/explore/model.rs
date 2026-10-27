//! The explored state space: contexts, their states and exits, and the interned values
//! they refer to.

use std::hash::Hash;

use rustc_hash::FxHashMap;

use crate::eval::ConstValue;
use crate::hir::ExprId;
use crate::item_tree::ItemId;
use crate::mir::BasicBlock;
use crate::ty::FnRef;

macro_rules! index {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub(in crate::analysis::story) struct $name(pub(in crate::analysis::story) u32);

        #[allow(dead_code)]
        impl $name {
            pub fn idx(self) -> usize {
                self.0 as usize
            }

            /// The id of the `index`th entry; saturates (the state cap stops far below).
            pub fn from_usize(index: usize) -> $name {
                $name(u32::try_from(index).unwrap_or(u32::MAX))
            }
        }
    };
}

index!(
    /// A context: index into [`Explored::contexts`].
    CtxId
);
index!(
    /// A state of a context: index into [`Context::nodes`].
    NodeId
);
index!(
    /// An exit of a context: index into [`Context::exits`].
    ExitId
);
index!(
    /// A value stored in [`Explored::values`].
    ValueId
);
index!(
    /// A frame (the kept locals of a block) stored in [`Explored::frames`].
    FrameId
);
index!(
    /// The story variables of a state, stored in [`Explored::globals`].
    GlobalsId
);

/// Values stored once and referred to by index.
pub(in crate::analysis::story) struct Interner<T> {
    items: Vec<T>,
    index: FxHashMap<T, u32>,
}

impl<T: Clone + Eq + Hash> Interner<T> {
    pub fn new() -> Interner<T> {
        Interner {
            items: Vec::new(),
            index: FxHashMap::default(),
        }
    }

    pub fn intern(&mut self, value: T) -> u32 {
        if let Some(id) = self.index.get(&value) {
            return *id;
        }
        let id = u32::try_from(self.items.len()).unwrap_or(u32::MAX);
        self.items.push(value.clone());
        self.index.insert(value, id);
        id
    }

    pub fn get(&self, id: u32) -> Option<&T> {
        self.items.get(id as usize)
    }
}

/// The values of the exploration, each stored once.
pub(in crate::analysis::story) struct ValueInterner(pub(super) Interner<ConstValue>);

impl ValueInterner {
    pub fn intern(&mut self, value: ConstValue) -> ValueId {
        ValueId(self.0.intern(value))
    }

    pub fn get(&self, id: ValueId) -> Option<&ConstValue> {
        self.0.get(id.0)
    }
}

/// The kept locals of a block, in ascending order; `None` for an uninitialized local.
pub(in crate::analysis::story) type Frame = Box<[Option<ValueId>]>;
/// The story variables, by position in [`Explored::vars`].
pub(in crate::analysis::story) type Globals = imbl::Vector<ValueId>;

/// Where a resumption leads.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(in crate::analysis::story) enum Target {
    Node(NodeId),
    Exit(ExitId),
    /// The path stopped: an error, or a state the analysis could not continue from.
    Dead,
}

pub(in crate::analysis::story) enum NodeKind {
    /// A step with its render and, per outcome (one for `s!`, one per shown arm of a
    /// `choice!`), where the story goes next.
    Step {
        expr: ExprId,
        render: Option<ConstValue>,
        /// Whether the step is a `choice!`.
        choice: bool,
        succ: Vec<Target>,
    },
    /// A scene call: the callee's context and, per exit of it, where the caller goes next.
    Call {
        callee: CtxId,
        succ: Vec<(ExitId, Target)>,
    },
    /// A random draw (`rng_choose`): per value of the drawn type, where the story goes next.
    Rng { succ: Vec<Target> },
}

pub(in crate::analysis::story) struct Node {
    pub block: BasicBlock,
    pub frame: FrameId,
    pub globals: GlobalsId,
    pub kind: NodeKind,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(in crate::analysis::story) struct ExitData {
    pub ret: ValueId,
    pub globals: GlobalsId,
    pub outs: Box<[ValueId]>,
}

pub(in crate::analysis::story) struct Context {
    /// Index into `Scenes::list`.
    pub scene: usize,
    pub fn_ref: FnRef,
    pub args: Box<[ValueId]>,
    pub globals: GlobalsId,
    pub entry: Target,
    pub nodes: Vec<Node>,
    pub(super) node_index: FxHashMap<(BasicBlock, FrameId, GlobalsId), NodeId>,
    pub exits: Vec<ExitData>,
    pub(super) exit_index: FxHashMap<ExitData, ExitId>,
    pub callers: Vec<(CtxId, NodeId)>,
}

impl Context {
    pub fn node(&self, id: NodeId) -> Option<&Node> {
        self.nodes.get(id.idx())
    }

    /// Every node with its id.
    pub fn node_ids(&self) -> impl Iterator<Item = (NodeId, &Node)> {
        self.nodes
            .iter()
            .enumerate()
            .map(|(index, node)| (NodeId::from_usize(index), node))
    }
}

pub(in crate::analysis::story) struct Explored {
    pub contexts: Vec<Context>,
    pub roots: Vec<CtxId>,
    pub values: ValueInterner,
    pub frames: Interner<Frame>,
    pub globals: Interner<Globals>,
    pub vars: Vec<ItemId>,
    /// Every reachable state was visited: no path was given up and no cap was hit.
    pub complete: bool,
}

impl Explored {
    pub fn context(&self, id: CtxId) -> Option<&Context> {
        self.contexts.get(id.idx())
    }
}
