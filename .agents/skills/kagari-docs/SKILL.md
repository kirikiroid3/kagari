---
name: kagari-docs
description: Load when editing anything under docs/ (Reference, STD Reference, Develop Guide), README.md or .agents/skills, or when a code change alters user-visible behaviour (syntax, diagnostics, std items, attributes, CLI flags, kagari.toml keys, runtime options, asset formats) or Kagari internals that the Develop Guide describes.
---

# Kagari documentation

Depth: `docs/dev/src/process/docs.md` (the full policy). Read it before a larger docs change.

## Scope

| Book | Path | Languages | Audience |
| ---- | ---- | --------- | -------- |
| The Kagari Reference | `docs/reference/{en,ja}/src` | en + ja | studios |
| Kagari STD Reference | `docs/std/{en,ja}/src` | en + ja | studios |
| Kagari Develop Guide | `docs/dev/src` | en | maintainers |

Each is a standalone mdBook. `SUMMARY.md` lists every page; add a page there (in en **and** ja)
or it is not built. `book.toml` `build-dir` mirrors the source tree under `docs/_site/`; keep that
when adding a book.

## Always

- Same change, same commit: code + Develop Guide chapter + (if a studio can observe it) the
  Reference or STD Reference page in **en and ja**.
- The code is the truth. Verify every name, signature, flag, default and diagnostic code against
  the source before writing it: std in `core/kagari-std/kis/std/**/*.kis`, diagnostic codes in
  `core/kagari-lang/src/diagnostic.rs`, CLI in `tooling/kagari-cli/src/cli.rs`, manifest in
  `tooling/kagari-project`, runtime flags in `runtime/kagari-desktop-runtime/src/cli.rs`.
- Reference/STD pages: no crate names, internal paths, IR names (HIR/THIR/MIR) or opcodes. They
  link to the Develop Guide only from their introduction.
- Develop Guide chapters: start with **Where in the code**; end with **Invariants** and
  **When you change this** where useful. Decisions are records (Decision / Why / Consequences,
  with a status) in `docs/dev/src/foundations/decisions.md`. Answered questions leave
  `docs/dev/src/process/open-questions.md`.

## Bilingual rules (Reference and STD)

1. `ja/src` mirrors `en/src`: same files, same SUMMARY order, same heading structure.
2. Every `ja` heading has an explicit id equal to the English anchor:
   `## エントリーシーン {#entry-scenes}`. English auto anchor = lowercase, spaces → `-`,
   punctuation (backticks, `:`, `.`, `(`) dropped: ``## `std::screen` `` → `stdscreen`. If the
   English anchor is ugly, give the English heading an explicit id and reuse it in ja.
3. Code blocks are byte-identical in en and ja (comments and strings too).
4. Japanese: です・ます調, `、。`, a half-width space between Japanese and inline code/ASCII
   (`` `Screen` 型 ``). Identifiers, keywords, attributes and codes stay in code spans.
5. Terms: use the glossary table in `docs/dev/src/process/docs.md#glossary` (proposal-jp is the
   tie-breaker). A new recurring term gets a row there and in the Reference glossary appendix.

## Diagnostics

Adding, removing or renumbering a `DiagnosticCode` requires exactly one `## E1234 name` heading
per language in `docs/reference/{en,ja}/src/diagnostics/*.md`. `cargo test -p kagari-lang --test
diagnostics_book` enforces it.

## STD pages

Node.js-API style: `# \`std::x\``, a `> **Stability:** …` line (ja: `> **安定性:** 安定 / 暫定 /
予定`), intro, optional `## Overview`
table, then `## Struct: \`T\`` / `Enum:` / `Trait:` / `Type:` / `Constant:` / `Function: f(args)`
with the declaration in a `kis` block, `### \`T::m(args)\`` per method with parameters, Returns,
Kind · Builtin · Profile. Every public item of the module's `.kis` file appears, in declared
order. User-visible compiler rules for std items go in `appendix/rules.md`.

## Diagrams

- SVG in the book's `src/diagrams/` (per language for en/ja books), named `<chapter>-<what>.svg`,
  included with `{{#include ../diagrams/NAME.svg}}`.
- `viewBox`, `width="100%"`, `style="max-width:…px"`, `role="img"`, `aria-label`; `<style>` with
  `kg-` classes using only `var(--fg,#333)`, `var(--quote-bg,#f6f7f6)`, `var(--links,#2f5d8a)`;
  unique marker ids `kg-arrow-<name>`; no scripts, external refs or rasters.
- Directly before the include, a hidden comment `<!-- diagram: NAME (mermaid-ish) … -->` that
  states the same graph. Never write `-->` inside it; use `==>` or `-.->`. Change SVG and comment
  together.

## Workflow

1. Find every page that states the fact you change: search `docs/` in all books and both
   languages (and code comments citing docs paths).
2. Edit the English page, then the Japanese mirror (same headings, ids, code blocks).
3. Update the Develop Guide chapter for internals, and decisions/open questions if affected.
4. If a Kagari example changed, make sure it compiles against current std (ideally it also lives
   in a fixture such as `core/kagari-lang/tests/game_source/script/001__book_examples`).
5. Build and check (below).

## Commands

```sh
mdbook build docs/reference/en && mdbook build docs/reference/ja
mdbook build docs/std/en && mdbook build docs/std/ja
mdbook build docs/dev
python3 docs/check-books.py                         # links, anchors, en/ja mirror, diagrams
cargo test -p kagari-lang --test diagnostics_book
```

Each `mdbook build` runs the `mdbook-kagari` preprocessor via `cargo run` (first build compiles
it); it highlights `kis`/`kix` blocks, so tag every Kagari block with one of them. Details:
`docs/dev/src/process/docs.md#code-highlighting`.
