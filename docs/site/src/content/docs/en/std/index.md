---
title: "About This Reference"
---

The *STD Reference* documents `std`, the standard library of Kagari. It lists every public item
of the library with its exact signature and its semantics, one page per module, in the style of
the Node.js API documentation.

The language itself (syntax, function kinds, scenes, forms, numbers) is described in the
[Language Reference](../reference/index.md). This reference assumes it.

## Page structure

Every module page has the same shape:

1. The title is the module path, for example `` `std::time` ``.
2. A **Stability** line summarizes the module and gives its stability (see below).
3. Intro paragraphs say what the module is for and which function kinds its items have.
4. An optional **Overview** table lists the items of the page.
5. One H2 section per item, in the order of the module's source file. The heading names the item
   kind: `Struct:`, `Enum:`, `Trait:`, `Type:`, `Constant:` or `Function:`.
6. One H3 section per method, associated function or group of variants, below its type.

Each item section starts with the declaration exactly as `std` declares it, followed by:

- one bullet per parameter: name, type, meaning;
- `Returns:` the result type and its meaning;
- a badge line: `Kind: … · Builtin: … · Profile: …` (see below);
- semantics, edge cases, related diagnostics and a short example.

The guides (*Staging Recipes*, *System Recipes*) show how the items combine. The appendices list
the compiler rules that apply to std items and what each runtime profile supports.

## Stability index

|Stability|Meaning|
|---|---|
|**Stable**|Implemented by the compiler and the runtime. The signature and the semantics change only with a new toolchain version that says so.|
|**Tentative**|Implemented, but the design is not settled. Expect changes to names, signatures or semantics.|
|**Planned**|Designed or wished for, but not implemented. The page describes the intent; nothing of it can be used yet.|

A page states the stability of its module in the first line. An item that differs from its module
carries its own `Stability:` note.

## Kind badges

The badge line of an item names its function kind:

|Badge|Meaning|
|---|---|
|`Kind: const fn`|Pure. Callable from every kind of code; in the story, calls with constant arguments are folded and calls with finite runtime arguments are tabulated.|
|`Kind: fn`|Ordinary code: the game system, UI and settings. Not callable from `const fn` or the story.|
|`Kind: pushdown fn`|Story code. Callable only from the story; some items only from a scene.|

The call rules between kinds are those of the language; see
[Function Kinds](../reference/concepts/kinds.md). They apply to std methods and
builtins exactly as to your own functions.

## `#[builtin]` items

An item marked `#[builtin]` has no body in `std`: the compiler or the engine provides it.

- A `#[builtin]` **type** is opaque. It has no visible fields and no layout you can rely on; you use
  it only through its methods and the functions that take it. `Text`, `Screen`, `Narrative` and the
  asset types are examples.
- A `#[builtin]` **function** has the declared signature and kind like any other function.
- `#[builtin]` is reserved for `std`. A game cannot declare builtins.

Everything not marked `#[builtin]` is ordinary Kagari code that a studio could have written
itself, for example `std::time` and the text-effect constructors.

The badge says `Builtin: yes` or `Builtin: no`.

## Signature conventions

- Signatures are copied from the `std` sources. Doc comments are shortened or omitted.
- Methods are headed `` `Type::method(args)` `` and show `self`, `&self` or `&mut self` as declared.
- Generic parameters are written as in Rust: `<T>`, `<T: Eq>`, `<const N: usize>`.
- `Int<lo, hi>` is the language's bounded integer type; the Rust integers (`u8` … `i128`, `usize`,
  `isize`) and `bool` are language types, not std items.
- Positions and lengths are pixels, times are `Duration`, fractions are `Decimal`.
- "Evaluation error" means: a compile error when the call is const-evaluated, a runtime error
  otherwise.

## Profile notes

A runtime profile is the set of features and asset formats a runtime provides. When an item is not
supported by a profile, its badge line says so, for example `Profile: not in native_v0`. The full
matrix is in [Runtime Profiles](./appendix/profiles.md).
