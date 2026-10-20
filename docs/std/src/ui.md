# UI (`std/ui.kix`)

System UI is markup inside the modes of `<system>`: buttons, sliders, text, layout containers.
`std/ui.kix` defines their types and styles.

## htmx syntax, Elm semantics

Interaction is declared on elements as attributes, in the style of htmx:

```xml
<button on-click="push(save)">Save</button>
<slider bind="bgm_volume" />
```

Every interaction attribute names one of the [operations](./modes.md#operations). So the set of
transitions is **finite and known at compile time**, and the compiler builds the mode graph
straight from the markup. The [system checks](./checks.md) run on that graph.

What is *not* taken from htmx is its core mechanism, swapping fragments into targets
(`hx-target`, `hx-swap`). With it, the UI state is whatever the tree became after the swaps, which
nothing can analyze. Instead, as in Elm, a view is a pure function

```text
view : (frame_local, settings, meta, &Narrative) -> Ui
```

re-derived after every operation.

| Attribute  | On                         | Meaning |
| ---------- | -------------------------- | ------- |
| `on-click` | any interactive element    | runs an operation |
| `bind`     | `<slider>`, `<toggle>`, `<select>` | reads a setting and writes it with `set` |

*Tentative* names.

## Views are full

A system view is computed from scratch, so every system render is a keyframe in the syntax book's
terms. System UI has no history to replay, so it does not use `Screen -> Screen` deltas and needs
no consistency analysis.

## Repetition

The UI repeats over finite collections only:

| Element         | Repeats over                        |
| --------------- | ----------------------------------- |
| `<for-slots>`   | the save slots, `<save-slots count>` |
| `<for-history>` | the [history](./backlog.md)         |
| `<for-choices>` | the arms offered at the current `<choice>` |

See [Styling](./styling.md) for how elements look and how they are laid out.
