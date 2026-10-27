# Quick Reference

## Function kinds

| Kind | Calls | Holds at runtime | Steps | Story variables |
| ---- | ----- | ---------------- | ----- | --------------- |
| `fn` | `fn`, `const fn`; starts entry scenes | anything | no | no |
| `#[scene] pushdown fn` | scenes, instant procedures, `const fn` (constant or finite arguments) | [finite types](./kinds.md#finite-types) | yes | read and write |
| `pushdown fn` (instant procedure) | instant procedures, `const fn`; no recursion, only `for` loops | finite types | no | read and write |
| `const fn` | `const fn` | anything | no | no |

`pushdown` is a keyword only directly before `fn` or `Fn`; `scene` only in the pointer type
`scene fn(…) -> …`.

## Attributes

| Attribute | On | Meaning |
| --------- | -- | ------- |
| `#[scene]` | `pushdown fn` | A scene: may run steps and call scenes; a component of the checked state machine |
| `#[entry]` | `#[scene]` with finite parameters and return type | The system can start the story here |
| `#[must_use]` | `struct`, `enum` | Discarding or leaving a value pending is a warning |
| `#[builtin]` | function, type | Engine-implemented: no body, no visible fields; standard library only |

## Forms (`.kix` only)

| Delimiter | Accepts |
| --------- | ------- |
| `name!( … )` | attributes, then a fixed number of elements |
| `name![ … ]` | elements only, any number |
| `name!{ … }` | attributes, then any number of elements |

Attributes are `key = expr` and come first. Elements are comma-separated, except the juxtaposed
items of `t![…]`.

| Form | Desugars to | Value | Attributes | Elements |
| ---- | ----------- | ----- | ---------- | -------- |
| `s!(…)` | `push_step(Say { … })` | `()` | `who`, `voice`, `render`, `history` | at most one `t![…]` |
| `choice!{…}` | `push_step(Choice { … })` | the arms' enum | `id` (required), `render` | arms `t![…] => V`, `t![…] if cond => V` |
| `t![…]` | `concat([…])` of `ruby`, `mono_ruby`, `effect`, `to_text` calls | `Text` | none | `"…"`, `["基"]("よみ")`, `["漢字"]("かん"\|"じ")`, `!["…"](effect)`, `{expr}` |
| UI forms | calls of `std::ui` functions with props structs | `Element` | see the standard library book, chapter *UI* | see the standard library book, chapter *UI* |

`s!` and `choice!` are allowed only in a scene. UI forms are introduced in [The Game
System](./system.md#components-and-signals).

## Story items

| Item | Form |
| ---- | ---- |
| Story variable | `static mut NAME: FiniteType = constant;` |
| Setting | `static NAME: Setting<T> = setting(init);` |
| Meta field | `static NAME: Meta<T> = meta(init);` |
| Asset | `const NAME: ImageAsset = asset("path");`, likewise `AudioAsset`, `VoiceAsset`, `VideoAsset`, `FontAsset`, `ShaderAsset` |
| System font | `const NAME: FontAsset = system_font("path");` |
| Entity id | `const NAME: EntityId = EntityId(n);`, with `n` ≥ 1 |
| Render | `const fn name(mut screen: Screen) -> Screen { …; screen }` |

## Control flow in the story

| Need | Write |
| ---- | ----- |
| Call a scene | `Name(x)` |
| Leave a scene | `return;` / `return value;` |
| Branch | `if`, `match` (exhaustive, with ranges and tuples) |
| Hub or repeated choice | `loop { match choice!{…} { … } }` with `break` |
| Bounded repetition | `for i in 0..n` |
| Change a counter | `x += 1;` (saturates) |
| Write meta | `META_FIELD.write(v);` |
| Draw at random (scenes only) | `rng_choose::<T>()`, `T` a fieldless enum or `Int<lo, hi>` |
| End the story | return from the entry scene |
