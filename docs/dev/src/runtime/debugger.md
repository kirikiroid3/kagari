# The Debugger

The story debugger lets an author move around the story and ask about it without playing it by
hand: where the story is (`stack`, `ls`, `detail`), moving it (`goto`, `push`, `pop`, `seed`),
whether a step can still be reached and how (`nav`), step breakpoints and stepping, and
instruction breakpoints with stepping inside story code. The same commands work on the
**headless query engine** (no window) and on a game **running in a window**, and editors reach
both over the Debug Adapter Protocol. The user-facing description is the Reference's
[The Debugger](https://kagari.suitsu31.club/en/reference/tools/debugger/).

## Where in the code

| Path | Contents |
| ---- | -------- |
| `runtime_libs/kagari-debugger/src/session.rs` | `Debugger`: the query commands, step breakpoints, step-level execution (`run`), and the pilots that steer the engine's drives |
| `runtime_libs/kagari-debugger/src/tracker.rs` | `Tracker`: the story's node in the story graph, followed in lockstep |
| `runtime_libs/kagari-debugger/src/nav.rs` | `Summaries`, `shortest`: shortest paths through the story graph |
| `runtime_libs/kagari-debugger/src/hook.rs` | `HookState`, `Hook`: instruction breakpoints and code stepping (the VM's `InstrHook`) |
| `runtime_libs/kagari-debugger/src/server.rs` | `Server` (live and headless), `serve_code_stop` |
| `runtime_libs/kagari-debugger/src/protocol.rs` | The Kagari debug protocol: `Message`, framing, commands, events, bodies |
| `runtime_libs/kagari-debugger/src/transport.rs` | `Link` (channels; in-process pair, byte streams, TCP), `listen`, `Connection` |
| `runtime_libs/kagari-debugger/src/frames.rs` | `Artifact` (the debug artifact a runtime reads), frames and scopes for the protocol |
| `runtime_libs/kagari-debugger/src/values.rs` | VM values ↔ story-graph values; display of values and texts |
| `runtime_libs/kagari-debugger/src/headless.rs` | `HeadlessPlatform`, `Headless`, `serve`: the query engine's thread |
| `runtime_libs/kagari-debugger/src/tests.rs` | The query engine and the server on a compiled fixture |
| `runtime_libs/kagari-engine/src/debug.rs` | The engine side: drives, the hold, live step breakpoints ([The Engine](./engine.md#debugger-access)) |
| `runtime_libs/kagari-vm/src/hook.rs` | `InstrHook`, `HookStop`, `FrameView` ([The VM](./vm.md)) |
| `runtime/kagari-desktop-runtime/src/debug.rs` | `--debug-port`, `--debug-stdio` ([The Desktop Runtime](./desktop.md#the-debug-channel)) |
| `tooling/kagari-cli/src/debugger/` | What the front ends share: `backend.rs` (reaching a runtime, the debug build), `locate.rs` (offsets ↔ lines, line breakpoints), `text.rs` (commands as text) |
| `tooling/kagari-cli/src/command/debug.rs`, `command/dap.rs` | `kagari debug` (the REPL), `kagari dap` (the adapter) ([Projects and the CLI](../tooling/cli.md)) |
| `tooling/zed-kagari` | Registers the adapter in Zed ([Language Server and Editors](../tooling/lsp.md)) |

`kagari-debugger` depends on the engine, the VM, `kagari-debug` and `kagari-archive`, never on the
compiler: the story graph and the names of locals come from the debug artifact
(`game.kagari-cache`, [`.kagari-cache` Files](../formats/cache.md)), which a runtime reads only
when its script hash is that of the `game.kagari` it runs.

## Structure

<!-- diagram: debugger-components (mermaid-ish)
flowchart TB
  zed["Zed (DAP client)"] ==>|DAP| dap["kagari dap: lines, variable refs"]
  term["terminal"] ==> repl["kagari debug: REPL, text commands"]
  dap ==>|"KDP: in-process link, --debug-stdio, --debug-port"| server["kagari_debugger::Server: live or headless"]
  repl ==> server
  server ==> debugger["Debugger: query engine, tracker, nav"]
  debugger ==> engine["Engine: probe, jump, step; the hold"]
  engine ==>|VM| hook["Hook (InstrHook): stops inside code, blocks"]
  hook -.->|serves stops| server
-->
{{#include ../diagrams/debugger-components.svg}}

- **Front ends** (`kagari-cli`) have the project: they build the debug profile, read the sources
  to turn byte offsets into lines, and turn source lines into breakpoints. `kagari debug` is a
  REPL; `kagari dap` speaks DAP to an editor.
- **The runtime side** (`kagari-debugger`) has the engine. A `Server` takes protocol requests and
  runs them on a `Debugger` over the engine; it is **live** inside a game the player plays (the
  desktop window, polled once per frame) or **headless** (the query engine, which serves until
  the client disconnects).
- **Transports**: the headless query engine runs on a thread of the front end with an in-process
  link; a window is launched with `--debug-stdio`; a running game opens `--debug-port`.

## Positions and moves

The story's position is its trace ([Saves](./saves.md)): the entry scene, its arguments, the seed,
the choice IDs taken and the lines advanced since the last choice. Every move is a **drive** of
the engine ([The Engine](./engine.md#debugger-access)): the save replay loop follows a trace, then
a pilot steers on, step by step, with side effects off.

| Command | Drive | Pilot |
| ------- | ----- | ----- |
| `ls` | probe the current trace | go on through lines, stop at the next choice (or 10 000 lines) |
| `goto <step>` | probe from the last choice to find the step's line count; jump to the cut trace | stay |
| `detail <step>` | probe from the last choice | stop at the step, take its record |
| `push <arms>` | step the live game | lines go on; each choice selects the next arm; stop after the last |
| `pop` | jump to the trace without its last choice | go on to that choice |
| `seed <n>` | jump to the trace with seed `n` | stay |
| `nav` | probe the current trace (tracking); probe once per seed tried | follow the path's arms to the target |
| `continue`, `next`, `stepIn`, `stepOut` (headless; at a step in a window) | step the live game | below |

A jump or a step installs the result like a load (`NavOp::Restore`): the screen shows the new
step at once, without the animations in between, and the game is held. `goto` accepts only the
steps from the step after the last choice (or the start) up to the next choice, the choice
included; anything else is an error naming the last choice and the next one. `push` accepts an
arm ID (`crate::Pick::B`), `#index`, or the arm ID's last segments when they name one shown arm
(`B`, `Pick::B`); a choice that does not offer the arm stops the push there with an error listing
the arms it offers. `detail` reports the record (`who`, the text with rubies and effects, the
voice), the render's path and, from the story graph, whether it is a keyframe; for a choice every
arm with its arm ID, text and whether it is shown. Guards are not shown: the record only has the
`shown` flag they produced.

**Decision.** The debugger's own replays never stop in code: `goto`, `pop`, `seed`, `ls`,
`detail` and `nav` disarm the instruction hook; `push`, `continue` and the steps run armed.
**Why.** A replay re-runs code the author already passed; stopping there would interrupt a move
in the middle. `push` runs code that has not run yet, like play.
**Consequences.** An instruction breakpoint in a scene called right after a choice is reached by
`push` (headless) or by the player's selection (window).

## Following the story through the graph

`nav` needs the story's node in the explored story graph ([`.kagari-cache`
Files](../formats/cache.md#story)). `Tracker` walks the graph in lockstep with a drive: the trace's
entry and arguments pick the root context; from a step node, a line goes to its successor and a
choice to its arm's target; a scene call enters its callee's context; a draw takes the value the
fiber drew (`StoryFiber::draws`, the RAND results of the resume); an exit returns to the caller's
call node, whose successor for that exit is known. The walk is deterministic, so the tracker
holds one node and the real call stack above it (the callers' call nodes). The node the walk
reaches must be the step the story reached, or the tracker is **lost**: the graph does not
describe this state (a stale artifact, or a state past a cap or a given-up path). The tracker of
a trace is cached until the trace changes.

## `nav`

`shortest` searches from the tracked node to any node of the target step (Dijkstra; the cost is
the number of steps passed):

- within a context along node targets: a line's successor, every arm of a choice, every value of
  a draw;
- past a scene call through a **summary**: the fewest steps from the callee's entry to each of
  its exits (computed for every context once per artifact, to a fixpoint, since contexts call
  each other), after which the caller resumes at that exit's successor;
- into a callee (**descending**), for targets inside it; a descended context's exits lead
  nowhere, since a path that must return uses the summary;
- up the real stack: an exit of a context on the real stack returns to the real caller.

| Answer | When |
| ------ | ---- |
| reachable | a path exists: its choices as `push` arguments, its draws, its length |
| unreachable | no path, and the graph is complete |
| unknown | no path in an incomplete graph (the `W5006`/`W5007` reasons are named), or the tracker is lost |

**Draws.** When the path passes draws, `nav` replays the current trace with other seeds and
follows the path (`RealizePilot`) until the target is reached: the current seed first, then 0,
1, 2, …, at most `--seeds` (default `DEFAULT_SEEDS`, 1000) tries. Each try costs a replay of the
whole trace, so the cost grows with the trace and with the odds of the draws, which `nav` reports
as the product of the outcome counts (one in how many seeds would draw them all, if the draws
were independent and uniform). A seed found is suggested (`seed <n>`, then `push`); none found
says so with the limit. Changing the seed also changes the draws before the current position, so
a seed whose replay of the trace diverges counts as a failed try.

## Execution

**Step level.** `continue`, `next`, `stepIn` and `stepOut` at a step drive the story on
(`ExecPilot`): lines go on; the drive stops at a step breakpoint, at a choice (which needs an arm:
reason `choice`), when the story would return (reason `end`, back at its last step), at the next
step for `stepIn`, at the next step at the same or a smaller fiber depth for `next`, at a smaller
depth for `stepOut`, and on a `pause` request (headless: checked at every step). In a window,
`continue` instead releases the hold and the player plays on; the engine stops by itself at a step
breakpoint or a pause ([The Engine](./engine.md#debugger-access)) and the live server reports it
on its next poll. A load or a new game by the player ends the hold; the server then sends
`continued`.

**Inside code.** Instruction breakpoints are code words of story functions (`fn` has none). The
VM asks the hook before every instruction while it is installed (one `Option` check per
instruction when no debugger is attached; [The VM](./vm.md)). A stop sends `stopped` with
`inCode` and **blocks in the VM**, serving `stackTrace`, `scopes`, breakpoint changes and
`continue`/`next`/`stepIn`/`stepOut`/`disconnect`; moves are refused (the VM is in the middle of
the story). In a window the frame loop is blocked meanwhile, so the window does not redraw.

A code step stops at the next source **position** (a change of the Debug section's byte offset)
at the same or a smaller call depth (`next`), at any depth (`stepIn`), or at a smaller depth
(`stepOut`), in steppable functions only (story code whose file is not under `std/`). A code step
from story code stays in story runs: a render the engine calls on the way is skipped. When the
story reaches a step first, the code step ends there as a step-level stop: the hook sets the
engine's pause flag (`Engine::debug_pause_flag`), which holds the live game at the next step it
enters; the headless pilot checks the same state. `kagari dap` turns positions into lines by
repeating a code step while the line and the depth stay the same.

## The Kagari debug protocol

The framing and the envelope are DAP's (`Content-Length` headers; `request`, `response` and
`event` messages with `seq`, `request_seq`, `success`, `message`, `body`), so `kagari dap` uses one
codec for the editor and the runtime. The payloads are Kagari's: positions are step IDs and
locations are a `file` (as the bytecode's Debug section names it: relative to the root file's
directory, or `std/…`) and a byte `offset`.

| Request | Arguments | Body |
| ------- | --------- | ---- |
| `initialize` | — | `scriptHash`, `artifact` (`graph`, `complete`, `incomplete`), `live`, `stopped` |
| `setStepBreakpoints` | `steps`: step IDs | `verified` per step |
| `setInstructionBreakpoints` | `points`: `{function, word}` | `verified` per point |
| `continue`, `next`, `stepIn`, `stepOut`, `pause` | — | — (a `stopped` event follows) |
| `stackTrace`, `kagari/stack` | — | `frames`, innermost first: `function`, `kind`, `file`, `offset`, `step`, `args` |
| `scopes` | `frame` | `scopes`: Arguments, Locals, Story variables, and Step for the innermost frame at a step |
| `kagari/ls` | `head`, `tail` | `lines` (`step`, `kind`, `who`, `text`), `total`, `ends` (`choice`, `end`, `limit`) |
| `kagari/goto`, `kagari/detail` | `step` | a position; a detail |
| `kagari/push` | `arms` | `position`, `selected` (choice IDs) |
| `kagari/pop` | — | a position |
| `kagari/nav` | `step`, `seeds` | `answer`, `reason`, `path` (choices and draws), `push`, `length`, `seed` (the search) |
| `kagari/seed` | `seed` | `seed` |
| `disconnect` | — | — (the runtime removes the hook and the breakpoints and releases the story) |

Events: `stopped` (`reason`: `entry`, `breakpoint`, `step`, `pause`, `choice`, `end`, `goto` after
a move, `error`; `step`; `description`; `inCode`), `continued`, `output`. A value is a `Var`:
`name`, its display (`crate::Pick::B`, `crate::Point { x: 1, y: 2 }`, `t!["…"]`) and its parts as
children. `kagari/swap` (scene hot swap) is not part of the protocol yet.

## Front ends

**`kagari debug`** runs `kagari build --profile debug` (unless `--no-build`), then connects:
the headless query engine on a thread (default; `--entry`, `--seed`), a window (`--window`: the
runtime is launched with `--debug-stdio`) or a running game (`--attach PORT`). It reads lines on a
thread and prints responses and events as they arrive; `break` takes step IDs or `file:line`
(the steps on the line, else the first code word on it of each story function), `vars` shows a
frame's scopes. The headless story starts at the story graph's first root (or `--entry`) with the
graph's arguments; without a graph, at an entry scene without parameters.

**`kagari dap`** is the adapter. It maps DAP onto the protocol:

| DAP | Kagari |
| --- | ------ |
| `launch` (`mode`: `window` or `headless`; `buildFirst`, `stopOnEntry`, …), `attach` (`host`, `port`) | a backend as above; `stopOnEntry` sends `pause` |
| `threads` | one thread, the story |
| `setBreakpoints` | per line: the step IDs of its step forms, else instruction breakpoints; lines in `fn` or without code are unverified with the reason |
| `stackTrace`, `scopes`, `variables` | `stackTrace` and `scopes`, with lines from the sources and variable references for the parts |
| `continue`, `next`, `stepIn`, `stepOut`, `pause` | the same requests (code steps repeated to whole lines) |
| `evaluate` | a query command as text (`ls`, `nav …`), answered as text |
| `kagari/*` | forwarded unchanged |
| `disconnect`, `terminate` | `disconnect`; a launched window is ended |

## Decisions

**Decision.** A runtime speaks its own protocol with DAP's framing, and the adapter translates.
**Why.** A runtime has no sources, so it cannot speak lines; reusing DAP's framing and envelope
keeps one codec and lets the adapter forward the story commands unchanged.
**Consequences.** `kagari dap` needs the project (sources, `game.kagari`); an editor attaching to a
game must point the adapter at the project.

**Decision.** The headless query engine runs inside the front end's process.
**Why.** `kagari-cli` already builds the project; the query engine needs only the engine and the
archives (`ArchiveDir`), not a window or a separate binary.
**Consequences.** `kagari-cli` links the engine (not the renderer). A window is a separate process
(`kagari-desktop-runtime`), found next to `kagari` or on `PATH`.

## Invariants

- The debugger moves the story only through engine drives; it never writes story state.
- Replaying moves never stop in code; only `push` and execution run with the hook armed.
- `nav` claims `unreachable` only for a complete graph and a tracked state; otherwise `unknown`.
- A debug artifact is used only when its script hash is the running bytecode's.
- Without a debugger attached nothing changes: no hook, no hold, no channel (a release build of
  the runtime opens a channel only with `--debug-port` or `--debug-stdio`).

## When you change this

- A command: `session.rs`, the protocol's `command` and bodies, the server's dispatch, `text.rs`
  (parse, format, `HELP`), `kagari dap` if it needs a mapping, the tests in
  `runtime_libs/kagari-debugger/src/tests.rs` and `tooling/kagari-cli/tests/`, this chapter and
  the Reference [The Debugger](https://kagari.suitsu31.club/en/reference/tools/debugger/) (en and ja).
- The engine's drives or the hold: [The Engine](./engine.md#debugger-access) and
  `runtime_libs/kagari-engine/src/tests/debug.rs`.
- The story graph's shape: the tracker and `nav` read it ([`.kagari-cache` Files](../formats/cache.md)).
- Launch or attach settings: `command/dap.rs`, the Zed schema
  (`tooling/zed-kagari/debug_adapter_schemas/Kagari.json`), the Reference's editor page.
