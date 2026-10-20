# Program Structure

## `<system>` and `<game>`

*Tentative.* The root `<kagari>` element has exactly two children: one `<system>` and one
`<game>`.

```xml
<kagari>
  <system start="title">
    <import src="std/ui.kix" />
    <import src="std/system-setting.kix" />
    <import src="std/narrative.kix" />
    <!-- settings, meta, save slots, modes -->
  </system>

  <game>
    <import src="std/system-ctx.kix" />
    <import src="characters.kix" />
    <entry name="main" scene="main" exit="ending" />
    <!-- everything the syntax book allows under <kagari>: enums, variables, templates, scenes -->
  </game>
</kagari>
```

| Element    | Contains                                                           | See |
| ---------- | ------------------------------------------------------------------ | --- |
| `<system>` | settings, meta fields, save slots, modes and their UI              | [Modes](./modes.md), [Settings and Meta](./settings-meta.md), [UI](./ui.md) |
| `<game>`   | the story, exactly as the syntax book describes the content of `<kagari>` | the syntax book |

`<system start="…">` names the root page the system starts on.

`<game>` is a **module boundary**, like `body` in HTML but stronger: names declared inside it are
invisible to the system, except through [`std/narrative.kix`](./narrative.md) and what
`<entry>` exports. In the other direction, game code sees nothing of the system except
[`std/system-ctx.kix`](./system-ctx.md).

## `<entry>`: export a way to start the story

```xml
<entry name="main" scene="main" exit="ending" />
<entry name="after_story" scene="after_story" exit="ending" />
```

*Tentative.* An entry is a named starting scene that the system can start with
[`start(name)`](./modes.md#operations).

| Attribute | Required | Meaning                                                                 |
| --------- | -------- | ----------------------------------------------------------------------- |
| `name`    | yes      | Name used by `start(…)` in the system                                   |
| `scene`   | yes      | The scene the narrative starts in                                       |
| `exit`    | no       | A global enum variable the system reads when the entry scene returns    |

A scene reports how it ended by setting a global enum before returning (syntax book, *Scenes and
Control Flow*). `exit` exports that variable, and only that variable, to the `Game` mode's
[`<on-exit>`](./modes.md#game-mode) handler.

A program has one `<game>`. The save format is keyed to one narrative program (its script hash and
choice ids), so extra chapters and after stories are further `<entry>` elements of the same
`<game>`, and their saves share one format.

## Scoped `<include>`

`<include>` stays textual splicing, so writers keep flat names, but every file now has a scope,
given by its root element:

| Root element      | May be included inside |
| ----------------- | ---------------------- |
| `<kagari-game>`   | `<game>`               |
| `<kagari-system>` | `<system>`             |

```text
include : File<s> → Splice<s>      -- legal only inside an element of scope s
```

Including a file into the wrong scope is an error in compilation stage 1. Includes inside an
included file must match the same scope.

## Scoped `<import>`

*This amends the syntax book*, where imports are hoisted to the whole program. Imports are
hoisted **per scope**:

- An `<import>` inside `<game>`, or inside a file included there, is visible to all render code of
  the game.
- An `<import>` inside `<system>` is visible to all system code.
- Neither sees the other's imports. Importing the same `.kix` file in both scopes gives each scope
  its own view of it, which is fine for files of plain data such as `characters.kix`.

The standard files are restricted to one scope each: `std/ui.kix`, `std/system-setting.kix` and
`std/narrative.kix` may only be imported in `<system>`, and `std/system-ctx.kix` only in
`<game>`. This is what makes the isolation in [The System Context](./system-ctx.md) enforceable.
