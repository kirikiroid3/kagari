//! Zed support for Kagari: launches `kagari lsp` for `.kis`/`.kix` files and registers the
//! `Kagari` debug adapter, which runs `kagari dap` (the Debug Adapter Protocol over
//! stdin/stdout) for the `launch` and `attach` configurations of `.zed/debug.json`.

use zed_extension_api::{
    self as zed, DebugAdapterBinary, DebugConfig, DebugRequest, DebugScenario, DebugTaskDefinition,
    LanguageServerId, Result, StartDebuggingRequestArguments,
    StartDebuggingRequestArgumentsRequest, serde_json, settings::LspSettings,
};

/// The debug adapter's name in `extension.toml` and in `debug.json`'s `adapter` field.
const ADAPTER: &str = "Kagari";

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

    /// `kagari dap` in the worktree root, speaking DAP over stdin/stdout; the scenario's
    /// configuration is passed through unchanged as the `launch`/`attach` arguments.
    fn get_dap_binary(
        &mut self,
        adapter_name: String,
        config: DebugTaskDefinition,
        user_provided_debug_adapter_path: Option<String>,
        worktree: &zed::Worktree,
    ) -> Result<DebugAdapterBinary, String> {
        check_adapter(&adapter_name)?;
        if config.tcp_connection.is_some() {
            return Err(
                "the Kagari debug adapter speaks DAP over stdin/stdout and takes no \
                        `tcp_connection`; to debug a running game, use an `attach` \
                        configuration with its `port`"
                    .into(),
            );
        }
        let value = serde_json::from_str(&config.config)
            .map_err(|error| format!("invalid Kagari debug configuration: {error}"))?;
        let request = self.dap_request_kind(adapter_name, value)?;
        let Some(command) = user_provided_debug_adapter_path.or_else(|| worktree.which("kagari"))
        else {
            return Err("`kagari` was not found on PATH; install it with \
                        `cargo install --path tooling/kagari-cli` or set the debug adapter's \
                        binary path"
                .into());
        };
        Ok(DebugAdapterBinary {
            command: Some(command),
            arguments: vec!["dap".into()],
            envs: worktree.shell_env(),
            cwd: Some(worktree.root_path()),
            connection: None,
            request_args: StartDebuggingRequestArguments {
                configuration: config.config,
                request,
            },
        })
    }

    /// The configuration's `request` field: `"launch"` or `"attach"`.
    fn dap_request_kind(
        &mut self,
        adapter_name: String,
        config: serde_json::Value,
    ) -> Result<StartDebuggingRequestArgumentsRequest, String> {
        check_adapter(&adapter_name)?;
        match config.get("request").and_then(serde_json::Value::as_str) {
            Some("launch") => Ok(StartDebuggingRequestArgumentsRequest::Launch),
            Some("attach") => Ok(StartDebuggingRequestArgumentsRequest::Attach),
            Some(other) => Err(format!(
                "Kagari debug configurations have `\"request\": \"launch\"` or `\"attach\"`, \
                 not `{other}`"
            )),
            None => Err(
                "a Kagari debug configuration needs `\"request\": \"launch\"` or \
                         `\"attach\"`"
                    .into(),
            ),
        }
    }

    /// The new-session form: a window-mode launch. Its program is the project's `kagari.toml`
    /// (any `.toml` path) or else the runtime to run, its arguments the runtime's extra
    /// arguments. A game is attached to by port, so a process to attach to is refused.
    fn dap_config_to_scenario(&mut self, config: DebugConfig) -> Result<DebugScenario, String> {
        check_adapter(&config.adapter)?;
        let launch = match config.request {
            DebugRequest::Launch(launch) => launch,
            DebugRequest::Attach(_) => {
                return Err(
                    "the Kagari debug adapter attaches to a game by its `--debug-port`, \
                            not by process; add an `attach` configuration with `port` to \
                            `.zed/debug.json`"
                        .into(),
                );
            }
        };
        if !launch.envs.is_empty() {
            return Err("Kagari launch configurations take no environment variables".into());
        }
        let mut scenario = serde_json::Map::new();
        scenario.insert("request".into(), "launch".into());
        scenario.insert("mode".into(), "window".into());
        if !launch.program.is_empty() {
            let key = if launch.program.ends_with(".toml") {
                "manifestPath"
            } else {
                "runtime"
            };
            scenario.insert(key.into(), launch.program.into());
        }
        if let Some(cwd) = launch.cwd {
            scenario.insert("cwd".into(), cwd.into());
        }
        if !launch.args.is_empty() {
            scenario.insert("runtimeArgs".into(), launch.args.into());
        }
        if let Some(stop_on_entry) = config.stop_on_entry {
            scenario.insert("stopOnEntry".into(), stop_on_entry.into());
        }
        Ok(DebugScenario {
            label: config.label,
            adapter: config.adapter,
            build: None,
            config: serde_json::Value::Object(scenario).to_string(),
            tcp_connection: None,
        })
    }
}

fn check_adapter(adapter_name: &str) -> Result<(), String> {
    if adapter_name == ADAPTER {
        Ok(())
    } else {
        Err(format!(
            "unknown debug adapter `{adapter_name}`; this extension provides `{ADAPTER}`"
        ))
    }
}

zed::register_extension!(KagariExtension);
