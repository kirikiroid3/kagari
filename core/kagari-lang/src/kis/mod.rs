//! Stage 2: the schema, names, values and templates of both scopes, lowered to
//! [`hir::ProgramHir`].

pub mod hir;
pub mod lower;
pub mod schema;
pub mod system_hir;
pub mod value;

pub use lower::program_hir;
