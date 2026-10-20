//! Stage 2: the game scope's schema, names, values and templates, lowered to [`hir::GameHir`].

pub mod hir;
pub mod lower;
pub mod schema;
pub mod value;

pub use lower::game_hir;
