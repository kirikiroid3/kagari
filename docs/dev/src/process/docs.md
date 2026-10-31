# Documentation

**Where in the code**

- The documentation site (`docs/site`, an Astro project with the [Lotus](https://astro-theme-lotus.prosefly.dev/)
  theme), published at `https://kagari.suitsu31.club`:
  - Pages: `src/content/docs/{ja,en}/` with four sections, `tutorial/`, `reference/` (the
    Language Reference), `std/` (the STD Reference) and `diagnostics/`. Japanese is the root
    locale (`/reference/…`), English is under `/en/` (`/en/reference/…`).
  - `theme.config.json`: site identity, locales, header links and the sidebar (`docsNav`), which
    lists every page in order.
  - `astro.config.ts` (Kagari code highlighting), `src/markdown.ts` (remark plugins for links,
    heading ids and diagrams), `src/components/Home.astro` (the landing page, used by
    `src/pages/index.astro` and `src/pages/en/index.astro`), `src/components/SiteBrand.astro`
    (Lotus override: the header logo links to the current locale's home), `src/styles/site.css`.
  - `grammars/kagari.tmLanguage.json`: the site's copy of the Kagari TextMate grammar (see
    [Code highlighting](#code-highlighting)).
  - Hosting: Vercel, with Root Directory `docs/site` and `docs/site/vercel.json` (see
    [Building and checking](#building-and-checking)).
- The Develop Guide (`docs/dev`): a standalone mdBook (`book.toml` + `src/SUMMARY.md`), built to
  `docs/_site/` (ignored by git) and not published with the site.
- Agent skills: `.agents/skills/<name>/SKILL.md`; the documentation skill is `kagari-docs`.
- Tests that read the docs: `core/kagari-lang/tests/diagnostics_book.rs`.
- Code highlighting: `tooling/textmate-kagari` for the site, `tooling/mdbook-kagari` for the
  Develop Guide (see [Code highlighting](#code-highlighting)).

## The rule

> Every change updates the documentation that describes it, in the same change.

- A change a studio can observe (syntax, a diagnostic, a std item, an attribute, a CLI flag, a
  manifest key, a runtime option, headless commands, an asset format) updates the **Language
  Reference**, the **STD Reference** or **Diagnostics** on the site in **English and Japanese** in
  the same commit.
- A change to how Kagari works inside (compiler, IRs, formats, VM, engine, tooling, process)
  updates the **Develop Guide** in the same commit.
- A design decision, or a change to one, updates
  [Design Decisions](../foundations/decisions.md) (status included). A question that gets answered
  leaves [Open Questions](./open-questions.md).

## Which part holds what

| Part | Audience | Holds | Never holds |
| ---- | -------- | ----- | ----------- |
| Tutorial (site, ja, en) | People new to Kagari | A guided path from installation to a first game | Rules not stated in the references |
| Language Reference (site, ja, en) | Studios writing and shipping games | Concepts, the language, special types, traits and attributes, projects, the CLI, editors, the runtime and headless mode | Crate names, internal module paths, IR names (HIR, THIR, MIR), opcodes and byte layouts. Bytecode is only "the `.kagari` file" |
| STD Reference (site, ja, en) | Studios | Every public item of `std` in the page template below, recipes, the user-visible compiler rules for std items (`appendix/rules.md`), runtime profiles | Host implementation details |
| Diagnostics (site, ja, en) | Studios | Every diagnostic code, one heading per code, grouped by stage | Compiler internals |
| Kagari Develop Guide (en) | Maintainers and contributors | Why and how: design decisions with rationale, the proposal mapping, the first design's history, crate layout, compiler internals, the std/host ABI, the std–checker contract, the `.kagari` byte layout, archive and save formats, VM, engine, tooling internals, testing, this policy, open questions | User-facing rules restated; link to them instead |

When a fact has a user-facing side and an implementation side, the user-facing fact goes onto the
site and the implementation detail into this guide, which links to the site page. Site pages link
to the Develop Guide only from the introductions ("for internals see …"), with a GitHub URL,
because the guide is not published with the site.

## Writing style (site and guide)

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
- Links between site pages are relative paths between **source** files, in any section of the
  same language (`../concepts/scenes.md#entry-scenes`, `../../std/screen.md`). They work in
  editors and on GitHub; at build time `remarkBookLinks` (`docs/site/src/markdown.ts`) turns them
  into routes (`/reference/concepts/scenes/#entry-scenes`) and fails the build on a missing page.
  Link to an anchor only when the heading exists. There are no language-switch links: the site
  header has a language selector.
- The Develop Guide links to the site with absolute URLs of the English pages
  (`https://kagari.suitsu31.club/en/reference/concepts/scenes/#termination`);
  `docs/check-books.py` maps each one back to its source file and checks the page and anchor.

## Site pages and bilingual rules

- **Page shape.** Pages are plain Markdown (`.md`, not MDX). The title is the frontmatter `title`
  (a page has no `#` heading); a `sidebar.label` overrides the sidebar text when it should be
  shorter. Sections start at `##`.
  ```markdown
  ---
  title: "std::screen"
  ---
  ```
- **Navigation.** A new page gets an entry in `docsNav` of `docs/site/theme.config.json`, which
  orders the sidebar of each section. Entries are paths without the locale and `.md`
  (`reference/concepts/scenes`; `reference/index.md` is `reference`), shared by both languages.
  Labels of sections and groups are Japanese with an `en` translation.
- **Mirror.** `ja/` mirrors `en/` exactly: the same files, the same heading structure, the same
  code blocks. A change to an English page changes its Japanese page in the same commit, and the
  reverse.
- **Shared anchors.** Every heading of a `ja` file carries an explicit id equal to the English
  file's anchor: `## エントリーシーン {#entry-scenes}`. The automatic anchor is mdBook's: the
  heading text in lowercase, spaces replaced by `-`, punctuation such as backticks, `:`, `.` and
  `(` removed: ``## `std::screen` `` → `stdscreen`, ``## Rust integers, `Rat` and `Decimal` `` →
  `rust-integers-rat-and-decimal`. `remarkBookHeadingIds` (`docs/site/src/markdown.ts`) gives the
  built pages exactly these ids, overriding Lotus's own slugs, which drop inline code. When the
  automatic English anchor would be ugly or ambiguous, give the English heading an explicit id
  too, and use the same id in `ja`. Links are then identical in both languages.
- **Identical code blocks.** Code blocks are byte-identical between `en` and `ja`, including
  comments and string contents.
- **Japanese style.** です・ます調; full-width `、。`; a half-width space between Japanese text and
  inline code or ASCII words (`` `Screen` 型 ``, `Rust の`). Identifiers, keywords, attribute names
  and diagnostic codes stay in ASCII code spans. Write natural Japanese, not a word-for-word
  translation.
- **Diagnostics.** Every diagnostic code has exactly one `## E1234 …` (or `## W…`) heading in
  `docs/site/src/content/docs/en/diagnostics/*.md` and one in the `ja` mirror. The
  `diagnostics_book` test in `kagari-lang` enforces it, so adding, removing or renumbering a code
  without updating both languages fails `cargo test`.

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
| step ID | ステップ ID | analysis cache | 解析キャッシュ |
| debug build / release build | デバッグビルド / リリースビルド | query engine | クエリエンジン |
| debug adapter | デバッグアダプター | debugger | デバッガー |
| breakpoint (step / instruction) | ブレークポイント（ステップ / 命令） | resize / box (of a layer) | リサイズ / 矩形 |

A new term that recurs gets a row here and in the Reference's glossary appendix in the same change.

## Diagrams

- Draw diagrams as standalone SVG files, named after the chapter (`architecture-overview.svg`):
  in the Develop Guide in `docs/dev/src/diagrams/`, on the site in a `diagrams/` directory of the
  section and language (`docs/site/src/content/docs/{en,ja}/reference/diagrams/`,
  `…/std/diagrams/`). `ja` diagrams carry Japanese labels.
- Include them inline with `\{{#include ../diagrams/NAME.svg}}` on a line of its own (the path is
  relative to the including file). mdBook expands it for the guide; on the site
  `remarkIncludeSvg` (`docs/site/src/markdown.ts`) inlines the file in a `<figure
  class="kg-diagram">` and fails the build on a missing file.
- Style: a `viewBox`, `width="100%"`, `style="max-width:…px"`, `role="img"` and an `aria-label`
  that states what the picture says. An embedded `<style>` uses only mdBook theme variables with
  fallbacks (`var(--fg,#333)`, `var(--quote-bg,#f6f7f6)`, `var(--links,#2f5d8a)`), and every class
  has the `kg-` prefix; on the site `.kg-diagram` (`docs/site/src/styles/site.css`) maps the
  three variables to the Lotus theme, so diagrams follow light and dark mode. Marker and `<defs>`
  ids are unique per diagram (`kg-arrow-<name>`), because all inline SVGs of a page share one id
  namespace. No scripts, no external references, no raster images.
  [`layout-crates.svg`](../diagrams/layout-crates.svg) is an example.
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
---
title: "std::screen"
---

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
  change: tests, site pages en+ja, Diagnostics pages, tree-sitter and TextMate grammars, formatter,
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

The site needs Node.js 22.23.4 or later (Astro's minimum; `engines` in `docs/site/package.json`)
and pnpm, nothing else: no Rust toolchain, no C compiler. Everything the build reads is inside
`docs/site`.

```sh
cd docs/site
pnpm install        # once, and after pnpm-lock.yaml changes
pnpm run dev        # live preview at http://localhost:4321
pnpm run build      # static site in docs/site/dist
```

The build fails on a link to a missing page or a missing diagram. Astro caches rendered pages in
`docs/site/node_modules/.astro`; after changing `astro.config.ts`, `src/markdown.ts` or the
Kagari grammar, delete that directory (and `docs/site/.astro`) so every page is rendered again.

The Develop Guide is built with `mdbook build docs/dev` into `docs/_site/dev/src/`. The build runs
the `mdbook-kagari` preprocessor through Cargo, so it needs the Rust toolchain and a C compiler
(the normal dev shell); the first build compiles the preprocessor. `mdbook build` fails on a
missing `\{{#include}}` file and reports `SUMMARY.md` entries without a file, but it does not
check links.

Before committing a docs change, run:

```sh
python3 docs/check-books.py                         # links, anchors, en/ja mirror, diagrams
cargo test -p kagari-lang --test diagnostics_book   # every diagnostic code once per language
```

`docs/check-books.py` needs only Python 3. On the site it checks that every page has a `title`
and is listed in `docsNav`, and every `docsNav` entry exists in both languages; that relative
links and `#anchors` resolve (anchors computed the way mdBook computes them, plus explicit
`{#id}` and inline `<a id>`); that `ja` mirrors `en` (same files, same heading ids,
byte-identical code blocks, an explicit `{#id}` on every `ja` heading); and that every included
diagram exists and has its mermaid-ish comment. In the Develop Guide it checks `SUMMARY.md`,
relative links and diagrams the same way, and that every link to `https://kagari.suitsu31.club`
names an existing page and anchor. It exits with status 1 and lists the problems otherwise.

**Deployment.** The Vercel project has Root Directory `docs/site` and the Astro framework
preset; `docs/site/vercel.json` pins the commands (`pnpm install --frozen-lockfile`,
`pnpm run build`, output `dist`). Vercel picks the pnpm version from `pnpm-lock.yaml`
(`lockfileVersion: '9.0'`, pnpm 10). Its `ignoreCommand` skips a deployment when nothing under
`docs/site` changed since the last successful deployment of the branch
(`git diff --quiet "$VERCEL_GIT_PREVIOUS_SHA" HEAD -- .`, run in the root directory). Any other
outcome exits 1 and builds: the first deployment of a branch, and a previous commit outside
Vercel's shallow clone (depth 10; `git diff` would fail with exit 128, which Vercel treats as a
failed deployment, not as "build"). Because only `docs/site` is diffed, the site imports nothing
from outside it. The Node.js version in the project
settings must satisfy `engines`. The domain is `kagari.suitsu31.club` (`site` in
`astro.config.ts`, used for canonical URLs).

## Code highlighting

Kagari code (`kis` and `kix` blocks) is colored by two highlighters that follow the same capture
list, Zed's `tooling/zed-kagari/languages/kagari/highlights.scm`.

### The site: a TextMate grammar

The site renders code with Expressive Code, which uses Shiki, which reads TextMate grammars.
The canonical grammar is `tooling/textmate-kagari/kagari.tmLanguage.json` (`name` `kis`, alias
`kix`, scope `source.kagari`); it follows the language and changes with the tree-sitter grammar.
The site uses its own copy, `docs/site/grammars/kagari.tmLanguage.json`, registered in
`docs/site/astro.config.ts` (`expressiveCode.shiki.langs`) and passed to the landing page's code
block directly. The copy may lag behind the canonical grammar, because the site may document an
older version of the language; copy the grammar over when the site's pages start using new
syntax. One grammar covers both file kinds: `.kix` adds forms, which are built from ordinary
tokens.

The grammar is regular expressions, not a parser, so it approximates the tree-sitter grammar:

| `highlights.scm` capture | TextMate scope |
| ------------------------ | -------------- |
| `keyword` | `keyword.control` (control flow), `keyword.other.*` (`let`, `use`, `as`, `impl`, `crate`, `super`), `storage.type.*` (`fn`, `struct`, `enum`, `trait`, `type`, `mod`), `storage.modifier.*` (`pub`, `mut`, `const`, `static`, `pushdown` before `fn`) |
| `type`, `type.builtin` | `entity.name.type` (capitalized names), `support.type.primitive` (integers, `bool`), `support.type.self` |
| `variable.special` (`self`) | `variable.language.self` |
| `constant`, `boolean` | `constant.other.caps` (`ALL_CAPS`), `constant.language.boolean` |
| `function`, `function.special` (form names) | `entity.name.function`, `entity.name.function.macro` |
| `module` | `entity.name.namespace` |
| `property`, `variant`, `label` | `variable.other.property`, `variable.other.enummember`, `entity.name.label` |
| `attribute` | `meta.attribute` with `entity.other.attribute-name` |
| `comment`, `comment.doc` | `comment.line.double-slash`, `comment.block`, `comment.line.documentation` |
| `string`, `string.escape` | `string.quoted.double`, `constant.character.escape` |
| `number` | `constant.numeric.*` |

Known approximations: a capitalized name followed by `{` is taken for a struct literal; a
capitalized call (`Some(x)`) is colored as a function, as in Zed; `<` and `>` are generic brackets
when unspaced and comparisons when spaced. Inside `t![…]` (`meta.form.text`) the grammar also
marks text-effect `!`, ruby bases and `{…}` interpolation.

### The Develop Guide: `mdbook-kagari`

mdBook's bundled highlight.js knows no Kagari, so `docs/dev/book.toml` registers a preprocessor:

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
  `include_str!`. The guide and Zed therefore color the same nodes.
- **Zed's precedence.** When captures overlap, the innermost node wins; for the same node the
  later pattern (or the later capture of one pattern) wins. The `tree-sitter-highlight` crate
  lets the first pattern win, so the preprocessor resolves captures itself with a `QueryCursor`.
- **Snippets.** Guide code is often a bare statement, an expression or a bodyless std signature.
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

- A new site section or page: `docsNav` in `docs/site/theme.config.json` (and the header links
  in `siteNav` for a section), both languages; a new book-like part also updates the
  [introduction](../introduction.md), the root `README.md` and the `kagari-docs` skill.
- A change to these rules: the `kagari-docs` skill in the same change, and `docs/check-books.py`
  when a rule it enforces changes.
- A new token or syntax: the canonical TextMate grammar (`tooling/textmate-kagari`) together with
  the tree-sitter grammar; the site's copy (`docs/site/grammars/`) when the site documents it.
- A new capture name in `highlights.scm`: an arm in `class_of`
  (`tooling/mdbook-kagari/src/highlight.rs`) and a row in the class table above, and a scope in
  the TextMate grammar with a row in its table.
