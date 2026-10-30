---
title: "The Debugger"
---

The story debugger moves around the story and answers questions about it without playing it by
hand: where the story is, what comes next, whether a step can still be reached from here and
which choices (and random draws) reach it. It works on the **query engine**, which runs the game
without a window, and on a game **running in a window**, where every move shows the new state at
once. Editors reach both through the Debug Adapter Protocol. For the design, see the Develop
Guide's [The Debugger](https://github.com/kirikiroid3/kagari/blob/main/docs/dev/src/runtime/debugger.md).

## Debug builds

The debugger reads the debug artifact `game.kagari-cache` that `kagari build --profile debug`
writes next to `game.kagari` (see [Build profiles](./cli.md#build-profiles)): the explored story
graph, which `nav` needs, and the names of locals. Without it the other commands still work, and
variables are named `arg0`, `arg1`, … A debug artifact from another build is ignored.
`kagari debug` and `kagari dap` build the debug profile themselves before they start a game.

## Positions {#positions}

A position is a **step ID** (`crate::story::Start#3`: the function's path, `#`, and the step's
number in its function; see [Step IDs](../concepts/steps.md#step-ids)). An **arm** of a choice is
named by its arm ID, as saves record it: the variant path of the arm's value (`crate::Pick::B`),
or `#` and its index in the arm list. The debugger also accepts the last segments of an arm ID
when they name one shown arm (`B`, `Pick::B`).

The debugger moves the story the way a save is loaded: it replays the choices from the entry
scene ([Saves, Replay and Determinism](../concepts/saves.md)). Nothing is saved or heard while it
does: no read flags, no meta writes, no autosaves, no audio. After a move the story **stops**
where it arrived; `continue` goes on.

## `kagari debug` {#kagari-debug}

```text
kagari debug [--manifest-path <PATH>] [--target <TARGET>] [--entry <PATH>] [--seed <N>] [--no-build] [--watch]
kagari debug --window [--runtime <PATH>] [--manifest-path <PATH>] [--target <TARGET>] [--no-build] [--watch]
kagari debug --attach <ADDRESS> [--manifest-path <PATH>] [--target <TARGET>] [--watch]
```

`kagari debug` builds the project in the debug profile, starts the game and reads commands, one
per line. By default it runs the query engine: the story starts at the entry scene the story
graph lists first (or `--entry`), with the arguments the graph has for it, and the seed `--seed`
(default `1`), and stops at its first step.

| Option | Meaning |
| ------ | ------- |
| `--manifest-path <PATH>` | The project's `kagari.toml`; by default the nearest one. |
| `--target <TARGET>` | The target to debug; by default the first of `build.targets`. |
| `--entry <PATH>` | The entry scene the query engine starts (query engine only). |
| `--seed <N>` | The query engine's random seed. Default `1`. |
| `--window` | Start the game in a window instead (`kagari-desktop-runtime`, found next to `kagari` or on `PATH`). |
| `--runtime <PATH>` | The runtime `--window` starts. |
| `--attach <ADDRESS>` | Attach to a game started with `--debug-port`: `PORT` or `HOST:PORT`. Nothing is built. |
| `--no-build` | Do not build first; use the files already in the target directory. |
| `--watch` | Watch the project's files (about every half second): on a change, print `source changed: rebuilding` and do what `swap` does. |

Locations are shown as `file:line:column`, with files relative to the directory of the project's
root file. Commands can be piped in (`kagari debug < commands.txt`); the debugger answers them in
order and exits at the end of the input.

## Commands {#commands}

| Command | Effect |
| ------- | ------ |
| `stack` | The call stack from the entry scene to the current step, innermost first: each scene with its arguments and the step (or the call site) it is at. |
| `ls [-h N \| --head N] [-t N \| --tail N]` | Every step from the current one up to the next choice, one line each: step ID, speaker, the first words. `-h` keeps the first `N`, `-t` the last `N`. |
| `goto <step>` | Jump to a step between the last choice and the next choice (both included). Anything else is an error that names those two choices. |
| `push <arm>...` | Starting at the next choice, select the first arm, go on to the following choice and select the second, and so on. Stops right after the last arm is selected. A choice that does not offer the arm stops the push there, with an error that lists the arms it offers. |
| `pop` | Back to the last choice, before its arm was selected. Each `pop` goes one choice further back. |
| `detail [<step>]` | Everything about a step (by default the current one; another one must lie between the last choice and the next): the speaker and the text with its rubies and effects, the voice, the render and whether it is a keyframe; for a choice, its `id` and every arm with its arm ID, its text and whether it is shown. |
| `nav <step> [--seeds N]` | Whether the step can be reached from the current state, and how (below). |
| `seed [<n>]` | Show the random seed, or replay the current position with seed `n` (an error when the choices no longer fit). |
| `continue`, `c` | Go on: to a breakpoint, a choice or the end of the story (in a window: the player plays on). |
| `next`, `n` | To the next step in this scene, past the steps of the scenes it calls. |
| `step`, `s` | To the next step, into a called scene. |
| `out` | To the step after the current scene returns. |
| `pause` | Stop a game that is running. |
| `vars [<frame>]` | The arguments, locals and story variables of a frame (`0` is the innermost), and the current step's record. |
| `break <step \| file:line>...` | Add breakpoints; `break` alone lists them. |
| `delete` | Remove every breakpoint. |
| `swap` | Rebuild the debug profile and [hot swap](#hot-swap) the new game in, then set the breakpoints again on the new sources. |
| `help`, `quit` | |

`continue`, `next`, `step` and `out` stop at a choice, since a choice needs an arm (use `push`).
When the story would end, they stop at its last step and say so.

## `nav` {#nav}

`nav` searches the story graph from the node that matches the current state (the scene, its
arguments, its locals and the story variables), past the ends of the current scenes up the real
call stack, for one shortest way to any state at the step. It answers:

| Answer | Meaning |
| ------ | ------- |
| reachable | The `push` arguments that realize the shortest path, and the random draws it passes. |
| unreachable | No way exists. Claimed only when the story graph is complete. |
| unknown | The graph is incomplete (the `W5006` cap or a `W5007` path given up is named), or the current state is not in the graph (a stale debug artifact, or a state past a cap). |

**Random draws.** The graph follows every value of `rng_choose`, but play follows the seed. When
the path passes draws, `nav` replays the current position plus the path with other seeds: the
current seed first, then `0`, `1`, `2`, …, at most `--seeds` tries (default 1000). It suggests a
seed that reaches the step (`seed <n>`, then `push`), or says that none did within the limit.
Each try replays the whole story so far, so the search takes longer for long stories and for
unlikely draws; `nav` reports the odds (one in how many seeds would draw the path's values).
Another seed also changes the draws before the current position, so a seed can fail because the
choices made so far no longer fit.

```text
(kagari) nav crate::AmusementPark#5
reachable: 9 steps
  push crate::Pick::Bold crate::Ride::Coaster
  draws: crate::AmusementPark draws crate::Coin::Tails (one of 2)
  the current seed already gives a path there: `push` it
```

## Breakpoints and stepping {#breakpoints}

A **step breakpoint** stops when the story enters a step (`break crate::Start#4`, or a source
line with a step form such as `s!(…)` or `choice!{…}`). A source line without a step form is an
**instruction breakpoint**: it stops before the line's code runs, in a scene, a `pushdown fn` or a
`const fn` (a render included). Lines in `fn` are not debugged (system code), and a line without
code is rejected.

At a stop inside code, `stack` and `vars` show the frames and their variables, and `next`,
`step` and `out` step through the code line by line; when the story reaches its next step, the
stop is at that step. The story commands that move it wait until the story is at a step again.
In a window, the window does not redraw while the story is stopped inside code.

The debugger's own moves (`goto`, `pop`, `seed`, `ls`, `detail`, `nav`) replay story code
without stopping in it; `push` and the execution commands stop at instruction breakpoints.

## Debugging a game in a window {#window}

Start the runtime with a debug channel and attach to it:

```text
kagari-desktop-runtime target/v0 --debug-port 4711
kagari debug --attach 4711
```

While the debugger has the story stopped, the player's story input (advance, choices, auto,
skip) is ignored; menus, saves and the backlog still work, and a load or a new game ends the stop.
`continue` gives the story back to the player, and the game stops again at a breakpoint or when
`pause` is requested. Every move shows the new state at once, without the animations in between;
the current step's own animations play again.

## Hot swap {#hot-swap}

Hot swap puts a rebuilt game in place of the running one without leaving the story: edit a scene,
then `swap` (or let `--watch` do it). The game is rebuilt in the debug profile, and the runtime
compares the two debug builds to find the code that changed. Hot swap is a debugging aid; it needs
debug builds of both games, and a game run without a debugger never pays for it.

Scenes whose code changed are **thrown off the stack**. A scene counts as changed when its body
changed (a step's text included), or when code that runs inside it changed: a `pushdown fn`, a
`const fn` or a render it uses. Walking the call stack from the entry scene, the first changed
scene and every scene above it are thrown off, and that scene is **called again** by its caller
with the same arguments: it starts from its first step with the new code. The callers keep their
position and their locals. Story variables, the values behind `&mut` arguments, the random state,
the position and the screen go back to what they were when the call was made, so what the thrown
scenes did is undone. The new step shows at once and the story stops there.

```text
(kagari) swap
wrote target/v0/game.kagari
swapped; called `crate::story::Greet` again (thrown off: crate::story::Greet), now at crate::story::Greet#0
stopped (swap) at crate::story::Greet#0
```

When nothing on the stack changed, the story stays at its step (`nothing on the stack changed`),
and the changed scenes run their new code when they are called. The story **restarts** from its
entry scene, with the same arguments and seed, and the reason is shown, when:

- the entry scene itself changed;
- a story variable was added, removed, or changed type;
- a value the surviving story holds can no longer be carried over: its type was redefined, or an
  asset or a `Setting` or `Meta` it names is gone;
- the scene to call again was entered before the game recorded its calls. A window records them
  from its start when it runs a debug build with `--debug-port` or `--debug-stdio`; the query
  engine always does;
- the new code stops with an error right away.

When the story cannot restart either (the entry scene is gone, or its arguments no longer fit),
the game goes back to its start page. Menus that were open over the game are closed by a swap, and
assets are read again from the rebuilt archives. The position after a swap is a normal position:
a save made afterwards loads by replaying it on the new game.

## Editors {#editors}

`kagari dap` is a debug adapter: it speaks the Debug Adapter Protocol over standard input and
output. The Zed extension registers it as the adapter `Kagari` (see
[Zed](./editors.md#debugging-in-zed)). Its launch configuration starts a game, by default in a
window:

| Key | Meaning |
| --- | ------- |
| `request` | `launch` or `attach`. |
| `manifestPath`, `cwd`, `target` | The project and target (as for `kagari debug`). |
| `mode` | `window` (default) or `headless` (the query engine). |
| `runtime`, `runtimeArgs` | The runtime a window launch starts, and extra arguments. |
| `entry`, `seed` | The query engine's entry scene and seed. |
| `buildFirst` | Build the debug profile first. Default `true`. |
| `stopOnEntry` | Window: stop at the first step the story enters. |
| `host`, `port` | Attach: where the game listens (`--debug-port`). |
| `watch` | Watch the project's files and [hot swap](#hot-swap) on a change, as `kagari debug --watch`. Default `false`. |

The editor's views map onto the story: one thread (the story), the call stack of scenes, the
variables of each frame and the story variables, breakpoints on lines, and continue, step over,
step into and step out as above. The debug console evaluates the commands of this page (`ls`,
`nav crate::Start#9`, `push B`, `swap`). The story commands are also custom requests
`kagari/stack`, `kagari/ls`, `kagari/goto`, `kagari/push`, `kagari/pop`, `kagari/detail`,
`kagari/nav` and `kagari/seed`, with structured results. The custom request `kagari/swap` rebuilds
and hot swaps the game, sets the line breakpoints again and answers with the outcome (`kept`,
`recalled`, `restarted` or `reset`), the scene called again, the reason, the scenes thrown off and
the new position. Conditional, function and exception breakpoints are not supported.
