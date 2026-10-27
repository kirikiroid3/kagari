#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![allow(clippy::arithmetic_side_effects)]

//! Drives the real server loop over an in-memory connection.

use std::path::Path;
use std::str::FromStr;
use std::thread::JoinHandle;
use std::time::Duration;

use kagari_lsp::LspError;
use lsp_server::{Connection, Message, Notification, Request, RequestId};
use lsp_types::notification::{
    DidChangeTextDocument, DidOpenTextDocument, Exit, Initialized, Notification as _,
    PublishDiagnostics,
};
use lsp_types::request::{
    DocumentSymbolRequest, Formatting, GotoDefinition, HoverRequest, Initialize, Request as _,
    Shutdown,
};
use lsp_types::{
    ClientCapabilities, DidChangeTextDocumentParams, DidOpenTextDocumentParams,
    DocumentFormattingParams, DocumentSymbol, DocumentSymbolParams, FormattingOptions,
    GotoDefinitionParams, Hover, HoverContents, HoverParams, InitializeParams, InitializedParams,
    Location, NumberOrString, Position, PublishDiagnosticsParams, SymbolKind,
    TextDocumentContentChangeEvent, TextDocumentIdentifier, TextDocumentItem,
    TextDocumentPositionParams, TextEdit, Uri, VersionedTextDocumentIdentifier, WorkspaceFolder,
};
use serde_json::Value;

const MANIFEST: &str = r#"[project]
name = "game"
version = "0.1.0"
entry = "src/main.kix"

[toolchain]
kagari = "*"
"#;

const HELPER: &str = "/// Adds.\nfn helper() -> u32 {\n    1\n}\n";
const HELPER_TYPE_ERROR: &str =
    "/// Adds.\nfn helper() -> u32 {\n    let x: u32 = true;\n    x\n}\n";
const HELPER_MISFORMATTED: &str = "/// Adds.\nfn  helper()->u32{1}\n";

/// `src/main.kix` with `helper` written as `helper_src`.
fn main_kix(helper_src: &str) -> String {
    format!(
        r#"use std::narrative::text::text_style;
use std::screen::size;
use std::system::default_bindings;

#[scene]
#[entry]
pushdown fn Start() {{
    s!(t!["It begins."]);
}}

static TEXT_SPEED: Setting<Duration> = setting(ms(30));
static AUTO_DELAY: Setting<Duration> = setting(ms(1300));
static AUTO_DELAY_PER_CHAR: Setting<Duration> = setting(ms(60));
static SKIP_UNREAD: Setting<bool> = setting(false);
static MASTER: Setting<Volume> = setting(volume(100));
static MUSIC: Setting<Volume> = setting(volume(60));
static SOUND: Setting<Volume> = setting(volume(100));
static VOICE: Setting<Volume> = setting(volume(100));
static VOICE_SUSTAIN: Setting<bool> = setting(false);
static WAIT_FOR_VOICE: Setting<bool> = setting(true);
static WINDOW_OPACITY: Setting<Int<0, 100>> = setting(80);
static FULLSCREEN: Setting<bool> = setting(false);
static ANIMATIONS: Setting<bool> = setting(true);

fn main() -> System {{
    System {{
        title: "Story",
        resolution: size(1280, 720),
        start: Title,
        game: GameView,
        save_slots: 20,
        auto_slots: helper(),
        backlog: 200,
        text: text_style(26, WHITE),
        settings: Settings {{
            text_speed: TEXT_SPEED,
            auto_delay: AUTO_DELAY,
            auto_delay_per_char: AUTO_DELAY_PER_CHAR,
            skip_unread: SKIP_UNREAD,
            master: MASTER,
            music: MUSIC,
            sound: SOUND,
            voice: VOICE,
            voice_sustain: VOICE_SUSTAIN,
            wait_for_voice: WAIT_FOR_VOICE,
            window_opacity: WINDOW_OPACITY,
            fullscreen: FULLSCREEN,
            animations: ANIMATIONS,
        }},
        bindings: default_bindings(),
    }}
}}

fn Title() -> RootPage {{
    RootPage(column![
        button!{{on_click = || nav::start(Start, (), |_end| ()), t!["Start"]}},
    ])
}}

fn GameView(game: Narrative) -> Element {{
    game_screen!(game = game)
}}

{helper_src}"#
    )
}

/// The position of the first occurrence of `needle` in `text` (ASCII text).
fn position_of(text: &str, needle: &str) -> Position {
    let at = text.find(needle).unwrap();
    let before = &text[..at];
    let line = before.matches('\n').count();
    let column = at - before.rfind('\n').map_or(0, |newline| newline + 1);
    Position {
        line: line as u32,
        character: column as u32,
    }
}

struct Client {
    connection: Connection,
    server: JoinHandle<Result<(), LspError>>,
    next_id: i32,
    main_uri: Uri,
    _dir: tempfile::TempDir,
}

impl Client {
    fn start() -> Client {
        let dir = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(dir.path()).unwrap();
        std::fs::write(root.join("kagari.toml"), MANIFEST).unwrap();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::create_dir_all(root.join("assets")).unwrap();
        let main_path = root.join("src/main.kix");
        std::fs::write(&main_path, main_kix(HELPER)).unwrap();

        let (server_connection, connection) = Connection::memory();
        let server = std::thread::spawn(move || kagari_lsp::run(server_connection));
        let mut client = Client {
            connection,
            server,
            next_id: 0,
            main_uri: uri(&main_path),
            _dir: dir,
        };
        let params = InitializeParams {
            capabilities: ClientCapabilities::default(),
            workspace_folders: Some(vec![WorkspaceFolder {
                uri: uri(&root),
                name: "game".into(),
            }]),
            ..InitializeParams::default()
        };
        client.request(Initialize::METHOD, params);
        client.notify(Initialized::METHOD, InitializedParams {});
        client
    }

    fn notify(&self, method: &str, params: impl serde::Serialize) {
        let notification = Notification::new(method.to_owned(), params);
        self.connection.sender.send(notification.into()).unwrap();
    }

    /// Sends a request and returns its result, skipping every other message.
    fn request(&mut self, method: &str, params: impl serde::Serialize) -> Value {
        self.next_id += 1;
        let id = RequestId::from(self.next_id);
        let request = Request::new(id.clone(), method.to_owned(), params);
        self.connection.sender.send(request.into()).unwrap();
        loop {
            if let Message::Response(response) = self.recv()
                && response.id == id
            {
                match response.response_result {
                    Ok(result) => return result,
                    Err(error) => panic!("`{method}` failed: {error:?}"),
                }
            }
        }
    }

    fn recv(&self) -> Message {
        self.connection
            .receiver
            .recv_timeout(Duration::from_secs(60))
            .unwrap()
    }

    /// Waits for diagnostics of `main.kix` that satisfy `accept`, skipping other messages.
    fn diagnostics(
        &self,
        accept: impl Fn(&PublishDiagnosticsParams) -> bool,
    ) -> PublishDiagnosticsParams {
        loop {
            if let Message::Notification(notification) = self.recv()
                && notification.method == PublishDiagnostics::METHOD
            {
                let params: PublishDiagnosticsParams =
                    serde_json::from_value(notification.params).unwrap();
                if params.uri == self.main_uri && accept(&params) {
                    return params;
                }
            }
        }
    }

    fn position_params(&self, text: &str, needle: &str) -> TextDocumentPositionParams {
        TextDocumentPositionParams {
            text_document: TextDocumentIdentifier {
                uri: self.main_uri.clone(),
            },
            position: position_of(text, needle),
        }
    }

    fn change(&self, version: i32, text: String) {
        self.notify(
            DidChangeTextDocument::METHOD,
            DidChangeTextDocumentParams {
                text_document: VersionedTextDocumentIdentifier {
                    uri: self.main_uri.clone(),
                    version,
                },
                content_changes: vec![TextDocumentContentChangeEvent {
                    range: None,
                    range_length: None,
                    text,
                }],
            },
        );
    }
}

fn uri(path: &Path) -> Uri {
    Uri::from_str(url::Url::from_file_path(path).unwrap().as_str()).unwrap()
}

#[test]
fn server_end_to_end() {
    let mut client = Client::start();

    // 1. Opening a file with a type error publishes it.
    client.notify(
        DidOpenTextDocument::METHOD,
        DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: client.main_uri.clone(),
                language_id: "kagari".into(),
                version: 1,
                text: main_kix(HELPER_TYPE_ERROR),
            },
        },
    );
    let published = client.diagnostics(|params| !params.diagnostics.is_empty());
    assert!(
        published.diagnostics.iter().any(|diagnostic| matches!(
            &diagnostic.code,
            Some(NumberOrString::String(code)) if code.starts_with("E2")
        )),
        "{:#?}",
        published.diagnostics
    );

    // 2. Fixing it clears the diagnostics.
    let text = main_kix(HELPER);
    client.change(2, text.clone());
    client.diagnostics(|params| params.diagnostics.is_empty());

    // 3. Hover on a call of `helper` shows its header and doc comment.
    let call = client.position_params(&text, "helper()");
    let hover: Hover = serde_json::from_value(client.request(
        HoverRequest::METHOD,
        HoverParams {
            text_document_position_params: call.clone(),
            work_done_progress_params: Default::default(),
        },
    ))
    .unwrap();
    let HoverContents::Markup(markup) = hover.contents else {
        panic!("{:?}", hover.contents)
    };
    assert!(
        markup.value.contains("fn helper() -> u32"),
        "{}",
        markup.value
    );
    assert!(markup.value.contains("Adds."), "{}", markup.value);

    // 4. Go to definition on the call jumps to `helper`'s name.
    let location: Location = serde_json::from_value(client.request(
        GotoDefinition::METHOD,
        GotoDefinitionParams {
            text_document_position_params: call,
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        },
    ))
    .unwrap();
    assert_eq!(location.uri, client.main_uri);
    let name = position_of(&text, "helper() -> u32");
    assert_eq!(location.range.start, name);
    assert_eq!(
        location.range.end,
        Position {
            character: name.character + 6,
            ..name
        }
    );

    // 5. The document symbols list the functions.
    let symbols: Vec<DocumentSymbol> = serde_json::from_value(client.request(
        DocumentSymbolRequest::METHOD,
        DocumentSymbolParams {
            text_document: TextDocumentIdentifier {
                uri: client.main_uri.clone(),
            },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        },
    ))
    .unwrap();
    for name in ["main", "helper"] {
        assert!(
            symbols
                .iter()
                .any(|symbol| symbol.name == name && symbol.kind == SymbolKind::FUNCTION),
            "{name} missing in {symbols:#?}"
        );
    }

    // 6. Formatting rewrites misformatted code.
    client.change(3, main_kix(HELPER_MISFORMATTED));
    let edits: Vec<TextEdit> = serde_json::from_value(client.request(
        Formatting::METHOD,
        DocumentFormattingParams {
            text_document: TextDocumentIdentifier {
                uri: client.main_uri.clone(),
            },
            options: FormattingOptions {
                tab_size: 4,
                insert_spaces: true,
                ..Default::default()
            },
            work_done_progress_params: Default::default(),
        },
    ))
    .unwrap();
    assert_eq!(edits.len(), 1, "{edits:#?}");
    assert!(
        edits[0].new_text.contains("fn helper() -> u32 {\n    1\n}"),
        "{}",
        edits[0].new_text
    );

    // 7. `shutdown` and `exit` end the server.
    client.request(Shutdown::METHOD, ());
    client.notify(Exit::METHOD, ());
    let Client {
        server, connection, ..
    } = client;
    drop(connection);
    server.join().unwrap().unwrap();
}
