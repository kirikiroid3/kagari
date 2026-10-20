# Templates, Include and Import

Three mechanisms bring script in from elsewhere. Templates and includes are macros: they are
expanded at compile time and leave nothing behind in the bytecode. Imports bring in `.kix`
definitions.

| Tag           | Expands to                         | Resolved in stage |
| ------------- | ---------------------------------- | ----------------- |
| `<x tpl="…">` | The template body with slots filled | 1 (collect)       |
| `<include>`   | The content of another `.kis` file  | 1 (collect)       |
| `<import>`    | Nothing; definitions become visible | 1 (collect)       |

## `<template>` and `<slot>`: define a reusable block

```xml
<template name="phone_call">
  <s><m who="">The phone rings. It's <slot pos="1" />.</m></s>
  <slot pos="2" />
  <s><m who="">The call ends.</m></s>
</template>
```

A `<template>` is a named block of script with numbered holes. `<slot pos="n" />` marks hole
number `n`, counting from 1.

- `name` is unique in the program. Templates are declared at top level.
- A slot may appear anywhere in the body where its eventual content would be valid: as a whole
  element, or inside the text of an `<m>` as above.
- The same `pos` may appear more than once; each occurrence receives a copy of the content.
- A template may use other templates, but not itself, directly or indirectly. Expansion must
  terminate.

## `<x>` and `<snip>`: use a template

```xml
<x tpl="phone_call">
  <snip>Hanako</snip>
  <snip>
    <s><m who="Hanako">Are you coming to the festival?</m></s>
    <s><m who="">"I'll be there."</m></s>
  </snip>
</x>
```

`<x>` is replaced by the body of the template named in `tpl`. The `n`th `<snip>` fills every
`<slot pos="n" />`. The example expands to:

```xml
<s><m who="">The phone rings. It's Hanako.</m></s>
<s><m who="Hanako">Are you coming to the festival?</m></s>
<s><m who="">"I'll be there."</m></s>
<s><m who="">The call ends.</m></s>
```

The number of `<snip>` children must equal the highest `pos` in the template. The expanded result
is checked like hand-written script, in stage 2.

A label inside a template body is duplicated by every use in the same scene, which collides with
the unique-label rule. How to handle that is an
[open question](../open-questions.md#template-hygiene).

## `<include>`: splice another script

```xml
<include src="chapter1.kis" />
```

Replaced by the children of the root element of `chapter1.kis`. Use it to split a game across
files.

- `src` is relative to the including file.
- Includes may nest. An include cycle is an error.
- *Tentative:* `<include>` is allowed only as a direct child of `<game>`, `<system>` or the root
  of an included file, so files contain whole scenes, templates and declarations rather than
  fragments of a scene. Reusable fragments are what templates are for.

The root element of an included file gives its scope, and the file may only be included into an
element of that scope:

| Root element      | May be included inside |
| ----------------- | ---------------------- |
| `<kagari-game>`   | `<game>`               |
| `<kagari-system>` | `<system>`             |

```xml
<!-- chapter1.kis -->
<kagari-game>
  <scene name="chapter1">...</scene>
</kagari-game>
```

Including a file into the wrong scope is an error in compilation stage 1. Includes inside an
included file stay in the same scope.

## `<import>`: use extension definitions

```xml
<import src="characters.kix" />
```

Makes every definition in a `.kix` file visible to the code of the scope it appears in.

- `src` is relative to the importing file. A path starting with `std/` names a file of the
  standard library, which the *Kagari STD Book* describes.
- Imports are hoisted **per scope**. An import inside `<game>`, or inside a file included there,
  is visible to all represent code of the game, regardless of position. An import inside
  `<system>` is visible to all system code. Neither scope sees the other's imports. By convention
  imports go at the top of the file.
- Importing the same `.kix` file in both scopes gives each scope its own view of it.
- Each standard library file belongs to one scope, and importing it into the other is an error.
- The standard prelude is visible in the game scope without an import.
- Importing the same file more than once in a scope is the same as importing it once.
- Two imported files defining the same name is an error.
