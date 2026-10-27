//! Notification handlers: document sync and watched files.

use std::path::PathBuf;

use kagari_lang::FilePath;
use kagari_lang::def_map::def_maps;
use kagari_project::MANIFEST_FILE;
use lsp_server::Notification;
use lsp_types::notification::{
    DidChangeTextDocument, DidChangeWatchedFiles, DidCloseTextDocument, DidOpenTextDocument,
    Notification as _, PublishDiagnostics,
};
use lsp_types::{
    DidChangeTextDocumentParams, DidChangeWatchedFilesParams, DidCloseTextDocumentParams,
    DidOpenTextDocumentParams, PublishDiagnosticsParams,
};
use serde::de::DeserializeOwned;

use super::Server;
use crate::convert::{path_to_uri, source_kind, uri_to_path};
use crate::diagnostics;

pub(super) fn dispatch(server: &mut Server, notification: Notification) {
    match notification.method.as_str() {
        DidOpenTextDocument::METHOD => {
            if let Some(params) = parse::<DidOpenTextDocumentParams>(&notification) {
                did_open(server, params);
            }
        }
        DidChangeTextDocument::METHOD => {
            if let Some(params) = parse::<DidChangeTextDocumentParams>(&notification) {
                did_change(server, params);
            }
        }
        DidCloseTextDocument::METHOD => {
            if let Some(params) = parse::<DidCloseTextDocumentParams>(&notification) {
                did_close(server, params);
            }
        }
        DidChangeWatchedFiles::METHOD => {
            if let Some(params) = parse::<DidChangeWatchedFilesParams>(&notification) {
                did_change_watched_files(server, params);
            }
        }
        // `didSave` needs nothing: the text is already synced. Others are not supported.
        _ => {}
    }
}

/// The notification's params; malformed ones are reported on stderr and skipped.
fn parse<P: DeserializeOwned>(notification: &Notification) -> Option<P> {
    match serde_json::from_value(notification.params.clone()) {
        Ok(params) => Some(params),
        Err(error) => {
            eprintln!(
                "kagari-lsp: invalid `{}` params: {error}",
                notification.method
            );
            None
        }
    }
}

/// The canonical path of a `.kis`/`.kix` document; other documents are ignored.
fn source_path(uri: &lsp_types::Uri) -> Option<PathBuf> {
    let path = uri_to_path(uri)?;
    source_kind(&path)?;
    Some(path)
}

fn did_open(server: &mut Server, params: DidOpenTextDocumentParams) {
    let Some(path) = source_path(&params.text_document.uri) else {
        return;
    };
    server.find_project(&path);
    set_open_text(server, path, params.text_document.text);
}

fn did_change(server: &mut Server, params: DidChangeTextDocumentParams) {
    let Some(path) = source_path(&params.text_document.uri) else {
        return;
    };
    let Some(change) = params.content_changes.into_iter().next_back() else {
        return;
    };
    set_open_text(server, path, change.text);
}

fn set_open_text(server: &mut Server, path: PathBuf, text: String) {
    server
        .db
        .set_file_text(FilePath::disk(&path), Some(text.clone()));
    server.open.insert(path, text);
    diagnostics::refresh(server);
}

fn did_close(server: &mut Server, params: DidCloseTextDocumentParams) {
    let Some(path) = source_path(&params.text_document.uri) else {
        return;
    };
    server.open.remove(&path);
    let file = FilePath::disk(&path);
    server
        .db
        .set_file_text(file.clone(), std::fs::read_to_string(&path).ok());
    let in_project = server.projects.iter().any(|loaded| {
        def_maps(&server.db, loaded.program)
            .files
            .iter()
            .any(|(source, _)| *source.path(&server.db) == file)
    });
    if !in_project && let Some(uri) = path_to_uri(&path) {
        let params = PublishDiagnosticsParams {
            uri,
            diagnostics: Vec::new(),
            version: None,
        };
        let notification =
            lsp_server::Notification::new(PublishDiagnostics::METHOD.to_owned(), params);
        // A closed connection ends the main loop on its own.
        let _ = server.sender.send(notification.into());
    }
    diagnostics::refresh(server);
}

fn did_change_watched_files(server: &mut Server, params: DidChangeWatchedFilesParams) {
    let mut reload = false;
    for event in params.changes {
        let Some(path) = uri_to_path(&event.uri) else {
            continue;
        };
        if path.file_name().is_some_and(|name| name == MANIFEST_FILE) {
            reload = true;
        } else if source_kind(&path).is_some() {
            if !server.open.contains_key(&path) {
                let text = std::fs::read_to_string(&path).ok();
                server.db.set_file_text(FilePath::disk(&path), text);
            }
        } else if server
            .projects
            .iter()
            .any(|loaded| path.starts_with(loaded.project.asset_dir()))
        {
            let exists = path.is_file();
            server.db.set_asset_exists(FilePath::disk(&path), exists);
        }
    }
    if reload {
        server.load_projects();
    }
    diagnostics::refresh(server);
}
