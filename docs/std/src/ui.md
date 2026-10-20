# UI (`std/ui.kix`)

System UI is markup inside the modes of `<system>`: buttons, sliders, text, layout containers.
`std/ui.kix` defines their types and styles. The elements and their attributes are in the syntax
book's *System Script* chapter; this chapter gives their semantics.

## htmx syntax, Elm semantics

Interaction is declared on elements as attributes, in the style of htmx, such as
`on-click="push(save)"` or `bind="bgm_volume"`. Every interaction attribute names one of the
[operations](./modes.md#operations). So the set of transitions is **finite and known at compile
time**, and the compiler builds the mode graph straight from the markup. The
[system checks](./checks.md) run on that graph.

What is *not* taken from htmx is its core mechanism, swapping fragments into targets
(`hx-target`, `hx-swap`). With it, the UI state is whatever the tree became after the swaps, which
nothing can analyze. Instead, as in Elm, a view is a pure function

```text
view : (frame_local, settings, meta, &Narrative) -> Ui
```

re-derived after every operation.

A `bind` attribute reads a setting and writes it with `set`, so it is one more operation, not a
second channel.

## Views are full

A system view is computed from scratch, so every system render is a keyframe in the syntax book's
terms. System UI has no history to replay, so it does not use `Screen -> Screen` deltas and needs
no consistency analysis.

## Repetition

The UI repeats over finite collections only: the save slots (their number is fixed by
`<save-slots count>`), the [history](./backlog.md), and the arms offered at the current
`<choice>`. So a view is always a finite tree.

See [Styling](./styling.md) for how elements look and how they are laid out.
