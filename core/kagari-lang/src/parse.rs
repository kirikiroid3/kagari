//! Per-file parse queries.

use kagari_syntax::Parse;
use kagari_syntax::represent::RepresentLanguage;
use kagari_syntax::script::ScriptLanguage;

use crate::db::Db;
use crate::input::SourceFile;

/// The `.kis` parse of `file`; a missing file parses as `""`.
#[salsa::tracked(returns(ref))]
pub fn parse_kis(db: &dyn Db, file: SourceFile) -> Parse<ScriptLanguage> {
    let text = file.text(db).as_deref().unwrap_or("");
    kagari_syntax::script::parse(text)
}

/// The `.kix`/`.kirp` parse of `file`; a missing file parses as `""`.
#[salsa::tracked(returns(ref))]
pub fn parse_represent(db: &dyn Db, file: SourceFile) -> Parse<RepresentLanguage> {
    let text = file.text(db).as_deref().unwrap_or("");
    kagari_syntax::represent::parse_file(text)
}
