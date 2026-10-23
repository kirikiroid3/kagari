# Kagari Visual Novel Engine

## Progress

Compiler stages follow [Compilation](docs/syntax/src/compilation.md#pipeline).

| Stage                    | Status         | Notes                                                                                          |
| ------------------------ | -------------- | ---------------------------------------------------------------------------------------------- |
| 1. Parse + desugar forms | ✅ Done        | Lossless CST, typed AST; `s!`/`choice!`/`t!` desugared. UI forms designed (std book); lowering not implemented. |
| 2. Resolve               | ✅ Done        | Module tree, imports, impls, paths, locals, labels, asset paths.                               |
| 3. Type and kind check   | ❌ Not started | Function kinds are recorded in HIR but not checked.                                            |
| 4. Const evaluation      | ❌ Not started |                                                                                                |
| 5. Analysis              | ❌ Not started | Termination, stuck choices, consistency, bounded screens, system checks.                       |
| 6. Emit bytecode         | ❌ Not started | `kagari-bytecode` is empty.                                                                    |

| Component     | Status                          | Notes                                                                                                                                                   |
| ------------- | ------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `kagari-std`  | 🚧 Designed; declarations only  | `time`, `screen` (+`anim`, `audio`), `stage`, `narrative` (+`text`), `system` (+`nav`, `saves`), `ui` (+`style`), `prelude`; see [`docs/std`](docs/std). |
| `check` CLI   | ✅ Done                         | `cargo run -p kagari-lang --example check -- [--resolutions] <root.kix>`                                                                                |
| `kagari-lsp`  | ❌ Not started                  | Stub only.                                                                                                                                              |
| Runtime / VM  | ❌ Not started                  |                                                                                                                                                         |
