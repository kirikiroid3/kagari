# Kagari Visual Novel Engine

## Progress

Compiler stages follow [Compilation](docs/syntax/src/compilation.md#pipeline).

| Stage                    | Status         | Notes                                                                                          |
| ------------------------ | -------------- | ---------------------------------------------------------------------------------------------- |
| 1. Parse + desugar forms | ✅ Done        | Lossless CST, typed AST; `s!`/`choice!`/`t!` and the UI forms desugared.                       |
| 2. Resolve               | ✅ Done        | Module tree, imports, impls, paths, locals, labels, asset paths.                               |
| 3. Type and kind check   | ✅ Done        | Item signatures and type inference of every body (`Int` ranges, coercions, generics and std trait bounds, `nav::start`, `asset`); `match` exhaustiveness; kind and story rules (call rules, scenes and instant procedures, story variables and handles, finite types, second-class references and closures, mutability, `choice!`/`t!` rules, `#[must_use]`). |
| 4. Const evaluation      | ✅ Done        | Constants, statics, folding and tabulation of story bodies, every const-eval error, keyframe detection (`src/eval`). Evaluating renders on the screens that reach their steps is part of stage 5. |
| 5. Analysis              | ❌ Not started | Termination, stuck choices, consistency, bounded screens, system checks.                       |
| 6. Emit bytecode         | ❌ Not started | `kagari-bytecode` is empty.                                                                    |

| Component     | Status                          | Notes                                                                                                                                                   |
| ------------- | ------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `kagari-std`  | 🚧 Designed; declarations only  | `time`, `screen` (+`anim`, `audio`), `stage`, `narrative` (+`text`), `system` (+`nav`, `saves`), `ui` (+`style`), `prelude`; see [`docs/std`](docs/std). |
| `check` CLI   | ✅ Done                         | `cargo run -p kagari-lang --example check -- [--resolutions] <root.kix>`                                                                                |
| `kagari-lsp`  | ❌ Not started                  | Stub only.                                                                                                                                              |
| Runtime / VM  | ❌ Not started                  |                                                                                                                                                         |
