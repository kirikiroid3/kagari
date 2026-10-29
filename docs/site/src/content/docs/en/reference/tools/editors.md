---
title: "Editor Support"
---

Kagari ships three pieces for editors:

| Piece | What it gives | Where |
| ----- | ------------- | ----- |
| Language server | Diagnostics, formatting, go to definition, hover, document outline | `kagari lsp` |
| Tree-sitter grammar | Syntax highlighting and structure for `.kis` and `.kix` | `tooling/tree-sitter-kagari` |
| Zed extension | Both of the above, ready to install in Zed | `tooling/zed-kagari` |

All of them need the `kagari` executable; see [Installing](./cli.md#installing).

## The language server

`kagari lsp` runs a language server that speaks the Language Server Protocol over stdin and
stdout. Configure your editor to start the command `kagari` with the argument `lsp` for files
ending in `.kis` and `.kix`.

### Features

| Feature | LSP request | Notes |
| ------- | ----------- | ----- |
| Diagnostics | `textDocument/publishDiagnostics` | The same diagnostics as `kagari check`, for every file of the project, including files that are not open. Recomputed in the background after each edit. |
| Formatting | `textDocument/formatting` | The same output as `kagari fmt`. A file with syntax errors is left unchanged. |
| Go to definition | `textDocument/definition` | Works on paths, local bindings and method calls. Jumps into std declarations too (see below). Field accesses and `use` paths are not navigable. |
| Hover | `textDocument/hover` | The header (signature) or type of what is under the cursor, followed by its `///` doc comment. |
| Document symbols | `textDocument/documentSymbol` | The items of the file, nested as written, with enum variants. Editors show it as the outline. |

Other requests are not supported. The server syncs whole documents: it sees your unsaved text.

### Projects in the language server

The server loads a project for each workspace folder that has a `kagari.toml` at its root, and
for each opened file, the project found by searching its directory and the parent directories
for `kagari.toml`. A file is checked as part of the project with the deepest root that contains
it. The manifest's [compiler options](./projects.md#compiler-options) apply, as in
`kagari check`.

A file that belongs to no project gets syntax diagnostics only. When a `kagari.toml` is invalid
or requires another toolchain version, the server shows the error as a message in the editor.

When the editor supports file watching, the server watches `.kis`/`.kix` files, `kagari.toml`
files, and created or deleted files in the asset directories, so adding or removing an asset file
updates the diagnostics. A changed `kagari.toml` reloads the projects.

The std sources live inside the compiler. To open them for go to definition, the server writes
copies to `kagari-std-<version>` in the system's temporary directory. Edits to these copies have
no effect.

## The formatter

`kagari fmt` and the language server's formatting share one formatter. It is in the style of
rustfmt and has no settings:

- Lines are reflowed to fit in **100 columns** where possible. Widths are display widths, so a
  full-width character counts as two columns.
- Indentation is **4 spaces** per level.
- A group (an argument list, a form, a chain, a block) stays on one line when it fits, and
  otherwise breaks with one element per line. The formatter adds or removes list commas
  accordingly.
- Every comment is kept, attached to the code it belongs to: a comment after code on the same
  line stays at the end of that line; other comments stay before the next token.
- Runs of blank lines collapse to one blank line. Lines never end in whitespace. The file ends
  with exactly one newline (an empty file stays empty).
- Forms (`s!`, `choice!`, `t![…]`, the UI forms) are formatted too.

A file with syntax errors is never formatted. After formatting, the formatter re-parses its
output and checks that it has the same tokens and comments as the input; if not, it refuses with
an error (a formatter bug) and the file is unchanged.

For the command line flags see [`kagari fmt`](./cli.md#kagari-fmt).

## Zed

The Zed extension in `tooling/zed-kagari` provides the `Kagari` language for `.kis` and `.kix`
files: highlighting, outline, indentation and bracket rules from the tree-sitter grammar, and the
language server.

To install it:

1. Install `kagari` so that it is on your `PATH` (`cargo install --path tooling/kagari-cli`).
2. In Zed, run the command **zed: install dev extension** and choose the `tooling/zed-kagari`
   directory of the Kagari repository.

The extension starts `kagari lsp`. To use another executable or arguments, set them in Zed's
`settings.json`:

```json
{
  "lsp": {
    "kagari": {
      "binary": {
        "path": "/path/to/kagari",
        "arguments": ["lsp"]
      }
    }
  }
}
```

If `kagari` is neither configured nor on the `PATH`, Zed reports
`` `kagari` was not found on PATH ``.

## Other editors

Any editor with an LSP client can use the language server: register the command `kagari lsp`
for the file extensions `kis` and `kix`.

For syntax highlighting, use the tree-sitter grammar in `tooling/tree-sitter-kagari`
(`grammar.js`, with the generated parser in `src/`). Its scope is `source.kagari` and it covers
both `.kis` and `.kix` files. Highlight, outline, indent and bracket queries written for the
grammar are in `tooling/zed-kagari/languages/kagari/` (`highlights.scm`, `outline.scm`,
`indents.scm`, `brackets.scm`); editors such as Neovim or Helix can reuse them, adjusting capture
names where the editor's conventions differ.
