# Formatter

`kagari-fmt` is a rustfmt-style formatter for `.kis` and `.kix` files. It is width-based: it
reflows code to fit in `MAX_WIDTH = 100` columns with `INDENT = 4` spaces per level. Nothing is
configurable, on purpose: one canonical layout, no style debates, no config file to discover. It
backs `kagari fmt` and the language server's `textDocument/formatting`.

## Where in the code

| Path | Contents |
| ---- | -------- |
| `tooling/kagari-fmt/src/lib.rs` | `format(text, kind) -> Result<String, FormatError>`, `MAX_WIDTH`, `INDENT`, `FormatError` |
| `tooling/kagari-fmt/src/trivia.rs` | Comment ownership: leading/trailing comments per significant token, blank-line facts |
| `tooling/kagari-fmt/src/rules/mod.rs` | `Formatter`: token emission (`tok`, `tok_bare`, `hoisted`, `dangling`), spacing helpers, `verbatim`, the dispatch over every `SyntaxKind` |
| `tooling/kagari-fmt/src/rules/items.rs` | Source file, attributes, `fn` headers, ADTs, declarations, `use`, `impl`, `trait` |
| `tooling/kagari-fmt/src/rules/lists.rs` | Delimited, vertical and hugging lists; the assignment layout |
| `tooling/kagari-fmt/src/rules/blocks.rs` | Blocks, `if` chains, `match` arms, loops |
| `tooling/kagari-fmt/src/rules/chains.rs` | Postfix chains (`a.b().c?`) and binary operators |
| `tooling/kagari-fmt/src/rules/forms.rs` | Forms (`s!(…)`, `column![…]`, `choice!{…}`) and text (`t![…]`) |
| `tooling/kagari-fmt/src/rules/{exprs,patterns,types}.rs` | Remaining expressions, patterns, function types |
| `tooling/kagari-fmt/src/doc/mod.rs` | The `Doc` layout IR, break propagation, `flatten` |
| `tooling/kagari-fmt/src/doc/print.rs` | The Wadler/Prettier-style printer |
| `tooling/kagari-fmt/src/verify.rs` | Post-format safety check |
| `tooling/kagari-fmt/src/doc/tests.rs` | Printer unit tests |
| `tooling/kagari-fmt/tests/format.rs` | Fixture snapshots, idempotence, corpus stability |
| `tooling/kagari-fmt/tests/fixtures/`, `tests/snapshots/` | Inputs and `insta` snapshots |

The crate depends only on `kagari-syntax`, `unicode-width` and `rustc-hash`. It never touches the
compiler database: formatting is purely syntactic.

## Pipeline

<!-- diagram: fmt-pipeline (mermaid-ish)
flowchart LR
  text["text (.kis/.kix)"] ==> parse["parse (kagari_syntax)"]
  parse ==> trivia["Trivia::new: comment ownership"]
  trivia ==> rules["rules: CST node to Doc"]
  rules ==> print["print: width 100, indent 4"]
  print ==> verify["verify: reparse, compare"]
  parse -.->|parse errors| syn["Err(Syntax(errors))"]
  verify -.->|mismatch| internal["Err(Internal(...))"]
  verify ==> ok["Ok(String)"]
-->
{{#include ../diagrams/fmt-pipeline.svg}}

```rust
pub fn format(text: &str, kind: SourceKind) -> Result<String, FormatError> {
    let parsed = kagari_syntax::parse(text, kind);
    if !parsed.errors().is_empty() {
        return Err(FormatError::Syntax(parsed.errors().to_vec()));
    }
    let mut doc = rules::format_file(&parsed.syntax_node());
    let output = doc::print::print(&mut doc);
    verify::check(&parsed, &output, kind)?;
    Ok(output)
}
```

1. **Parse.** The lossless CST from `kagari-syntax` (see
   [Syntax: Lexer, CST, AST and Forms](../compiler/syntax.md)). A file with any syntax error is
   refused: guessing a layout around an `ERROR` node risks changing the program.
2. **Trivia.** `Trivia::new` assigns every comment to a significant token (below).
3. **Rules.** `rules::format_file` maps every CST node kind to a `Doc`.
4. **Print.** `doc::print::print` propagates forced breaks and lays the `Doc` out.
5. **Verify.** `verify::check` reparses the output and compares it with the input.

`kind` matters because `.kis` and `.kix` lex differently (forms are only in `.kix`).

## The `Doc` model

`Doc` (`doc/mod.rs`) is a Wadler/Prettier document:

| Variant | Flat | Broken |
| ------- | ---- | ------ |
| `Text(s)` | `s` | `s` |
| `Line` | space | newline + indent |
| `SoftLine` | nothing | newline + indent |
| `HardLine` | newline | newline |
| `BlankLine` | exactly one empty line before the next text (nothing at file start) | same |
| `Concat(docs)` | sequence | sequence |
| `Indent(doc)` | `doc` with indent + 4 | same |
| `Group { doc, broken }` | printer chooses flat if it fits and `broken` is false | — |
| `ConditionalGroup(states)` | alternative layouts of the same content, tried in order | — |
| `IfBreak { broken, flat }` | `flat` | `broken` (chosen by the enclosing group's mode) |

Helpers: `text`, `concat`, `group`, `broken_group`, `indent`, `if_break`, `conditional_group`.
Rule docs in source comments use a shorthand: `G` group, `I` indent, `L` line, `S` soft line,
`H` hard line, `B` blank line.

### Break propagation

Before printing, `propagate_breaks` marks a `Group` broken when its own content has a forced
break (`HardLine`, `BlankLine`, or a multi-line `Text`) **followed by more content**. A forced break
at the very end of a group, which is the newline after a trailing `// comment`, does not break
that group; the enclosing groups see it. So a trailing line comment does not reflow the
expression before it. `ConditionalGroup` alternatives are propagated independently, and a
`ConditionalGroup` counts as plain content for its parent, since one of its states may avoid
the break.

`breaks_inside(doc)` and `has_forced_break(doc)` answer the same question for rules that must
pick a layout up front (the hug layout uses them).

### The printer

`doc/print.rs` is a stack machine over `(indent, mode, &Doc)` commands:

- A `Group` prints flat iff it is not `broken` and `fits` in the remaining width; otherwise in
  break mode. `fits` measures the group flat, followed by the rest of the current line from the
  stack (each command in its own mode), up to the first newline.
- A `ConditionalGroup` takes its first state flat if it has no inner forced break and fits; else
  the first later state whose first line fits in break mode; else the last state, broken.
- Newlines are idempotent at line start: a newline requested on an empty line only sets the next
  line's indentation. Only `BlankLine` makes an empty line, and at most one. This is what makes
  blank lines canonical (any number in the source → zero or one).
- Indentation is written lazily before a line's first text; trailing spaces are trimmed before
  every newline, so lines never end in whitespace.
- Widths are `unicode-width` display widths, so CJK text in `t![…]` counts as two columns per
  character.
- The output ends with exactly one `\n`, or is empty if nothing was printed (an empty or
  whitespace-only file formats to `""`).

## Comments

The parser attaches trivia in front of the next significant token, which says nothing about
which construct a comment belongs to. `trivia.rs` therefore decides ownership per token,
independent of tree shape:

- a token's **trailing** comments are those after it, before the first newline;
- its **leading** comments are the other comments in the trivia run before it;
- trivia after the last token belongs to a virtual end-of-file token.

Each `Comment` records `is_line`, `newline_before`, `blank_line_before` and `newline_after`; each
`TokenTrivia` records `blank_before` (an empty line before the token or its first leading
comment) and `blank_after_leading`.

Every significant token is emitted exactly once, through `Formatter::tok`, which emits the
token's leading comments, its text and its trailing comments. Variants exist for the places where
that is wrong:

| Helper | Use |
| ------ | --- |
| `hoisted(element)` | Emits the leading comments of the element's first token **before** the element, outside its groups, so a comment line above `a + b` does not force the `+` to break. Records the token so `tok` does not emit them again. |
| `tok_bare(close)` + `dangling(close, …)` | Closing delimiters: their leading comments are emitted inside the list's indentation, as the last "element". |
| `comments_only(comma)` | Commas the list layout drops: the text goes, the comments stay. |
| `eof_comments()` | Comments after the last item. |
| `verbatim(node)` | Token-by-token copy with a space wherever the source had trivia, never just inside delimiters. Used for attribute token trees, `ERROR` nodes and unknown kinds. |

A line comment always ends with a `HardLine`. A block comment on its own line keeps its own
line; an inline block comment stays inline. Hugging (below) is disabled for any list with
comments at its delimiters, commas or element edges, because the hug state would have to drop
them.

## Layout rules

`Formatter::node` dispatches on every `SyntaxKind` that can be a node; the match is the
inventory of supported syntax. The main layouts:

| Construct | Layout |
| --------- | ------ |
| Source file | Items separated by `H`, plus `B` where the source had a blank line; then end-of-file comments |
| Attribute | `#[path(tokens)]` then `H`; token trees are verbatim |
| `fn` header | `G([vis][const \|pushdown ]fn name[generics](params)[ -> ret])`, then the body |
| Delimited list | `G(open, I(S\|L, e1, ",", L, e2, …, trailing ","), S\|L, close)`; trailing comma only when broken (`Trailing::IfBreak`); `(a,)` always keeps its comma; `{ … }` record lists get inner spaces |
| Vertical list | `open, I(H, e1[","], H, e2[","], …), H, close`, a blank line kept before an element that had one. Used for `enum` variants, item lists, `impl`/`trait` bodies, `match` arms, struct fields, and `{}` forms with arms |
| Hug list | Argument lists and `()`/`[]` forms whose last element is huggable (block, record, array, closure with block body, `[]`/`{}` form): `ConditionalGroup([all flat, open + flat prefix + G(last) + close, fully broken])` |
| Block | Vertical when it is a `fn` body, or has a statement, item or comment; otherwise `G({ I(L tail) L })`; empty is `{}` |
| `if` chain | One group for the whole `else if` chain, so every block is inline or none is |
| `match` arm | `pat[ if guard] =>` + body in the assignment layout; a block body gets no comma, any other body gets one |
| Assignment layout (`let`, `const`, `static`, `type`, `=`/`op=`, record fields, form attributes) | `op rhs` on one line for block-like and call-like right-hand sides (calls, method calls, forms, `t![]`, records, arrays, tuples, closures, blocks, `if`, `match`, loops, parens); otherwise `G(op, I(L, rhs))` |
| Postfix chain | With two or more method calls: `G(root, I(S .link1, S .link2, …))`, `?` and `[i]` stay on their link, field accesses right after the root stay on its line; otherwise links concatenate and only argument lists break |
| Binary operators | Left-nested operators of one binding-power class flatten into `G(first, I(L op operand, …))`; the classes copy the parser's binding powers |
| Or-pattern | `G(p1, I(L "\|" p2, …))`; a leading `\|` is kept |
| Forms | `name!` + hug list; a `{}` form with arms (`choice!{…}`) is vertical |
| Text `t![…]` | `t![` + `G(I(S, item, L, item, …), S)` + `]`; the items (strings, rubies, effects, interpolations) are juxtaposed, without commas |

What the formatter does **not** do: reorder or merge `use` items, sort anything, rewrite
literals, add or remove parentheses, or change comments' text (line comments lose trailing
whitespace only). Every change is whitespace, newlines, and commas.

An example from the fixtures (`tests/snapshots/format__fixtures@006__forms.kix.snap`):

```kix
#[scene]
pushdown fn Hub() {
    s!(t!["The school festival starts today."]);
    s!(
        render = |mut screen| {
            screen.background().change(BG::ROOF).apply_animation(fade(30));
            screen
        },
        t!["The wind on the roof is cold."],
    );
    loop {
        match choice!{
            id = "map",
            t!["Go to stall A"] => Map::A,
            t!["Leave"] if VISITED_A => Map::Leave,
        } {
            Map::A => StallA(),
            Map::Leave => break,
        }
    }
}
```

## Safety check

`verify::check` reparses the output with the same `SourceKind` and requires:

1. no syntax errors, and
2. the same *signature* as the input: the sequence of significant tokens `(kind, text)` with
   whitespace and **commas** removed, and the sequence of comment texts (right-trimmed).

Commas are excluded because the list layouts add and drop trailing commas. Any difference is
`FormatError::Internal("formatting changed the program; this is a formatter bug")`; the caller
leaves the file untouched (`kagari fmt` reports it and exits 1; the LSP returns no edit). This
turns layout bugs into refusals instead of silent miscompilation.

## Idempotence

`format(format(x)) == format(x)` for every `x` that formats. The printer cannot guarantee this by
itself: a rule that reads source facts (blank lines, whether a block had a comment, `(a,)`) must
produce output from which it reads the same facts again. Typical sources of non-idempotence are
a rule that keys on source line breaks (other than blank lines), a comment whose ownership
changes after the first pass (a trailing comment pushed onto a line of its own becomes a leading
comment), and hug decisions that depend on comments the first pass moved.

## Tests

`tooling/kagari-fmt/tests/format.rs`:

| Test | Checks |
| ---- | ------ |
| `fixtures` | Each `tests/fixtures/NNN__name.k{is,ix}` formats, formats again to the same text, and matches its `insta` snapshot |
| `corpus_is_stable` | Every file that parses cleanly under `core/kagari-std/kis`, `core/kagari-lang/tests/bytecode`, `core/kagari-lang/tests/game_source` and `core/kagari-syntax/tests/parse` (more than 30) formats without `Internal` errors and idempotently. It does **not** require those files to be formatted already |
| `refuses_syntax_errors` | `FormatError::Syntax` |
| `empty_file_stays_empty` | `""` and whitespace-only input → `""` |

`src/doc/tests.rs` unit-tests the printer on hand-built docs (fit/break, `IfBreak`, conditional
groups, blank lines, trailing whitespace). `tooling/kagari-cli/tests/fmt.rs` checks the CLI, and
asserts that a fresh `kagari init` project is already formatted. The LSP end-to-end test formats
a misformatted function.

Workflow: `cargo test -p kagari-fmt`; after an intended layout change, review and accept the
snapshot diffs with `cargo insta review` (or `INSTA_UPDATE=always cargo test -p kagari-fmt` and
inspect `git diff tests/snapshots`).

## Invariants

- **Idempotence**: `format(format(x)) == format(x)`. Enforced on every fixture and the corpus.
- **Program preservation**: output reparses without errors to the same tokens (commas aside) and
  comments. Enforced at run time by `verify`.
- Files with syntax errors are refused, never partially formatted.
- Every significant token is emitted exactly once, through the `Formatter` token helpers, so its
  comments are emitted exactly once.
- Not configurable: `MAX_WIDTH` and `INDENT` are constants.
- No trailing whitespace, at most one consecutive blank line, exactly one final newline.
- `kagari init`'s template is formatted output (`tooling/kagari-cli/tests/fmt.rs` checks it).

## When you change this

- **New syntax** in `kagari-syntax` (a new `SyntaxKind` node): add an arm to `Formatter::node`
  (otherwise it falls back to `verbatim`, which preserves but does not lay out), add a fixture
  under `tests/fixtures/` covering short, long (over 100 columns) and commented variants, accept
  the snapshot, and keep `corpus_is_stable` green. Also update the tree-sitter grammar
  ([Language Server and Editors](./lsp.md#tree-sitter-grammar)).
- **Changed layout**: update the affected snapshots, re-format the template
  (`tooling/kagari-cli/template/**`) with `kagari fmt FILES…` so that `fresh_project_is_formatted`
  passes, and check that examples in the Reference and STD books still look like formatter
  output.
- **New fixture**: name it `NNN__topic.kis` or `.kix` with the next number; the snapshot name
  follows from it.
- User-facing behaviour of `kagari fmt` (flags, messages, exit codes) is documented in the
  Reference [The `kagari` CLI](../../../reference/en/src/tools/cli.md) (en and ja); the
  editor integration in [Editor Support](../../../reference/en/src/tools/editors.md).
