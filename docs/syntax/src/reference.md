# Quick Reference

## Function kinds

| Kind | Calls | Holds at runtime | Steps | Story variables |
| ---- | ----- | ---------------- | ----- | --------------- |
| `fn` | `fn`, `const fn`; starts entry scenes | anything | no | no |
| `pushdown fn` | `pushdown fn`, `const fn` (constant or finite arguments) | [finite types](./kinds.md#finite-types) | yes | read and write |
| `const fn` | `const fn` | anything | no | no |

`pushdown` is a keyword only directly before `fn` or `Fn`.

## Attributes

| Attribute | On | Meaning |
| --------- | -- | ------- |
| `#[scene]` | `pushdown fn` | Usable as a self-closing tag; attributes are parameters by name |
| `#[entry]` | `pushdown fn` with finite parameters and return type | The system can start the story here |
| `#[component]` | `fn` | A UI component, usable as a tag in UI code |
| `#[must_use]` | `struct`, `enum` | Discarding or leaving a value pending is a warning |
| `#[builtin]` | function | Engine-implemented, no body; standard library only |

## Narrative elements

| Element | Desugars to | Value | Props | Children |
| ------- | ----------- | ----- | ----- | -------- |
| `<s>` | `step(Say { … })` | `()` | `render` | text (narration), `<m>` |
| `<m>` | `Message { … }` | — | `who`, `voice` | text |
| `<choice>` | `step(Choice { … })` | the arms' enum | `id` (required), `render` | `<arm>` |
| `<arm>` | `Arm { … }` | — | `id` (required), `when` | text |
| `<Name … />` (`#[scene]`) | `Name(…)` | the scene's return value | the scene's parameters | none |

Only the outermost element of an element expression is a step; nested elements are data.

## Story items

| Item | Form |
| ---- | ---- |
| Story variable | `static mut NAME: FiniteType = constant;` |
| Setting | `static NAME: Setting<T> = setting(init);` |
| Meta field | `static NAME: Meta<T> = meta(init);` |
| Asset | `const NAME: ImageAsset = asset("path");` |
| Render | `const fn name(mut screen: Screen) -> Screen { …; screen }` |

## Control flow in the story

| Need | Write |
| ---- | ----- |
| Call a scene | `<Name a={x} />` or `name(x)` |
| Leave a scene | `return;` / `return value;` |
| Branch | `if`, `match` (exhaustive, with ranges and tuples) |
| Hub or repeated choice | `loop { match <choice …> … }` with `break` |
| Bounded repetition | `for i in 0..n` |
| Change a counter | `x += 1;` (saturates) |
| Write meta | `META_FIELD.write(v);` |
| End the story | return from the entry scene |
