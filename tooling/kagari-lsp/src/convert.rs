//! Conversions between LSP types and compiler types: URIs, paths and symbols.

use std::path::{Path, PathBuf};
use std::str::FromStr;

use kagari_lang::FilePath;
use kagari_lang::ide::{FileSymbol, SymbolKind};
use kagari_syntax::SourceKind;
use lsp_types::{DocumentSymbol, Uri};

use crate::line_index::LineIndex;
use crate::std_files;

/// The file path of a `file:` URI, canonicalized when it exists.
pub fn uri_to_path(uri: &Uri) -> Option<PathBuf> {
    let path = url::Url::parse(uri.as_str()).ok()?.to_file_path().ok()?;
    Some(std::fs::canonicalize(&path).unwrap_or(path))
}

/// The `file:` URI of an absolute path.
pub fn path_to_uri(path: &Path) -> Option<Uri> {
    let url = url::Url::from_file_path(path).ok()?;
    Uri::from_str(url.as_str()).ok()
}

/// The URI of a compiler file; std files are written to disk first.
pub fn file_uri(file: &FilePath) -> Option<Uri> {
    match file {
        FilePath::Disk(path) => path_to_uri(path),
        FilePath::Std(name) => path_to_uri(&std_files::materialize(name)?),
    }
}

/// The source kind of a `.kis` or `.kix` file; `None` for any other file.
pub fn source_kind(path: &Path) -> Option<SourceKind> {
    SourceKind::from_extension(path.extension()?.to_str()?)
}

pub fn symbol_kind(kind: SymbolKind) -> lsp_types::SymbolKind {
    match kind {
        SymbolKind::Module => lsp_types::SymbolKind::MODULE,
        SymbolKind::Function => lsp_types::SymbolKind::FUNCTION,
        SymbolKind::Method => lsp_types::SymbolKind::METHOD,
        SymbolKind::Struct => lsp_types::SymbolKind::STRUCT,
        SymbolKind::Enum => lsp_types::SymbolKind::ENUM,
        SymbolKind::Variant => lsp_types::SymbolKind::ENUM_MEMBER,
        SymbolKind::TypeAlias => lsp_types::SymbolKind::TYPE_PARAMETER,
        SymbolKind::Const => lsp_types::SymbolKind::CONSTANT,
        SymbolKind::Static => lsp_types::SymbolKind::VARIABLE,
        SymbolKind::Trait => lsp_types::SymbolKind::INTERFACE,
        SymbolKind::Impl => lsp_types::SymbolKind::OBJECT,
    }
}

/// A file symbol and its children as an LSP document symbol.
pub fn document_symbol(symbol: FileSymbol, index: &LineIndex, text: &str) -> DocumentSymbol {
    let children: Vec<DocumentSymbol> = symbol
        .children
        .into_iter()
        .map(|child| document_symbol(child, index, text))
        .collect();
    #[allow(deprecated)]
    DocumentSymbol {
        name: symbol.name,
        detail: None,
        kind: symbol_kind(symbol.kind),
        tags: None,
        deprecated: None,
        range: index.range(text, symbol.full_range),
        selection_range: index.range(text, symbol.focus_range),
        children: (!children.is_empty()).then_some(children),
    }
}
