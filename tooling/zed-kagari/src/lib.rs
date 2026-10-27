//! Zed support for Kagari: launches `kagari lsp` for `.kis`/`.kix` files.

use zed_extension_api::{self as zed, LanguageServerId, Result, settings::LspSettings};

struct KagariExtension;

impl zed::Extension for KagariExtension {
    fn new() -> Self {
        KagariExtension
    }

    fn language_server_command(
        &mut self,
        _language_server_id: &LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<zed::Command> {
        let binary = LspSettings::for_worktree("kagari", worktree)
            .ok()
            .and_then(|settings| settings.binary);
        let configured = binary.as_ref().and_then(|binary| binary.path.clone());
        let Some(command) = configured.or_else(|| worktree.which("kagari")) else {
            return Err("`kagari` was not found on PATH; install it with \
                        `cargo install --path tooling/kagari-cli` or set `lsp.kagari.binary.path`"
                .into());
        };
        let args = binary
            .and_then(|binary| binary.arguments)
            .unwrap_or_else(|| vec!["lsp".into()]);
        Ok(zed::Command {
            command,
            args,
            env: worktree.shell_env(),
        })
    }
}

zed::register_extension!(KagariExtension);
