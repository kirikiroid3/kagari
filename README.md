# Kagari Visual Novel Engine

## Progress

Compiler stages follow [Compilation](docs/syntax/src/compilation.md#pipeline).

| Stage                    | Status         | Notes                                                                                          |
| ------------------------ | -------------- | ---------------------------------------------------------------------------------------------- |
| 1. Parse + desugar forms | ✅ Done        | Lossless CST, typed AST; `s!`/`choice!`/`t!` and the UI forms desugared.                       |
| 2. Resolve               | ✅ Done        | Module tree, imports, impls, paths, locals, labels, asset paths.                               |
| 3. Type and kind check   | ✅ Done        | Item signatures and type inference of every body (`Int` ranges, coercions, generics and std trait bounds, `nav::start`, `asset`); `match` exhaustiveness; kind and story rules (call rules, scenes and instant procedures, story variables and handles, finite types, second-class references and closures, mutability, `choice!`/`t!` rules, `#[must_use]`). |
| 4. Const evaluation      | ✅ Done        | Constants, statics, folding and tabulation of story bodies, every const-eval error, keyframe detection (`src/eval`). Evaluating renders on the screens that reach their steps is part of stage 5. |
| 5. Analysis              | ✅ Done        | Termination; the reachable states of the story from every entry scene and argument, summarized per scene (`src/analysis/story`): stuck choices, errors in story code, unreachable scenes and steps, renders on the screens that reach their steps, bounded screens, consistency; story variables that never influence the story; mode-graph checks of the system. |
| 6. Emit bytecode         | ✅ Done        | `kagari-bytecode`: format, encoder, decoder, verifier, disassembler; emitter in `src/emit`.     |

| Component     | Status                          | Notes                                                                                                                                                   |
| ------------- | ------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `kagari-std`  | 🚧 Designed; declarations only  | `time`, `screen` (+`anim`, `audio`), `stage`, `narrative` (+`text`), `system` (+`nav`, `saves`), `ui` (+`style`), `prelude`; see [`docs/std`](docs/std) and the diagnostic codes in [`docs/diagnostics`](docs/diagnostics). |
| `kagari-archive` | ✅ Done                      | `.kiarchive` asset packages (Store/LZ4 blocks, solid blocks, streamable front index, CRC32, numbered patch chains) and `.kisave` paged save databases (in-place edits, `compact()`). |
| `kagari` CLI  | ✅ Done                         | `cargo run -p kagari-cli -- check\|build\|disasm …`                                                                                                     |
| `kagari-lsp`  | ❌ Not started                  | Stub only.                                                                                                                                              |
| Runtime / VM  | ❌ Not started                  |                                                                                                                                                         |
