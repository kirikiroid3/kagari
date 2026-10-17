# Templates, Include and Import

Three mechanisms bring script in from elsewhere. Templates and includes are macros: they are
expanded at compile time and leave nothing behind in the bytecode. Imports bring in `.kix`
declarations.

| Tag           | Expands to                         | Resolved in stage |
| ------------- | ---------------------------------- | ----------------- |
| `<x tpl="…">` | The template body with slots filled | 1 (collect)       |
| `<include>`   | The content of another `.kis` file  | 1 (collect)       |
| `<import>`    | Nothing; declarations become visible | 1 (collect)      |

## `<template>` and `<slot>`: define a reusable block

```xml
<template name="phone_call">
  <s>The phone rings. It's <slot pos="1" />.</s>
  <slot pos="2" />
  <s>The call ends.</s>
</template>
```

A `<template>` is a named block of script with numbered holes. `<slot pos="n" />` marks hole
number `n`, counting from 1.

- `name` is unique in the program. Templates are declared at top level.
- A slot may appear anywhere in the body where its eventual content would be valid: as a whole
  element, or inside text as above.
- The same `pos` may appear more than once; each occurrence receives a copy of the content.
- A template may use other templates, but not itself, directly or indirectly. Expansion must
  terminate.

## `<x>` and `<snip>`: use a template

```xml
<x tpl="phone_call">
  <snip>Hanako</snip>
  <snip>
    <s>"Are you coming to the festival?"</s>
    <s>"I'll be there."</s>
  </snip>
</x>
```

`<x>` is replaced by the body of the template named in `tpl`. The `n`th `<snip>` fills every
`<slot pos="n" />`. The example expands to:

```xml
<s>The phone rings. It's Hanako.</s>
<s>"Are you coming to the festival?"</s>
<s>"I'll be there."</s>
<s>The call ends.</s>
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
- *Tentative:* `<include>` is allowed at top level only, so files contain whole scenes,
  templates and declarations rather than fragments of a scene. Reusable fragments are what
  templates are for.

## `<import>`: use extension declarations

```xml
<import src="characters.kix" />
```

Makes every declaration in a `.kix` file visible to the represent code of the program.

- `src` is relative to the importing file.
- Imports are hoisted: an import anywhere, including in an included file, is visible to all
  represent code in the program, regardless of position. By convention imports go at the top of
  the file.
- Importing the same file more than once is the same as importing it once.
- Two imported files declaring the same name is an error.
