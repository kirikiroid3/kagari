# Documentation

**Where in the code**

- Books: `docs/reference/{en,ja}`, `docs/std/{en,ja}`, `docs/dev`, each a standalone mdBook
  (`book.toml` + `src/SUMMARY.md`).
- Built site: `docs/_site/` (ignored by git).
- Agent skills: `.agents/skills/<name>/SKILL.md`; the documentation skill is `kagari-docs`.
- Tests that read the docs: `core/kagari-lang/tests/diagnostics_book.rs`.
- Code highlighting: `tooling/mdbook-kagari`, an mdBook preprocessor registered in every
  `book.toml` (see [Code highlighting](#code-highlighting)).

## The rule

> Every change updates the documentation that describes it, in the same change.

- A change a studio can observe (syntax, a diagnostic, a std item, an attribute, a CLI flag, a
  manifest key, a runtime option, headless commands, an asset format) updates the **Reference** or
  the **STD Reference** in **English and Japanese** in the same commit.
- A change to how Kagari works inside (compiler, IRs, formats, VM, engine, tooling, process)
  updates the **Develop Guide** in the same commit.
- A design decision, or a change to one, updates
  [Design Decisions](../foundations/decisions.md) (status included). A question that gets answered
  leaves [Open Questions](./open-questions.md).

## Which book holds what

| Book | Audience | Holds | Never holds |
| ---- | -------- | ----- | ----------- |
| The Kagari Reference (en, ja) | Studios writing and shipping games | Concepts, the language, special types, traits and attributes, projects, the CLI, editors, the runtime and headless mode, the diagnostics index | Crate names, internal module paths, IR names (HIR, THIR, MIR), opcodes and byte layouts. Bytecode is only "the `.kagari` file" |
| Kagari STD Reference (en, ja) | Studios | Every public item of `std` in the page template below, recipes, the user-visible compiler rules for std items (`appendix/rules.md`), runtime profiles | Host implementation details |
| Kagari Develop Guide (en) | Maintainers and contributors | Why and how: design decisions with rationale, the proposal mapping, the first design's history, crate layout, compiler internals, the std/host ABI, the std–checker contract, the `.kagari` byte layout, archive and save formats, VM, engine, tooling internals, testing, this policy, open questions | User-facing rules restated; link to them instead |

When a fact has a user-facing side and an implementation side, the user-facing fact goes into the
Reference or STD Reference and the implementation detail into this guide, which links to the
reference page. Reference and STD pages link to the Develop Guide only from their Introduction
("for internals see …").

## Writing style (all books)

- Plain, precise, reference-style English. Present tense. Short sentences. Tables for enumerable
  facts. No marketing tone, no emoji.
- Code blocks are tagged `kis` (plain Kagari), `kix` (uses forms), `wgsl`, `rust`, `sh`, `toml` or
  `text`. `kis` and `kix` blocks are highlighted at build time by the Kagari grammar
  ([Code highlighting](#code-highlighting)), so an untagged Kagari block stays plain.
- Every Kagari example must be valid against the current code and std: names, signatures, forms.
  Prefer examples that also appear in a test fixture (for instance
  `core/kagari-lang/tests/game_source/script/001__book_examples`).
- The Reference marks unsettled design as *Tentative*. The STD Reference uses the stability index
  instead (Stable, Tentative, Planned).
- Links within a book are relative (`../concepts/scenes.md#entry-scenes`). Link to an anchor only
  when the heading exists. Links to another book are relative paths between **source** files, for
  example from `docs/reference/en/src/concepts/x.md` to `../../../../std/en/src/screen.md`; they
  work on GitHub and in the built site because every `build-dir` mirrors the source tree under
  `docs/_site/`. The language switch works the same way (`../../ja/src/introduction.md` from
  `docs/reference/en/src/introduction.md`). Use cross-book links sparingly.

## Bilingual rules (Reference and STD Reference)

- **Mirror.** `ja/src` mirrors `en/src` exactly: the same files, the same `SUMMARY.md` order, the
  same heading structure, the same code blocks. A change to an English page changes its Japanese
  page in the same commit, and the reverse.
- **Shared anchors.** Every heading of a `ja` file carries an explicit id equal to the English
  file's anchor: `## エントリーシーン {#entry-scenes}`. mdBook's automatic anchor is the heading
  text in lowercase, spaces replaced by `-`, punctuation such as backticks, `:`, `.` and `(`
  removed: ``## `std::screen` `` → `stdscreen`, ``## Rust integers, `Rat` and `Decimal` `` →
  `rust-integers-rat-and-decimal`. When the automatic English anchor would be ugly or ambiguous,
  give the English heading an explicit id too, and use the same id in `ja`. Links are then
  identical in both languages.
- **Identical code blocks.** Code blocks are byte-identical between `en` and `ja`, including
  comments and string contents.
- **Japanese style.** です・ます調; full-width `、。`; a half-width space between Japanese text and
  inline code or ASCII words (`` `Screen` 型 ``, `Rust の`). Identifiers, keywords, attribute names
  and diagnostic codes stay in ASCII code spans. Write natural Japanese, not a word-for-word
  translation.
- **Diagnostics appendix.** Every diagnostic code has exactly one `## E1234 …` (or `## W…`)
  heading in `docs/reference/en/src/diagnostics/*.md` and one in the `ja` mirror. The
  `diagnostics_book` test in `kagari-lang` enforces it, so adding, removing or renumbering a code
  without updating both appendices fails `cargo test`.

### Glossary

Use these renderings consistently. The Japanese edition of the engine proposal
(`../proposal/vn-engine/proposal2-jp/vn-engine-proposal.typ`) is the tie-breaker.

| en | ja | en | ja |
| -- | -- | -- | -- |
| story | 物語 | story variable | 物語変数 |
| scene | シーン | instant procedure | 即時手続き |
| step | ステップ | choice (step) | 選択肢（ステップ） |
| arm (of choice/match) | アーム | guard | ガード |
| entry scene | エントリーシーン | ending | エンディング |
| function kind / kind | 関数種別 / 種別 | finite type | 有限型 |
| pushdown system | プッシュダウンシステム | reachability | 到達可能性 |
| decidable | 決定可能 | termination | 停止性 |
| render function | レンダー関数 | render | レンダー |
| screen | 画面 | logical screen | 論理画面 |
| entity / component | エンティティ / コンポーネント | keyframe | キーフレーム |
| consistency | 一貫性 | unbounded screen | 非有界な画面 |
| constant expression | 定数式 | const evaluation | 定数評価 |
| tabulation / folding | 表化 / 畳み込み | form (`s!` etc.) | フォーム |
| text effect | テキスト効果 | mark | マーク |
| timeline / cursor | タイムライン / カーソル | transition / tween / easing | トランジション / トゥイーン / イージング |
| motion | モーション | layer / plane | レイヤー / プレーン |
| save / trace / snapshot | セーブ / トレース / スナップショット | meta | メタ |
| setting | 設定 | mode | モード |
| game system | ゲームシステム | signal | シグナル |
| closure | クロージャ | reference (`&T`) | 参照 |
| second-class | 第二級 | value semantics | 値セマンティクス |
| module / item | モジュール / アイテム | attribute | 属性 |
| trait | トレイト | generics | ジェネリクス |
| builtin | ビルトイン | prelude | プレリュード |
| asset | アセット | diagnostic | 診断 |
| runtime | ランタイム | headless mode | ヘッドレスモード |
| stability: Stable / Tentative / Planned | 安定 / 暫定 / 予定 | studio | スタジオ |
| shader / role / effect value | シェーダー / ロール / 効果値 | mix / mask / paint | ミックス / マスク / ペイント |
| local motion / transitional motion | ローカルモーション / 遷移モーション | travel / pose | トラベル / ポーズ |

A new term that recurs gets a row here and in the Reference's glossary appendix in the same change.

## Diagrams

- Draw diagrams as standalone SVG files in the book's `src/diagrams/`, named after the chapter
  (`architecture-overview.svg`). In the Reference and STD Reference each language has its own
  diagrams directory; `ja` diagrams carry Japanese labels.
- Include them inline with `\{{#include ../diagrams/NAME.svg}}` (the path is relative to the
  including file).
- Style: a `viewBox`, `width="100%"`, `style="max-width:…px"`, `role="img"` and an `aria-label`
  that states what the picture says. An embedded `<style>` uses only mdBook theme variables with
  fallbacks (`var(--fg,#333)`, `var(--quote-bg,#f6f7f6)`, `var(--links,#2f5d8a)`), and every class
  has the `kg-` prefix. Marker and `<defs>` ids are unique per diagram (`kg-arrow-<name>`), because
  all inline SVGs of a page share one id namespace. No scripts, no external references, no raster
  images. [`layout-crates.svg`](../diagrams/layout-crates.svg) is an example.
- Immediately before every `\{{#include …svg}}`, add a hidden *mermaid-ish* comment so that readers
  of the Markdown source (people and AI agents) get the picture's content:

  ```text
  <!-- diagram: architecture-overview (mermaid-ish)
  flowchart LR
    src[".kis/.kix sources"] ==> cli["kagari build"]
    cli ==> kagari["game.kagari"]
  -->
  ```

  Never write `-->` inside the comment body: it closes the HTML comment. Draw edges with `==>`
  or `-.->`. Keep the comment faithful to the SVG; change both together.
- Use diagrams only where structure helps: pipelines, state machines, call stacks, layouts, state
  transitions, crate graphs, byte layouts.

## STD Reference page template

Each module page follows a Node.js-API-reference shape:

````markdown
# `std::screen`

> **Stability:** Stable — one line summary of the module.

Intro paragraphs: what the module is for, its kind (`const fn` / `fn`), how it relates to others.

## Overview

A table of the items on the page (name, kind, summary). Optional.

## Struct: `Color`

```kis
pub struct Color { pub r: u8, pub g: u8, pub b: u8, pub a: u8 }
```

Description.

### `Color::rgb(r, g, b)`

```kis
pub const fn rgb(r: u8, g: u8, b: u8) -> Color
```

- `r` `u8` — …
- Returns: `Color` — …
- Kind: `const fn` · Builtin: no · (Profile: not in `native_v0` — when relevant)

Semantics, edge cases, related diagnostics (linked to the Reference's diagnostics pages), a short
example.
````

- One H2 per type, prefixed `Struct:`, `Enum:`, `Trait:`, `Type:`, `Constant:` or `Function:`;
  free functions are `## Function: name(args)`. One H3 per method, associated function or group
  of variants.
- Stability values are Stable, Tentative and Planned, with optional per-item overrides. Japanese
  pages write the line as `> **安定性:** 安定 — …` with the values 安定, 暫定 and 予定.
- Keep the declared order of the module's `.kis` file unless a grouping reads clearly better.
  Every public item of the file appears.

## Develop Guide conventions

- English only.
- Each chapter starts with **Where in the code** (crate and file paths) and ends, where useful,
  with **Invariants** (what a change must preserve) and **When you change this** (what else must
  change: tests, Reference/STD en+ja, diagnostics appendix, tree-sitter grammar, formatter,
  bytecode version, …).
- Design decisions are short records: **Decision**, **Why**, **Consequences** (including the
  alternatives rejected), with a status (Implemented, Partly implemented, Planned).
- Byte layouts are tables (offset, size, field, meaning) and, where it helps, an SVG.
- Assume the reader knows Rust and compilers. Name files and functions so they can be found with
  a search.
- Code comments may cite a chapter (`docs/dev/src/compiler/analysis.md`); keep such citations
  pointing at existing files when chapters move.

## Agent skills

Project skills for AI agents live in `.agents/skills/<name>/SKILL.md`:

```markdown
---
name: kagari-<area>
description: One line saying exactly when to load it (which crates, paths or tasks).
---

Body: scope (paths), what to read first (Develop Guide chapters), invariants, step-by-step
workflow for the common changes, which docs and tests to update, commands to verify.
```

Skills stay short (about 60–150 lines) and point to Develop Guide chapters instead of repeating
them. A change to a chapter that a skill summarizes updates the skill too.

## Building and checking

```sh
mdbook build docs/reference/en
mdbook build docs/reference/ja
mdbook build docs/std/en
mdbook build docs/std/ja
mdbook build docs/dev
```

Every build runs the `mdbook-kagari` preprocessor through Cargo, so it needs the Rust toolchain
and a C compiler (the normal dev shell); the first build compiles the preprocessor.

Output goes to `docs/_site/<book>/…/src/`, mirroring the sources, so relative links between books
work in the built site. `mdbook build` fails on a missing `\{{#include}}` file and reports
`SUMMARY.md` entries without a file, but it does not check links.

Before committing a docs change, run:

```sh
python3 docs/check-books.py                         # links, anchors, en/ja mirror, diagrams
cargo test -p kagari-lang --test diagnostics_book   # every diagnostic code once per language
```

`docs/check-books.py` needs only Python 3. It checks that every `SUMMARY.md` entry exists and
every page is listed; that relative links and `#anchors` resolve, across books too (anchors are
computed the way mdBook computes them, plus explicit `{#id}` and inline `<a id>`); that each `ja`
tree mirrors its `en` tree (same files, same heading ids, byte-identical code blocks, an explicit
`{#id}` on every `ja` heading); and that every included diagram exists and has its mermaid-ish
comment. It exits with status 1 and lists the problems otherwise.

## Code highlighting

mdBook's bundled highlight.js knows no Kagari, so every `book.toml` registers a preprocessor:

```toml
[preprocessor.kagari]
command = "cargo run --quiet --package mdbook-kagari --"
after = ["links"]
```

It runs after `links`, so code pulled in with `\{{#include}}` is highlighted too, and only for the
HTML renderer (`mdbook-kagari supports html`). Each fenced block whose info string starts with
`kis` or `kix` becomes highlighted HTML; every other block, and all other text, is left byte for
byte for highlight.js (`tooling/mdbook-kagari/src/chapter.rs`).

- **One grammar, one query file.** `build.rs` compiles `tooling/tree-sitter-kagari/src/parser.c`
  into the binary, and `src/highlight.rs` embeds the Zed extension's `highlights.scm` with
  `include_str!`. The books and Zed therefore color the same nodes, and neither has a second
  set of rules.
- **Zed's precedence.** When captures overlap, the innermost node wins; for the same node the
  later pattern (or the later capture of one pattern) wins. The `tree-sitter-highlight` crate
  lets the first pattern win, so the preprocessor resolves captures itself with a `QueryCursor`.
- **Snippets.** Book code is often a bare statement, an expression or a bodyless std signature.
  A block that has syntax errors on its own is parsed again inside
  `fn __kagari_snippet() { … }`; a byte inside an `ERROR` node of the first parse but not of the
  second takes its class from the second. Blocks mixing statements and items are thus colored
  throughout; text that parses neither way stays partly plain.
- **Output.** The block becomes `<pre><code class="hljs nohighlight">` with `<span class="hljs-…">`
  runs, written on one line with newlines as `&#10;`: a raw HTML block inside a list item or
  block quote would otherwise need the container's indentation or `>` on every line.
  `nohighlight`, without a `language-` class, makes highlight.js skip the block, and the
  `hljs-*` classes are the ones mdBook's themes already color. The Coal and Navy themes
  (`tomorrow-night`) have no color for `hljs-type` and `hljs-meta`.

Capture names map to classes the way highlight.js classes Rust (`class_of`); captures without a
class (operators, punctuation) keep the color of the enclosing capture:

| Capture | Class |
| ------- | ----- |
| `keyword`, `variable.special` (`self`) | `hljs-keyword` |
| `type` | `hljs-type` |
| `type.builtin`, `function.special` (form names) | `hljs-built_in` |
| `function`, `module` | `hljs-title` |
| `constant`, `boolean` | `hljs-literal` |
| `property` | `hljs-attr` |
| `variant`, `label` | `hljs-symbol` |
| `attribute` | `hljs-meta` |
| `comment`, `comment.doc` | `hljs-comment` |
| `string`, `string.escape` | `hljs-string` |
| `number` | `hljs-number` |

**When you change this**

- A new book or a moved book: `book.toml` (`build-dir` must mirror the source tree, and the
  `[preprocessor.kagari]` table), the [introduction](../introduction.md), the root `README.md`,
  and the `kagari-docs` skill.
- A change to these rules: the `kagari-docs` skill in the same change.
- A new capture name in `highlights.scm` that the books should color: an arm in `class_of`
  (`tooling/mdbook-kagari/src/highlight.rs`) and a row in the table above.
