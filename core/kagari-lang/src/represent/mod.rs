//! Stage 3: name resolution and type checking of `.kix`, `.kirp` and inline `<r>` code.

pub mod check;
pub mod infer;
pub mod item_tree;
pub mod scope;
pub mod ty;
