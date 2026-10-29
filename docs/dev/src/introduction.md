# Introduction

The Kagari Develop Guide is for people who change Kagari itself: the compiler, the standard
library, the bytecode, the VM, the engine, the runtimes and the tooling. It records why the system
is built the way it is and how its parts fit together. It assumes you know Rust and the usual
compiler vocabulary (CST, HIR, MIR, salsa queries, decision trees).

## The documentation

| Part | Audience | Holds | Languages |
| ---- | -------- | ----- | --------- |
| [Tutorial](https://kagari.suitsu31.club/en/tutorial/) (site) | People new to Kagari | Installation and a first game | ja, en |
| [Language Reference](https://kagari.suitsu31.club/en/reference/) (site) | Studios writing games | Concepts, the language, special types and attributes, projects, CLI, runtime | ja, en |
| [STD Reference](https://kagari.suitsu31.club/en/std/) (site) | Studios writing games | Every public item of `std`, recipes, compiler rules for std items, runtime profiles | ja, en |
| [Diagnostics](https://kagari.suitsu31.club/en/diagnostics/) (site) | Studios writing games | Every diagnostic code of the compiler | ja, en |
| Kagari Develop Guide (this book) | Maintainers and contributors | Design decisions, proposal mapping, crate layout, compiler internals, the std/host ABI, file formats, VM, engine, tooling internals, process | en |

The site (`https://kagari.suitsu31.club`, sources in `docs/site`) describes *what* a studio can
rely on. This guide describes *why* and *how*. It never repeats the user-facing rules; it links
to them. When this guide and the code disagree, the code is right and this guide has a bug.

## How to read this guide

- [Foundations](./foundations/decisions.md) first: the design decisions and their rationale,
  [the relation to the engine proposal](./foundations/proposal.md), and the
  [repository layout](./foundations/layout.md).
- Then the part you are about to change: [Compiler](./compiler/pipeline.md),
  [Standard Library](./std/builtins.md), [Formats](./formats/kagari.md),
  [Runtime](./runtime/vm.md) or [Tooling](./tooling/cli.md).
- [Process](./process/testing.md) covers testing, [documentation](./process/docs.md) and the
  [open questions](./process/open-questions.md).

Chapters start with **Where in the code** (the crates and files they describe) and end, where
useful, with **Invariants** (what a change must preserve) and **When you change this** (what else
must change with it).

## The rule

> Update this guide in the same change as the code it describes.

A change to the compiler, a format, the VM, the engine or the tooling updates the chapter that
describes it, in the same commit. A change a studio can observe (syntax, a diagnostic, a std item,
a CLI flag, a runtime option, a file format a studio touches) also updates the site (Language
Reference, STD Reference or Diagnostics), in English **and** Japanese, in the same commit.
[Documentation](./process/docs.md) has the details.

## Building the documentation

This guide is an [mdBook](https://rust-lang.github.io/mdBook/):

```sh
mdbook build docs/dev       # into docs/_site/dev/src/ (ignored by git)
mdbook serve docs/dev       # live reload
```

The site is an Astro project in `docs/site` and needs only Node.js:

```sh
cd docs/site && npm ci && npm run dev
```

[Building and checking](./process/docs.md#building-and-checking) has the details and the checks
to run before committing.
