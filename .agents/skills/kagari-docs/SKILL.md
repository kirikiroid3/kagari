---
name: kagari-docs
description: Load when editing anything under docs/ (the documentation site docs/site — Tutorial, Language Reference, STD Reference, Diagnostics — or the Develop Guide docs/dev), README.md or .agents/skills, or when a code change alters user-visible behaviour (syntax, diagnostics, std items, attributes, CLI flags, kagari.toml keys, runtime options, asset formats) or Kagari internals that the Develop Guide describes.
---

# Kagari documentation

Depth: `docs/dev/src/process/docs.md` (the full policy). Read it before a larger docs change.

## Scope

| Part | Path | Languages | Audience |
| ---- | ---- | --------- | -------- |
| Tutorial | `docs/site/src/content/docs/{ja,en}/tutorial` | ja + en | newcomers |
| Language Reference | `docs/site/src/content/docs/{ja,en}/reference` | ja + en | studios |
| STD Reference | `docs/site/src/content/docs/{ja,en}/std` | ja + en | studios |
| Diagnostics | `docs/site/src/content/docs/{ja,en}/diagnostics` | ja + en | studios |
| Kagari Develop Guide | `docs/dev/src` (mdBook) | en | maintainers |

The site (`docs/site`) is Astro with the Lotus theme, published at `https://kagari.suitsu31.club`
(ja at `/`, en at `/en/`). Pages are plain `.md` with a frontmatter `title` and no `#` heading.
`docsNav` in `docs/site/theme.config.json` lists every page in sidebar order (entries are paths
without locale and `.md`; `x/index.md` is `x`); add new pages there or `check-books.py` fails.
The landing page is `docs/site/src/components/Home.astro`. The Develop Guide is a standalone
mdBook: `docs/dev/src/SUMMARY.md` lists every page.

## Always

- Same change, same commit: code + Develop Guide chapter + (if a studio can observe it) the site
  page in **en and ja**.
- The code is the truth. Verify every name, signature, flag, default and diagnostic code against
  the source before writing it: std in `core/kagari-std/kis/std/**/*.kis`, diagnostic codes in
  `core/kagari-lang/src/diagnostic.rs`, CLI in `tooling/kagari-cli/src/cli.rs`, manifest in
  `tooling/kagari-project`, runtime flags in `runtime/kagari-desktop-runtime/src/cli.rs`.
- Site pages: no crate names, internal paths, IR names (HIR/THIR/MIR) or opcodes. They link to the
  Develop Guide (a GitHub URL) only from the section introductions.
- Links between site pages: relative `.md` source paths (`../concepts/scenes.md#entry-scenes`,
  `../../std/screen.md`); the build turns them into routes. The Develop Guide links to the site
  with English URLs (`https://kagari.suitsu31.club/en/reference/concepts/scenes/#termination`).
- Develop Guide chapters: start with **Where in the code**; end with **Invariants** and
  **When you change this** where useful. Decisions are records (Decision / Why / Consequences,
  with a status) in `docs/dev/src/foundations/decisions.md`. Answered questions leave
  `docs/dev/src/process/open-questions.md`.

## Bilingual rules (site)

1. `ja/` mirrors `en/`: same files, same heading structure.
2. Every `ja` heading has an explicit id equal to the English anchor:
   `## エントリーシーン {#entry-scenes}`. English auto anchor (mdBook's, reproduced by
   `docs/site/src/markdown.ts`) = lowercase, spaces → `-`, punctuation (backticks, `:`, `.`, `(`)
   dropped: ``## `std::screen` `` → `stdscreen`. If the English anchor is ugly, give the English
   heading an explicit id and reuse it in ja.
3. Code blocks are byte-identical in en and ja (comments and strings too).
4. Japanese: です・ます調, `、。`, a half-width space between Japanese and inline code/ASCII
   (`` `Screen` 型 ``). Identifiers, keywords, attributes and codes stay in code spans.
5. Terms: use the glossary table in `docs/dev/src/process/docs.md#glossary` (proposal-jp is the
   tie-breaker). A new recurring term gets a row there and in the Language Reference glossary.

## Diagnostics

Adding, removing or renumbering a `DiagnosticCode` requires exactly one `## E1234 name` heading
per language in `docs/site/src/content/docs/{en,ja}/diagnostics/*.md`.
`cargo test -p kagari-lang --test diagnostics_book` enforces it.

## STD pages

Node.js-API style: frontmatter `title: "std::x"`, a `> **Stability:** …` line (ja:
`> **安定性:** 安定 / 暫定 / 予定`), intro, optional `## Overview` table, then
`## Struct: \`T\`` / `Enum:` / `Trait:` / `Type:` / `Constant:` / `Function: f(args)` with the
declaration in a `kis` block, `### \`T::m(args)\`` per method with parameters, Returns,
Kind · Builtin · Profile. Every public item of the module's `.kis` file appears, in declared
order. User-visible compiler rules for std items go in `std/appendix/rules.md`.

## Diagrams

- SVG named `<chapter>-<what>.svg` in `docs/dev/src/diagrams/` or, on the site, in the section's
  `diagrams/` per language (`…/{en,ja}/reference/diagrams/`, `…/{en,ja}/std/diagrams/`),
  included on a line of its own with `{{#include ../diagrams/NAME.svg}}`.
- `viewBox`, `width="100%"`, `style="max-width:…px"`, `role="img"`, `aria-label`; `<style>` with
  `kg-` classes using only `var(--fg,#333)`, `var(--quote-bg,#f6f7f6)`, `var(--links,#2f5d8a)`;
  unique marker ids `kg-arrow-<name>`; no scripts, external refs or rasters.
- Directly before the include, a hidden comment `<!-- diagram: NAME (mermaid-ish) … -->` that
  states the same graph. Never write `-->` inside it; use `==>` or `-.->`. Change SVG and comment
  together.

## Workflow

1. Find every page that states the fact you change: search `docs/` in all parts and both
   languages (and code comments citing docs paths).
2. Edit the English page, then the Japanese mirror (same headings, ids, code blocks).
3. Update the Develop Guide chapter for internals, and decisions/open questions if affected.
4. If a Kagari example changed, make sure it compiles against current std (ideally it also lives
   in a fixture such as `core/kagari-lang/tests/game_source/script/001__book_examples`).
5. Build and check (below).

## Commands

```sh
python3 docs/check-books.py                         # links, anchors, docsNav, en/ja mirror, diagrams
cargo test -p kagari-lang --test diagnostics_book
(cd docs/site && pnpm install && pnpm run build)    # Node.js >= 22.23.4; fails on broken links
mdbook build docs/dev                               # runs mdbook-kagari via cargo
```

Tag every Kagari block `kis` or `kix`: the site highlights them with its copy of the TextMate
grammar, `docs/site/grammars/kagari.tmLanguage.json` (copied from `tooling/textmate-kagari` when
the site starts documenting new syntax; it may lag), the Develop Guide with `mdbook-kagari`.
Everything the site builds from stays inside `docs/site`: Vercel builds from that root and skips
deployments without changes there. After changing
`docs/site/astro.config.ts`, `docs/site/src/markdown.ts` or the grammar, delete
`docs/site/node_modules/.astro` before building (Astro caches rendered pages). Details:
`docs/dev/src/process/docs.md#building-and-checking` and `#code-highlighting`.
