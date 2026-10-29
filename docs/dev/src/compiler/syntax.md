# Syntax: Lexer, CST, AST and Forms

`kagari-syntax` turns the text of one `.kis` or `.kix` file into a lossless
[rowan](https://github.com/rust-analyzer/rowan) tree plus a list of syntax errors, and offers a
typed AST view over that tree. It knows nothing about other files, names or types. Forms (`s!`,
`choice!`, `t!`, the UI forms) are parsed and validated here and desugared one layer up, during
HIR lowering in `kagari-lang`.

## Where in the code

| Path | Contents |
| ---- | -------- |
| `core/kagari-syntax/src/lib.rs` | Re-exports: `parse`, `Parse`, `SourceKind`, `SyntaxError`, `SyntaxKind`, `SyntaxNode`, `TextRange` |
| `core/kagari-syntax/src/parse.rs` | `parse(text, kind)`: lex, parse, validate, sort errors |
| `core/kagari-syntax/src/lex/mod.rs` | The logos lexer (`Tok`), `lex`, tuple-index splitting |
| `core/kagari-syntax/src/cst/mod.rs` | `SyntaxKind` (every token and node kind), keywords, composite punctuation, `KagariLanguage` |
| `core/kagari-syntax/src/cst/parser/` | Recursive-descent parser: `mod.rs` (token access, tree building, errors), `items.rs`, `exprs.rs` (statements, Pratt loop), `patterns.rs`, `types.rs`, `paths.rs`, `forms.rs` |
| `core/kagari-syntax/src/validate.rs` | Post-parse validation: forms, string escapes, integer literals, forms in `.kis` |
| `core/kagari-syntax/src/ast/` | Typed AST: `ast_node!`/`ast_enum!` wrappers, accessors; `forms.rs` holds `UI_FORMS`; `literal.rs` holds literal parsing and unescaping |
| `core/kagari-syntax/tests/` | `snapshots.rs` + `parse/*.k{is,ix}` (CST snapshots), `ast.rs` (accessor tests) |
| `core/kagari-lang/src/parse_query.rs` | The `parse` query: picks the `SourceKind` from the file extension |
| `core/kagari-lang/src/hir/forms.rs` | Form desugaring into ordinary HIR |
| `tooling/tree-sitter-kagari/grammar.js` | Editor grammar, kept in step by hand |
| `tooling/kagari-fmt/src/rules/` | Formatter rules per node kind |

## Layers

<!-- diagram: syntax-layers (mermaid-ish)
flowchart LR
  text["source text + SourceKind"] ==> lex["lex (logos): tokens + lex errors"]
  lex ==> parser["parser: rowan green tree + errors"]
  parser ==> validate["validate: forms, literals, .kis rule"]
  validate ==> parse["Parse (lossless, errors sorted)"]
  parse ==> ast["typed AST view (ast::*, accessors return Option)"]
  ast ==> itemtree["item tree (kagari-lang)"]
  ast ==> hir["HIR lowering, forms desugared"]
  ast ==> fmt["kagari-fmt rules"]
  ast ==> ide["IDE (CST helpers)"]
-->
{{#include ../diagrams/syntax-layers.svg}}

`parse(text, kind) -> Parse` is the only entry point. `Parse` holds the `GreenNode` and the
errors, sorted by `(start, end, message)` and deduplicated. `Parse::syntax_node()` builds a red
tree, `Parse::tree()` returns the typed `ast::SourceFile`. The root text of the tree always equals
the input, byte for byte, whatever the input.

In `kagari-lang`, the `parse` query wraps this: `.kis` files parse as `SourceKind::Kis`, every
other extension as `SourceKind::Kix` (the module lookup only ever finds `.kis`/`.kix` files, and a
root file with another extension is `E1001`). A missing file parses as the empty string. Every
`SyntaxError` becomes one `E0001` diagnostic in `check`; the syntax crate has no codes of its own.

## `.kis` and `.kix`

> **Decision.** One grammar for both extensions. The parser accepts forms in both; the validator
> reports a form in a `.kis` file (``forms are only allowed in `.kix` files``, once per outermost
> form).
>
> **Why.** The extension decides syntax only, never semantics. One parser keeps the tree shape
> identical, so the formatter, the IDE layer and HIR lowering need no mode. Reporting forms in
> `.kis` as a validation error gives a precise message instead of a cascade of parse errors.
>
> **Consequences.** A form in a `.kis` file still lowers to HIR, so later stages may report more
> errors for it. `SourceKind` is consulted only by `validate.rs`.

## Lexer

The lexer is a `logos` enum `Tok` mapped to `SyntaxKind` by `kind_of`:

| Token | Rule |
| ----- | ---- |
| Whitespace | Runs of ASCII whitespace plus U+000B, U+000C, U+0085, U+200E, U+200F, U+2028, U+2029 |
| `LINE_COMMENT` | `//` to end of line (doc comments `///` included; the IDE reads them from the CST) |
| `BLOCK_COMMENT` | `/* … */`, **nested**; unterminated: error `unterminated block comment`, token runs to EOF |
| `IDENT` | `[\p{XID_Start}_]\p{XID_Continue}*`; `_` alone is `UNDERSCORE`; strict keywords via `SyntaxKind::from_keyword` |
| `INT_NUMBER` | `[0-9][0-9a-zA-Z_]*`: radix prefix, digits and suffix in one token; checked later by `ast::parse_int_literal` |
| `DECIMAL_NUMBER` | `[0-9][0-9_]*\.[0-9][0-9_]*` |
| `STRING` | `"…"` with `\` skipping one byte; unterminated: error, token runs to EOF; escapes checked by the validator |
| `LABEL` | `'name` (there are no char literals) |
| Punctuation | **One character per token**: `( ) { } [ ] < > , ; : . = ! + - * / % & \| ^ # ?` |
| `ERROR_TOKEN` | Adjacent unrecognised characters, merged; error `unexpected character` |

After lexing, `split_tuple_indices` splits a `DECIMAL_NUMBER` that directly follows a single `.`
into `INT_NUMBER . INT_NUMBER`, so `t.0.1` is two field accesses while `0..1.5` keeps `1.5` whole.

Contextual keywords stay `IDENT` in the lexer: `pushdown` and `scene` are re-kinded to
`PUSHDOWN_KW`/`SCENE_KW` by the parser (`bump_remap`) where they start a `pushdown fn` item, a
`pushdown fn(…)`/`scene fn(…)` pointer type or a `pushdown Fn(…)` closure type. `Fn` is never a
keyword; the parser recognises `Fn(` by text.

### Composite punctuation

`::`, `->`, `=>`, `==`, `!=`, `<=`, `>=`, `&&`, `||`, `<<`, `>>`, the compound assignments,
`..` and `..=` are glued by the parser from **adjacent** single-character tokens
(`SyntaxKind::composite_parts`, `Parser::bump_composite`). So `>>` in `Vec<Vec<u8>>` stays two
`R_ANGLE` tokens until an expression context asks for `SHR`, and `> >` with a space is never a
shift. The tree contains the composite kind as one token.

## Syntax kinds and the CST

`SyntaxKind` (in `cst/mod.rs`) is generated by the `kinds!` macro: a `#[repr(u16)]` enum plus
`SyntaxKind::ALL`, indexed by raw value (`from_u16`). Its sections are trivia, literals and names,
strict keywords, contextual keywords, single-character punctuation, composite punctuation, the root
`SOURCE_FILE`, then node kinds for items, names and paths, types, patterns, statements,
expressions and forms, and finally `ERROR`. Raw values are not persisted anywhere, so inserting a
kind in the middle is fine.

Helper tables live next to the enum: `from_keyword` (strict keywords), `composite_parts`,
`describe` (the text used in `expected …` messages) and `is_trivia`.

### Parser

The parser (`cst/parser/`) is hand-written recursive descent with a Pratt loop for binary
operators, building a `rowan::GreenNodeBuilder` tree directly.

- **Token access** counts only significant tokens (`nth`, `nth_at`, `at`); `nth_at` with a
  composite kind checks that its parts are adjacent.
- **Trivia** is attached lazily: pending whitespace and comments are flushed into the current node
  right before the next token or node start. Nodes therefore never begin or end with trivia, and
  trailing trivia ends up in the root.
- **Checkpoints** (`checkpoint` + `start_at`) wrap an already parsed left-hand side into a
  `BIN_EXPR`, `CALL_EXPR`, `FORM_ARM`, …
- **Restrictions**: `no_struct` (in `if`/`while`/`match`/`for` heads, `PATH {` is not a record
  literal) and `prefer_stmt` (statement position: a block-like expression ends the statement, so
  `if c {} (x)` is two statements).
- `Done { kind, block_like }` is what an expression parse returns; block, `if`, `match`, loops and
  brace forms `name!{…}` are block-like and need no `;` as statements.

Binding powers (`exprs.rs`, lowest first):

| Operators | Left, right | Notes |
| --------- | ----------- | ----- |
| `=` and compound assignments | 1, 1 | Right-associative |
| `..`, `..=` | 3, 4 | Operands optional (`..`, `a..`, `..=b`) |
| `\|\|` | 5, 6 | |
| `&&` | 7, 8 | |
| `== != < > <= >=` | 9, 10 | Chaining two comparisons is the error `comparison operators cannot be chained` |
| `\|` | 11, 12 | |
| `^` | 13, 14 | |
| `&` | 15, 16 | |
| `<< >>` | 17, 18 | |
| `+ -` | 19, 20 | |
| `* / %` | 21, 22 | |
| `as` | 23 | Parsed (`CAST_EXPR`); rejected by inference as `E2019` |
| prefix `- ! * &` | 25 | |

Postfix operators (`?`, `.field`, `.0`, `.method(…)`, `.method::<…>(…)`, call, index) bind tighter
than all of these.

### Error recovery

The parser never fails and never loses input:

- `expect(kind)` reports `expected …` at an **empty range right after the last consumed token**
  (`error_missing`) and consumes nothing.
- `err_bump` reports at the next token and wraps it in an `ERROR` node.
- `err_missing_node(message, stop)` leaves an empty `ERROR` node when the next token belongs to an
  enclosing construct (an item start or a token in `stop`), otherwise wraps it.
- At item level, unexpected tokens are collected into one `ERROR` node until the next token in the
  item recovery set (`at_item_start`: `fn const static struct enum type trait impl mod use pub #`
  and `pushdown fn`) or `}`.
- In blocks, a missing `;` is reported and `recover_stmt` skips to the next `;` (consumed) or `}`
  unless the next token can start a statement.
- Lists use `list_comma`: a missing comma is reported only when another element follows.
- `finish_root` bumps anything left over (only reachable after a parser bug) so the tree stays
  lossless.

`tests/snapshots.rs::every_prefix_is_lossless` parses every char-boundary prefix of every fixture
and asserts the tree text equals the prefix: recovery never loses input, panics or loops on
truncated files.

### Validation

`validate.rs` walks the finished tree once:

- every `STRING` token through `ast::unescape_string` (invalid escapes are reported at their
  position inside the literal);
- every `INT_NUMBER` through `ast::parse_int_literal` (radix `0x`/`0o`/`0b`, `_` separators,
  suffix from `INT_SUFFIXES`, value ≤ `u128::MAX`);
- every form: the `.kis` rule above, the name (`s`, `choice`, `t` or a UI form), the delimiter,
  attribute placement (before elements, no duplicates, only known keys), arms only in `choice!`,
  element counts, required attributes (`choice!` needs `id`, a string literal; UI forms per
  `UI_FORMS`), and the element shapes (`s!` takes at most one element, which must be `t![…]`;
  every `choice!` element must be an arm `t![…] [if guard] => value`).

Validation needs no names or types, so these are syntax errors (`E0001`).

## Typed AST

`ast/` wraps nodes in newtypes (`ast_node!(Fn, FN)`) and unions (`ast_enum!(Expr { … })`), in
rust-analyzer's style and naming. Every accessor returns `Option` or an iterator, because the tree
may come from erroneous input. Shared traits: `HasAttrs`, `HasName`, `HasVisibility`,
`HasGenericParams`. Accessors that depend on token position use the helpers `child_before`,
`child_after` and `child_after_token` (for example `FormArm::value` is the expression after
`=>`). Literal helpers are `Literal::int_value`, `decimal_text`, `string_value`, and the free
functions `parse_int_literal` and `unescape_string`.

## Forms

Forms are fixed syntax, not macros: there is no macro system and the set of forms is closed.

| Stage | `s!(…)` | `choice!{…}` | `t![…]` | UI forms (`column![…]`, `button!{…}`, …) |
| ----- | ------- | ------------ | ------- | ---------------------------------------- |
| Parse (`parser/forms.rs`) | `FORM_EXPR` with `FORM_ATTR` (`key = expr`) and expression entries | `FORM_EXPR`; an entry followed by `if`/`=>` becomes `FORM_ARM` with `FORM_GUARD` | `TEXT_FORM` with juxtaposed `TEXT_STRING`, `TEXT_RUBY` (+ `RUBY_READINGS`), `TEXT_EFFECT`, `TEXT_INTERP`; commas are an error | `FORM_EXPR` |
| Validate (`validate.rs`) | keys `who voice render history`; ≤ 1 element, a `t![…]` | keys `id render`; `id` required, string literal; only text arms | no attributes | `UI_FORMS`: delimiter, keys, required keys, element count |
| Desugar (`hir/forms.rs`) | `push_step(Say { who, voice, render, text, history })`, each field `Some(…)`/`None` | `push_step(Choice { id, render, arms: [ChoiceArm { text, shown, value }, …] })`, `shown` = guard or `true` | one item: the item itself; several: `concat([…])`; string → literal, ruby → `ruby(base, r)` or `mono_ruby(base, [r…])`, effect → `effect(base, e)`, `{e}` → `to_text(e)`; empty → `""` | `ui::f([\|\| e, …])` or `ui::f(Props { key: …, children/content/…: … })` |

Desugaring details shared by all forms:

- Desugared code refers to std items with `Path { kind: PathKind::Lang(LangItem::…) }`.
  `LangItem::std_path` lists each target (`narrative::push_step`, `narrative::Say`,
  `Option::Some`, `narrative::text::concat`, `ui::button`, `ui::ButtonProps`, …). Name resolution
  resolves them from the std root regardless of the user's scope; a missing target is `E1026`.
- Every synthesized node gets the **whole form's range** in the source map and the form's
  `source_order`, so diagnostics on desugared code point at the form.
- `debug::resolution_dump` and the IDE lookup skip `Lang` paths.
- UI forms: `ast::UI_FORMS` (syntax: name, delimiter, `(key, required)` pairs in the order of the
  props struct's fields, element count) is shared by validation and lowering; `hir/forms.rs`
  `ui_target` adds the target function, props struct and element field. Reactive attributes become
  `|| expr` (wrapped in `Some` when optional), `on_*` handlers stay as written (in `Some`), absent
  optional attributes become `None`, every element becomes `|| e`, and inside `column!`, `row!`,
  `button!` and `toggle!` a `t![…]` element becomes `|| ui::label(t![…])`. A missing required
  attribute lowers to `Expr::Missing` (the syntax error is already reported).
- The rules that need types (`choice!` ids unique and arms distinct fieldless variants of one
  enum, mono-ruby reading counts, constant text effects) are kind checks on the desugared
  `Choice { … }` record and the `mono_ruby`/`effect` calls, so a hand-written `push_step(Choice {
  … })` is checked the same way. See [Type Inference and Checks](./typeck.md#form-rules).

The user-facing desugaring tables are in the Reference
([Forms](https://kagari.suitsu31.club/en/reference/syntax/forms/)) and the STD Reference
([`std::narrative::text`](https://kagari.suitsu31.club/en/std/narrative-text/), [`std::ui`](https://kagari.suitsu31.club/en/std/ui/)).

## Tests

| Test | What it checks |
| ---- | -------------- |
| `tests/snapshots.rs::parse_fixtures` | `insta::glob!("parse/*.k{is,ix}")`: the CST debug dump plus `error <range>: <message>` lines for each fixture, in `tests/snapshots/snapshots__parse_fixtures@<fixture>.snap`; also asserts losslessness |
| `tests/snapshots.rs::every_prefix_is_lossless` | Every prefix of every fixture parses losslessly |
| `tests/ast.rs` | Typed accessors on small inputs (items, expressions, forms, text items) |

Fixture names are `kis_NNN__topic.kis` / `kix_NNN__topic.kix`; the extension selects the
`SourceKind`. Run `cargo test -p kagari-syntax`; after an intended tree change, accept the new
snapshots (see [Testing](../process/testing.md)) and review the diff.

## How to

### Add a token or keyword

1. Single character: a `Tok` variant with `#[token("…")]`, a `SyntaxKind` in the punctuation
   section, a `kind_of` arm and a `describe` text. Never add a multi-character punctuation token to
   the lexer: add a composite kind with `composite_parts`, add it to `COMPOSITE_OPS` (longest
   first) and `infix_bp` if it is an operator, and to `describe`.
2. Strict keyword: a `*_KW` kind, a `from_keyword` arm, a `describe` text. Prefer a contextual
   keyword (re-kinded with `bump_remap`) when the word is a valid identifier today, so existing
   programs keep compiling.
3. Update `tooling/tree-sitter-kagari/grammar.js` (and the Zed `highlights.scm` if it should be
   highlighted) and the Reference [Lexical Structure](https://kagari.suitsu31.club/en/reference/syntax/lexical/)
   page in en and ja.

### Add a syntax node

1. Add the node kind to `kinds!` in its section.
2. Parse it in the matching `cst/parser/*.rs` file; use checkpoints for left-recursive shapes, and
   give it recovery that consumes at least one token or stops at a token an enclosing rule wants.
3. Add the `ast_node!` wrapper and `Option` accessors in `ast/`; add it to the relevant
   `ast_enum!` (`Expr`, `Pat`, `Type`, `Item`, …) and trait impls.
4. Add or extend a fixture in `core/kagari-syntax/tests/parse/` and accept the snapshot; add an
   accessor test in `tests/ast.rs`.
5. Lower it in `core/kagari-lang/src/hir/lower.rs` (and `item_tree.rs` for items), then follow it
   through resolution, inference and the checks.
6. Add a formatter rule in `tooling/kagari-fmt/src/rules/` and a fixture in
   `tooling/kagari-fmt/tests/fixtures/`; the formatter's verifier rejects output that does not
   reparse to the same tokens.
7. Mirror it in `tooling/tree-sitter-kagari/grammar.js` and its `test/corpus/*.txt`.

### Add a form

1. Non-UI form: add `(name, delimiter)` to `FORMS` in `validate.rs` and its key/element rules in
   `validate_form`. UI form: add a `ui_form(…)` entry to `ast::UI_FORMS` with attributes in the
   order of the props struct's fields.
2. Declare the target in std (`core/kagari-std/kis/std/…`): the function and, for UI forms, the
   props struct.
3. Add `LangItem` variants with their `std_path`, and the desugaring in `hir/forms.rs`
   (`lower_form`, or `ui_target` for UI forms).
4. Add kind checks in `typeck/kinds/forms.rs` if the form has rules that need types.
5. Fixtures: a parse fixture (`kix_NNN__…`), a `game_source` fixture in `kagari-lang` for the
   desugared behaviour and its diagnostics, a formatter fixture (`rules/forms.rs` formats forms).
6. Docs: Reference [Forms](https://kagari.suitsu31.club/en/reference/syntax/forms/) (en and ja), the STD page of
   the target module (en and ja), and the desugaring table above.

## Invariants

- The tree is lossless for every input: `parse(text).syntax_node().text() == text`.
- The parser always terminates and every loop iteration consumes a token or exits.
- Nodes never start or end with trivia.
- The lexer emits single-character punctuation only; composites exist only in the tree.
- Syntax errors are collected, never thrown; `kagari-syntax` has no notion of diagnostic codes,
  names or types.
- Both source kinds share one grammar; `SourceKind` is read only by the validator.
- `UI_FORMS` attribute order equals the field order of the std props structs.
- Like every core crate, `kagari-syntax` denies `unwrap`, `expect`, `panic!` and unchecked
  arithmetic (`#![deny(clippy::…)]` in `lib.rs`): use `checked_*`/`saturating_*` and `Option`.

## When you change this

- Any change to tokens or grammar: `tooling/tree-sitter-kagari/grammar.js` (regenerate
  `src/parser.c`, `src/grammar.json`, `src/node-types.json` with `tree-sitter generate`, run
  `tree-sitter test`), the Zed extension's queries in `tooling/zed-kagari/languages/kagari/` and
  the grammar `rev` in `tooling/zed-kagari/extension.toml` once the grammar change is committed.
  See [Language Server and Editors](../tooling/lsp.md).
- `tooling/kagari-fmt`: a rule for each new node kind, fixtures and snapshots
  ([Formatter](../tooling/fmt.md)).
- Reference syntax chapters in en and ja (`docs/site/src/content/docs/{en,ja}/reference/syntax/*.md`), and
  [Differences from Rust](https://kagari.suitsu31.club/en/reference/syntax/rust-differences/) when the change
  moves Kagari closer to or further from Rust.
- New messages are still `E0001`; no diagnostics page change is needed unless the Reference
  [Syntax diagnostics](https://kagari.suitsu31.club/en/diagnostics/syntax/) page lists the case.
- Run `cargo test -p kagari-syntax`, `cargo test -p kagari-lang` (fixtures re-parse everything)
  and `cargo test -p kagari-fmt`.
