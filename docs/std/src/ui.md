# UI (`std/ui.kix`)

System UI is markup inside the modes of `<system>`: buttons, sliders, text, layout containers.
`std/ui.kix` defines their types and styles. The elements and their attributes are in the syntax
book's *System Script* chapter; this chapter gives their semantics.

## htmx syntax, Elm semantics

Interaction is declared on elements as attributes, in the style of htmx, such as
`on-click="interface__call(save)"` or `bind="bgm_volume"`. Every handler attribute holds one call,
either of an [operation](./modes.md#operations) or of a system function (`<sysfn>`). System
functions are finite and loop-free, and they cannot recurse, so each handler runs a finite set of
operations. So the set of transitions is still **finite and known at compile time**, and the
compiler builds the mode graph straight from the markup and the system functions. The
[system checks](./checks.md) run on that graph.

What is *not* taken from htmx is its core mechanism, swapping fragments into targets
(`hx-target`, `hx-swap`). With it, the UI state is whatever the tree became after the swaps, which
nothing can analyze. Instead, as in Elm, a view is a pure function

```text
view : (frame_local, settings, meta, &Narrative) -> Ui
```

re-derived after every operation.

A `bind` attribute reads a setting and writes it, like a `<set>` in a system function, so it is
one more transition, not a second channel.

## Views are full

A system view is computed from scratch, so every system render is a keyframe in the syntax book's
terms. System UI has no history to replay, so it does not use `Screen -> Screen` deltas and needs
no consistency analysis.

## Repetition

The UI repeats over finite collections only: the save slots (their number is fixed by
`<save-slots count>`), the [history](./backlog.md), and the arms offered at the current
`<choice>`. So a view is always a finite tree.

## Focus and input

Keyboard and controller input reach the system only through the `on-keyboard` and `on-controller`
handlers of the top frame's mode. Each names a system function, and the engine passes the input
as its positional arguments. There are no event objects. Without a handler, that input is
ignored; the system does not force controller support.

| Handler         | Arguments | Fires |
| --------------- | --------- | ----- |
| `on-keyboard`   | `Key`     | on every key press, including auto-repeat |
| `on-controller` | `vec2 PadButton` | on a button press with `0 0` and the button; on a d-pad press with a unit vector and `None`; when a stick leaves its dead zone, with the components scaled to −100..=100 and `None` |

In a `vec2`, x grows to the right and y grows downward. The enums are declared in `std/input.kis`,
which is included in the system scope automatically, with reprs consecutive from 0:

- `Key`: `Enter`, `Escape`, `Space`, `Backspace`, `Tab`, `Up`, `Down`, `Left`, `Right`, `PageUp`,
  `PageDown`, `Home`, `End`, `Shift`, `Control`, `Alt`, `F1`…`F12`, `A`…`Z`, `Digit0`…`Digit9`.
- `PadButton`: `None`, `South`, `East`, `West`, `North`, `LeftShoulder`, `RightShoulder`,
  `LeftTrigger`, `RightTrigger`, `Start`, `Select`, `LeftStick`, `RightStick`.

The handler turns input into focus operations. The focusable elements are the `<button>`,
`<slider>`, `<toggle>` and `<select>` of the top frame's mode, in document order with repetitions
expanded; hidden voice buttons are skipped. Focus belongs to the frame: a new frame starts with
nothing focused.

- `interface__focus_move(N)` focuses the Nth focusable. An index out of range does nothing.
- `interface__focus_move(next)` and `interface__focus_move(prev)` wrap around.
- With nothing focused, `next`, a direction and `N` start from index 0, and `prev` focuses the
  last focusable.
- `interface__focus_move` with a `vec2` direction focuses the nearest focusable whose center lies
  within 45° of the vector from the focused element's center. If there is none, the focus is
  unchanged. The vector `0 0` does nothing.
- `interface__focus_activate()` runs the `on-click` of a focused button or text frame and flips the
  setting of a focused toggle. On anything else it does nothing.

See [Styling](./styling.md) for how elements look and how they are laid out.
