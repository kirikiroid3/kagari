//! Where the nodes of an [`ExprStore`](super::ExprStore) are in the source text. Kept out of
//! the store so that semantic queries do not change when only positions do.

use kagari_syntax::TextRange;
use la_arena::ArenaMap;
use rustc_hash::FxHashMap;
use smallvec::SmallVec;

use super::{ExprId, PatId, TypeRefId};
use crate::diagnostic::NodeId;

/// The source ranges of one signature or body.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StoreSourceMap {
    pub exprs: ArenaMap<ExprId, TextRange>,
    pub pats: ArenaMap<PatId, TextRange>,
    pub types: ArenaMap<TypeRefId, TextRange>,
    /// The ranges of the segments of the path a node owns (see [`NodeId`]).
    pub segments: FxHashMap<NodeId, SmallVec<[TextRange; 2]>>,
    /// The names of the generic parameters of a signature.
    pub generic_params: Vec<TextRange>,
}

impl StoreSourceMap {
    pub fn expr(&self, id: ExprId) -> TextRange {
        self.exprs.get(id).copied().unwrap_or_default()
    }

    pub fn pat(&self, id: PatId) -> TextRange {
        self.pats.get(id).copied().unwrap_or_default()
    }

    pub fn type_ref(&self, id: TypeRefId) -> TextRange {
        self.types.get(id).copied().unwrap_or_default()
    }

    /// The range of segment `index` of the path `node` owns.
    pub fn segment(&self, node: NodeId, index: usize) -> Option<TextRange> {
        self.segments.get(&node)?.get(index).copied()
    }
}
