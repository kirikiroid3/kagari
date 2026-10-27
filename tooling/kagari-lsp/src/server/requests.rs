//! Request handlers: formatting, go to definition, hover and document symbols.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;

use kagari_lang::ide::{self, NavTarget};
use kagari_lang::{Db, FilePath};
use kagari_syntax::TextSize;
use lsp_server::{ErrorCode, Request, Response};
use lsp_types::request::{
    DocumentSymbolRequest, Formatting, GotoDefinition, HoverRequest, Request as _,
};
use lsp_types::{
    DocumentFormattingParams, DocumentSymbolParams, DocumentSymbolResponse, GotoDefinitionParams,
    GotoDefinitionResponse, Hover, HoverContents, HoverParams, Location, MarkupContent, MarkupKind,
    Position, Range, TextDocumentPositionParams, TextEdit, Uri,
};
use serde_json::Value;

use super::Server;
use crate::convert::{document_symbol, file_uri, source_kind, uri_to_path};
use crate::line_index::LineIndex;

type Handler = fn(&Server, Value) -> Result<Value, serde_json::Error>;

/// Runs the handler of `request`. A panicking handler gets an `InternalError` response.
pub(super) fn dispatch(server: &Server, request: Request) -> Response {
    let handler: Handler = match request.method.as_str() {
        Formatting::METHOD => formatting,
        GotoDefinition::METHOD => definition,
        HoverRequest::METHOD => hover,
        DocumentSymbolRequest::METHOD => document_symbols,
        method => {
            let message = format!("unknown request `{method}`");
            return Response::new_err(request.id, ErrorCode::MethodNotFound as i32, message);
        }
    };
    let id = request.id;
    let params = request.params;
    match catch_unwind(AssertUnwindSafe(|| handler(server, params))) {
        Ok(Ok(result)) => Response::new_ok(id, result),
        Ok(Err(error)) => Response::new_err(id, ErrorCode::InvalidParams as i32, error.to_string()),
        Err(payload) => {
            let message = payload
                .downcast_ref::<&str>()
                .copied()
                .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
                .unwrap_or("non-string panic payload");
            eprintln!("kagari-lsp: `{}` panicked: {message}", request.method);
            Response::new_err(
                id,
                ErrorCode::InternalError as i32,
                "internal error".to_owned(),
            )
        }
    }
}

/// The canonical path of a `.kis`/`.kix` document.
fn source_path(uri: &Uri) -> Option<PathBuf> {
    let path = uri_to_path(uri)?;
    source_kind(&path)?;
    Some(path)
}

fn formatting(server: &Server, params: Value) -> Result<Value, serde_json::Error> {
    let params: DocumentFormattingParams = serde_json::from_value(params)?;
    let edit = (|| {
        let path = uri_to_path(&params.text_document.uri)?;
        let kind = source_kind(&path)?;
        let text = server.text(&path)?;
        let formatted = kagari_fmt::format(&text, kind).ok()?;
        if formatted == *text {
            return None;
        }
        let end = LineIndex::new(&text).position(&text, text_len(&text));
        let range = Range {
            start: Position {
                line: 0,
                character: 0,
            },
            end,
        };
        Some(vec![TextEdit {
            range,
            new_text: formatted,
        }])
    })();
    serde_json::to_value(edit)
}

fn definition(server: &Server, params: Value) -> Result<Value, serde_json::Error> {
    let params: GotoDefinitionParams = serde_json::from_value(params)?;
    let location = (|| {
        let (program, file, offset) = locate(server, &params.text_document_position_params)?;
        let target = ide::definition(&server.db, program, file, offset)?;
        nav_location(server, &target)
    })();
    serde_json::to_value(location.map(GotoDefinitionResponse::Scalar))
}

fn hover(server: &Server, params: Value) -> Result<Value, serde_json::Error> {
    let params: HoverParams = serde_json::from_value(params)?;
    let hover = (|| {
        let position = &params.text_document_position_params;
        let (program, file, offset) = locate(server, position)?;
        let hover = ide::hover(&server.db, program, file, offset)?;
        let text = file.text(&server.db).as_deref()?;
        Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: hover.markdown,
            }),
            range: Some(LineIndex::new(text).range(text, hover.range)),
        })
    })();
    serde_json::to_value(hover)
}

fn document_symbols(server: &Server, params: Value) -> Result<Value, serde_json::Error> {
    let params: DocumentSymbolParams = serde_json::from_value(params)?;
    let symbols = (|| {
        let path = source_path(&params.text_document.uri)?;
        let file = server.db.source_file(FilePath::disk(&path));
        let text = file.text(&server.db).as_deref()?;
        let index = LineIndex::new(text);
        let symbols = ide::file_symbols(&server.db, file)
            .into_iter()
            .map(|symbol| document_symbol(symbol, &index, text))
            .collect();
        Some(DocumentSymbolResponse::Nested(symbols))
    })();
    serde_json::to_value(symbols)
}

/// The project program, file and offset of a position in a project's document.
fn locate(
    server: &Server,
    position: &TextDocumentPositionParams,
) -> Option<(kagari_lang::Program, kagari_lang::SourceFile, TextSize)> {
    let path = source_path(&position.text_document.uri)?;
    let program = server.project_for(&path)?.program;
    let file = server.db.source_file(FilePath::disk(&path));
    let text = file.text(&server.db).as_deref()?;
    let offset = LineIndex::new(text).offset(text, position.position)?;
    Some((program, file, offset))
}

fn nav_location(server: &Server, target: &NavTarget) -> Option<Location> {
    let file = server.db.source_file(target.file.clone());
    let text = file.text(&server.db).as_deref()?;
    Some(Location {
        uri: file_uri(&target.file)?,
        range: LineIndex::new(text).range(text, target.range),
    })
}

fn text_len(text: &str) -> TextSize {
    TextSize::try_from(text.len()).unwrap_or(TextSize::new(u32::MAX))
}
