//! The parse query.

use kagari_syntax::{Parse, SourceKind};

use crate::db::Db;
use crate::input::SourceFile;

/// The syntax tree of `file`. `.kis` files parse as plain Kagari, every other extension as
/// `.kix`; a missing file parses as empty.
#[salsa::tracked(returns(ref))]
pub fn parse(db: &dyn Db, file: SourceFile) -> Parse {
    let kind = match file.path(db).extension() {
        Some("kis") => SourceKind::Kis,
        _ => SourceKind::Kix,
    };
    let text = file.text(db).as_deref().unwrap_or("");
    kagari_syntax::parse(text, kind)
}
