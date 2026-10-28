# `std::system::nav`

> **Stability:** Stable — the operations that change the system's mode stack.

`std::system::nav` holds the navigation operations. UI changes the mode stack only through these
functions and through the methods of the `Narrative` handle. All of them are builtin `fn`s, so
they may be called from any `fn` code (button handlers, effects, exit handlers), but never from a
render or the story.

The prelude imports the module itself, so call the functions as `nav::call(Config)`,
`nav::back()` and so on, without a `use`.

See [Mode types](./system.md#mode-types) for the kinds of modes and where each may sit on the
stack.

## Overview {#overview}

| Function | Effect |
| -------- | ------ |
| [`nav::call(o)`](#call) | Pushes the overlay `o` |
| [`nav::dialog(d)`](#dialog) | Pushes the dialog `d` |
| [`nav::back()`](#back) | Pops the top mode; ignored on a base |
| [`nav::goto(p)`](#goto) | Replaces the whole stack with the root page `p` |
| [`nav::start(e, args, on_exit)`](#start) | Replaces the stack with a new game running the entry scene `e` |
| [`nav::load(slot)`](#load) | Replaces the stack with the game restored from `slot` |
| [`nav::quit()`](#quit) | Quits the program at once |

The operations on a running game are methods of the `Narrative` handle (see
[`std::narrative`](./narrative.md)):

| Operation | Effect |
| --------- | ------ |
| `game.advance()` | Sends `NextStep` to the story |
| `game.select(arm)` | Selects a shown arm of the current choice |
| `game.save(slot)` | Writes the running game to `slot` |
| `game.rewind(i)` | Restores the story to backlog entry `i` |
| `game.call(o)` | Pushes the game overlay `o` |

A handle exists only in the game view and in game overlays, so "needs a running game" is enforced
by types: a Save button on the title page does not type-check.

## Mode graph checks {#mode-graph-checks}

At compile time, the compiler collects the `nav` operations each mode's handlers can run and
builds a graph of the modes. It reports:

| Code | Problem |
| ---- | ------- |
| `W6001` | A mode no reachable mode opens, or a game that is never started |
| `E6002` | `nav::back()` in a mode at the base of the stack |
| `E6003` | A mode with no way out (no `back`, `goto`, `start`, `load` or `quit`, and no `Action::Back` binding) |
| `W6004` | Overlays that open each other with `nav::call`, so the stack can grow without bound |

See the [game system diagnostics](../../../reference/en/src/diagnostics/system.md).

## Function: `call(overlay)` {#call}

```kis
#[builtin] pub fn call(overlay: fn() -> Overlay);
```

- `overlay` `fn() -> Overlay` — the overlay component, as a function pointer.
- Kind: `fn` · Builtin: yes

Pushes the overlay over the current top mode. The overlay's component runs once, when it is
shown.

```kix
button!{on_click = || nav::call(Config), t!["Config"]}
```

## Function: `dialog(dialog)` {#dialog}

```kis
#[builtin] pub fn dialog(dialog: fn() -> Dialog);
```

- `dialog` `fn() -> Dialog` — the dialog component.
- Kind: `fn` · Builtin: yes

Pushes a modal dialog. The dialog is drawn over a still image of the modes below. A dialog must
offer a way out, usually a button calling [`nav::back()`](#back) (`E6003` otherwise).

```kix
fn ConfirmQuit() -> Dialog {
    Dialog(column![
        t!["Quit the game?"],
        row![
            button!{on_click = || nav::quit(), t!["Yes"]},
            button!{on_click = || nav::back(), t!["No"]},
        ],
    ])
}
```

## Function: `back()` {#back}

```kis
#[builtin] pub fn back();
```

- Kind: `fn` · Builtin: yes

Pops the top mode, returning to the mode below. On a base mode (a root page or the game mode) it
does nothing at run time; a `nav::back()` the compiler can see running in a base mode is the error
`E6002`. To leave a root page, use [`nav::goto`](#goto) instead. `Action::Back` calls
`nav::back()`.

## Function: `goto(page)` {#goto}

```kis
#[builtin] pub fn goto(page: fn() -> RootPage);
```

- `page` `fn() -> RootPage` — the root page component.
- Kind: `fn` · Builtin: yes

Replaces the whole stack with the root page `page`. Called while a game runs, it ends the game;
for example, a "Back to title" button.

```kix
button!{on_click = || nav::goto(Title), t!["Back to title"]}
```

## Function: `start(entry, args, on_exit)` {#start}

```kis
#[builtin] pub fn start<E, A, R>(entry: E, args: A, on_exit: Fn(R));
```

- `entry` `E` — a pointer to an `#[entry]` scene, of type `scene fn(P1, …, Pn) -> R`.
- `args` `A` — the tuple `(P1, …, Pn)` of the scene's arguments.
- `on_exit` `Fn(R)` — the exit handler; it receives the scene's return value.
- Kind: `fn` · Builtin: yes

Replaces the stack with a new game that runs the entry scene `entry` with `args`. This is the only
way into the story.

- The typing of this signature is special: the compiler relates `E`, `A` and `R` as above. See
  [Compiler Rules](./appendix/rules.md).
- The arguments have the entry scene's finite parameter types. They are recorded in the save, and
  they are how [meta](./narrative.md) values reach the story: read a `Meta` with `get()` in the
  system and pass it in.
- When the entry scene returns, its value is passed to `on_exit`. A return type such as an
  `Ending` enum is handled with an ordinary exhaustive `match`.
- If `on_exit` navigates nowhere, the stack resets to the [`System::start`](./system.md#system)
  page.
- A new game starts fresh: story variables have their initial values. A replay of a scene is a
  `nav::start` of a further `#[entry]` scene (see
  [System Recipes](./guides/system.md#replay)).

```kix
fn start_button() -> Element {
    button!{on_click = || nav::start(story::Main, (CLEARED_AZUSA.get(),), on_ending), t!["Start"]}
}

fn on_ending(ending: story::Ending) {
    match ending {
        story::Ending::AzusaEnd => { CLEARED_AZUSA.set(true); nav::goto(Credits) }
        story::Ending::NormalEnd => nav::goto(Credits),
    }
}
```

For an entry scene without arguments or result, pass `()` and a closure that ignores its
argument:

```kix
button!{on_click = || nav::start(story::Main, (), |_end| nav::goto(Title)), t!["Start"]}
```

## Function: `load(slot)` {#load}

```kis
#[builtin] pub fn load(slot: SaveSlot) -> Result<(), Divergence>;
```

- `slot` [`SaveSlot`](./system-saves.md#saveslot) — the slot to load.
- Returns: `Result<(), Divergence>` — `Ok(())` when the game was restored;
  `Err(divergence)` when the save no longer fits the program.
- Kind: `fn` · Builtin: yes

Replaces the stack with the game restored from `slot`. A save records the path the player took;
if a later version of the script changed that path, the save **diverges**: `load` returns `Err`
and leaves the stack unchanged. Show the player
[`Divergence::describe()`](./system-saves.md#divergence-describe).

```kix
use std::system::saves::SaveSlot;

fn Load() -> Overlay {
    let (error, set_error) = signal("");
    Overlay(column![
        button!{
            on_click = || match nav::load(SaveSlot::Manual(0)) {
                Ok(()) => {}
                Err(divergence) => set_error(divergence.describe()),
            },
            t!["Slot 1"],
        },
        text!(error()),
        button!{on_click = || nav::back(), t!["Back"]},
    ])
}
```

## Function: `quit()` {#quit}

```kis
#[builtin] pub fn quit();
```

- Kind: `fn` · Builtin: yes

Quits the program at once, without confirmation. A Quit button that should ask first opens a
[`Dialog`](#dialog) the studio writes.
