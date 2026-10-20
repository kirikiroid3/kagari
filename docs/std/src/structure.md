# Program Structure

A program is split into two scopes: `<system>` for the game system and `<game>` for the story. The
syntax of `<system>`, `<game>`, `<entry>` and of scoped `<include>` and `<import>` is in the syntax
book (*Game Script*, *Scenes and Control Flow*, *Templates, Include and Import* and *System
Script*). This chapter explains what the split guarantees.

## `<game>` is a module boundary

`<game>` is a module boundary, like `body` in HTML but stronger: names declared inside it are
invisible to the system, except through [`std/narrative.kix`](./narrative.md) and what `<entry>`
exports. In the other direction, game code sees nothing of the system except
[`std/system-ctx.kix`](./system-ctx.md).

An `<entry>` exports a starting scene, which the system starts with
[`start(name)`](./modes.md#operations). Its `exit` attribute names an enum variable of the story.
When the entry scene returns, the value of that variable, and nothing else, is passed to the `Game`
mode's [`<on-exit>`](./modes.md#game-mode) handler. A scene reports how it ended by setting that
variable before returning.

A program has one `<game>`. The save format is keyed to one narrative program (its script hash and
choice ids), so extra chapters and after stories are further `<entry>` elements of the same
`<game>`, and their saves share one format.

## Scoped include and import

Every included file has a scope, given by its root element, and may only be spliced into that
scope:

```text
include : File<s> → Splice<s>      -- legal only inside an element of scope s
```

Imports are hoisted per scope, so neither scope sees the other's imports. The standard files are
restricted to one scope each (see the [Introduction](./introduction.md)): `std/ui.kix`,
`std/system-setting.kix` and `std/narrative.kix` may only be imported in `<system>`, and
`std/system-ctx.kix` only in `<game>`. The [prelude](./prelude.md) is visible in `<game>` without
an import. This is what makes the isolation in [The System Context](./system-ctx.md)
enforceable.
