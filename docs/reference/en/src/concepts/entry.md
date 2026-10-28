# Entry Scenes and Endings

## Entry scenes

```kix
enum Ending { NormalEnd, AzusaEnd }

#[scene]
#[entry]
pushdown fn Main(cleared_azusa: bool) -> Ending {
    …
    if ROUTE == Route::Azusa {
        return Ending::AzusaEnd;
    }
    Ending::NormalEnd
}
```

An **entry scene** is a scene also marked `#[entry]`. It is where the system can start the story,
and it is the only door from Turing-complete code into the story.

- A game has at least one entry scene. Extra chapters, after stories and scene replays are
  further entry scenes.
- Its parameters and return type are finite (`E3008`), and they cannot contain `Text` (`E3009`):
  `fn` code can build runtime text, so a `Text` from the system would not be one of the program's
  constant texts.
- `#[entry]` without `#[scene]` is `E3018`. An entry scene is an ordinary scene otherwise: other
  scenes may also call it.
- Entry scenes are the roots of every story analysis: a scene or step that no entry scene can
  reach is reported (`W5008`, `W5009`).

## Starting the story

The system starts the story with `nav::start` from `fn` code:

```kix
button!{on_click = || nav::start(story::Main, (CLEARED_AZUSA.get(),), on_ending), t!["Start"]}

fn on_ending(ending: story::Ending) {
    match ending {
        story::Ending::AzusaEnd => { CLEARED_AZUSA.set(true); nav::goto(Credits) }
        story::Ending::NormalEnd => nav::goto(Credits),
    }
}
```

`nav::start` takes an entry scene, its arguments, and an exit handler. This is the only way into
the story.

- `story::Main` is a `scene fn(P1, …, Pn) -> R` pointer to an `#[entry]` scene, the arguments are
  the tuple `(P1, …, Pn)`, and the exit handler is an `Fn(R)`. A call that does not fit is
  `E2027`.
- The arguments have the entry scene's finite parameter types. They are recorded in the save.

The system side is described in [The Game System](./system.md) and in the STD Reference,
[`std::system::nav`](../../../../std/en/src/system-nav.md).

## Arguments

Entry arguments are how the story reads cross-playthrough state, such as an ending cleared in an
earlier run. The system reads the meta field and passes its value when it starts the story. The
arguments are stored in the save with the trace, so loading replays with the same values even if
meta changed since. The compiler treats them as free: every analysis holds for every argument
value.

## The ending

When the entry scene returns, the story is over and control goes back to the system. The return
value tells the system which ending was reached, and the system's exit handler decides what comes
next, such as rolling the credits or returning to the title. Only the value crosses over: the
system cannot see story variables. If the handler navigates nowhere, the mode stack resets to the
start page.

Because every path of `Main` must return an `Ending`, a path that forgets its ending is a type
error. Because the exit handler is an exhaustive `match`, an ending the system does not handle is
a compile error too.

## Writing meta

```kix
CLEARED_AZUSA.write(true);
```

The story writes a meta field with `write`, a `pushdown fn` method of `Meta<T>`:

| Method | Kind | Use |
| ------ | ---- | --- |
| `get()` | `fn` | system code reads it |
| `set(v)` | `fn` | system code writes it |
| `write(v)` | `pushdown fn` | the story writes it; the story can never read it back |

Meta is **write-only** from the story: reading it is `fn`-only, and `const fn` code cannot name
it at all (`E3013`). A write the story can never read back cannot change its control flow, so
every story analysis is unaffected. Writes are not reverted by load or rewind: unlocks stay
unlocked. Meta reaches the story only through the arguments of an entry scene, at start.

A **replay** of a scene from an extras page is `nav::start` of a further `#[entry]` scene,
unlocked by a `Meta<bool>` that the story writes when it first passes the scene.
