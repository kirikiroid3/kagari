//! The server state and its main loop.

mod notifications;
mod requests;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crossbeam_channel::Sender;
use kagari_lang::{Db, FilePath, OsFileSystem, RootDatabase};
use lsp_server::{Connection, Message, Notification, Request, RequestId};
use lsp_types::notification::{Notification as _, ShowMessage};
use lsp_types::request::{RegisterCapability, Request as _};
use lsp_types::{
    DidChangeWatchedFilesRegistrationOptions, FileSystemWatcher, GlobPattern,
    HoverProviderCapability, InitializeParams, MessageType, OneOf, Registration,
    RegistrationParams, ServerCapabilities, ShowMessageParams, TextDocumentSyncCapability,
    TextDocumentSyncKind, WatchKind, notification::DidChangeWatchedFiles,
};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::convert::uri_to_path;
use crate::diagnostics;
use crate::error::LspError;
use crate::projects::LoadedProject;

/// The id of the `client/registerCapability` request for the file watchers.
const WATCH_REGISTRATION: &str = "kagari-watch";

pub(crate) struct Server {
    pub sender: Sender<Message>,
    pub db: RootDatabase,
    pub projects: Vec<LoadedProject>,
    /// The text of every open document, by canonical path.
    pub open: FxHashMap<PathBuf, String>,
    /// The files that currently have diagnostics on the client.
    pub published: Arc<Mutex<FxHashSet<PathBuf>>>,
    /// The workspace folders; each one with a `kagari.toml` is a project.
    pub roots: Vec<PathBuf>,
    /// Project load errors already shown since the projects were last reloaded.
    pub shown_errors: FxHashSet<String>,
}

pub(crate) fn run(connection: Connection) -> Result<(), LspError> {
    let capabilities = ServerCapabilities {
        text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
        hover_provider: Some(HoverProviderCapability::Simple(true)),
        definition_provider: Some(OneOf::Left(true)),
        document_formatting_provider: Some(OneOf::Left(true)),
        document_symbol_provider: Some(OneOf::Left(true)),
        ..ServerCapabilities::default()
    };
    let params = connection.initialize(serde_json::to_value(capabilities)?)?;
    let params: InitializeParams = serde_json::from_value(params)?;

    let mut server = Server {
        sender: connection.sender.clone(),
        db: RootDatabase::new(Arc::new(OsFileSystem)),
        projects: Vec::new(),
        open: FxHashMap::default(),
        published: Arc::default(),
        roots: workspace_roots(&params),
        shown_errors: FxHashSet::default(),
    };
    if watched_files_registrable(&params) {
        server.register_watchers()?;
    }
    server.load_projects();
    diagnostics::refresh(&server);

    for message in &connection.receiver {
        match message {
            Message::Request(request) => {
                if connection.handle_shutdown(&request)? {
                    return Ok(());
                }
                let response = requests::dispatch(&server, request);
                server.sender.send(response.into())?;
            }
            Message::Notification(notification) => {
                notifications::dispatch(&mut server, notification)
            }
            Message::Response(_) => {}
        }
    }
    Ok(())
}

#[allow(deprecated)] // `root_uri` is the fallback for clients without workspace folders.
fn workspace_roots(params: &InitializeParams) -> Vec<PathBuf> {
    match &params.workspace_folders {
        Some(folders) if !folders.is_empty() => folders
            .iter()
            .filter_map(|folder| uri_to_path(&folder.uri))
            .collect(),
        _ => params.root_uri.iter().filter_map(uri_to_path).collect(),
    }
}

fn watched_files_registrable(params: &InitializeParams) -> bool {
    params
        .capabilities
        .workspace
        .as_ref()
        .and_then(|workspace| workspace.did_change_watched_files)
        .and_then(|watched| watched.dynamic_registration)
        == Some(true)
}

impl Server {
    /// Asks the client to report changes to sources, manifests and (created or deleted) assets.
    fn register_watchers(&self) -> Result<(), LspError> {
        let watcher = |glob: &str, kind: WatchKind| FileSystemWatcher {
            glob_pattern: GlobPattern::String(glob.to_owned()),
            kind: Some(kind),
        };
        let options = DidChangeWatchedFilesRegistrationOptions {
            watchers: vec![
                watcher("**/*.{kis,kix}", WatchKind::all()),
                watcher("**/kagari.toml", WatchKind::all()),
                watcher("**/*", WatchKind::Create | WatchKind::Delete),
            ],
        };
        let params = RegistrationParams {
            registrations: vec![Registration {
                id: WATCH_REGISTRATION.to_owned(),
                method: DidChangeWatchedFiles::METHOD.to_owned(),
                register_options: Some(serde_json::to_value(options)?),
            }],
        };
        let request = Request::new(
            RequestId::from(WATCH_REGISTRATION.to_owned()),
            RegisterCapability::METHOD.to_owned(),
            params,
        );
        self.sender.send(request.into())?;
        Ok(())
    }

    /// Shows an error message in the client.
    pub fn show_error(&self, message: String) {
        let params = ShowMessageParams {
            typ: MessageType::ERROR,
            message,
        };
        let notification = Notification::new(ShowMessage::METHOD.to_owned(), params);
        // A closed connection ends the main loop on its own.
        let _ = self.sender.send(notification.into());
    }

    /// The text of `path`. Open documents' text is synced into the database, so this is the
    /// editor's text for them and the disk's for the rest.
    pub fn text(&self, path: &Path) -> Option<Arc<str>> {
        self.db
            .source_file(FilePath::disk(path))
            .text(&self.db)
            .clone()
    }
}
