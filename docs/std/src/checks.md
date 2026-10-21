# System Checks

The system scope is checked by different rules than the story. The story's rules exist to keep
narrative analysis decidable and presentation consistent. The system has neither concern: its
state is a finite mode graph plus values no analysis quantifies over. It has others: every mode
must be reachable in the right context, and the player must never be trapped.

| Check                          | Game scope                         | System scope |
| ------------------------------ | ---------------------------------- | ------------ |
| Finite data                    | required, for decidability         | not required for settings; bounded types suggested |
| Every loop passes through a step | required                         | not applicable: handlers are calls of non-recursive, loop-free system functions |
| Consistency, bounded screens   | required                           | not applicable: views are full |
| Reachability, dead code        | pushdown reachability              | reachability on the finite mode graph |
| Context typing                 | not applicable                     | `interface__call(m)` only where `m.requires ⊆ ctx`; game-only operations only with a running game |
| No `interface__return` from a base | not applicable                 | required |
| No trap                        | stuck-choice lint                  | every reachable overlay can return or reset |
| Bounded stack                  | not required                       | lint: the push graph is acyclic, so the stack depth is bounded |
| Exhaustive endings             | not applicable                     | `<on-exit>` covers every variant of `Ending` |
| Isolation                      | no system names except `std/system-ctx.kix` | no story names except `std/narrative.kix`, entry scenes and enums |
| Operation arguments            | not applicable                     | `metafn__start` names an entry scene, slot literals below `count`, modes of the right kind, arguments assignable to the parameter types |

## The mode graph

The checks run on a graph built at compile time. The operations of a mode are those reachable
from every `on-click` in its UI, its `on-keyboard` and `on-controller` handlers, and, for the game
mode, its `<on-exit>`. They are collected through system function calls, over all branches. Each
operation records its *site*, the handler attribute or the operation tag in `<on-exit>`, and its
*origin*, the operation tag inside a system function.

`interface__call(m)` and `interface__goto(m)` are edges to `m`; `metafn__start` and
`metafn__load` are edges to the game mode. `metafn__revert` counts as `interface__return`.

Each mode gets a set of contexts, a subset of {NoGame, Game}. The start page and every goto
target get NoGame, and the game mode gets Game. An overlay inherits the union of the contexts of
the modes that call it, up to a fixpoint. A mode with no context is unreachable. If `<system
start>` does not resolve, none of these checks run.

| Check | Code | Rule |
| ----- | ---- | ---- |
| Reachability | `kis/unreachable-mode` (warning) | "mode `x` can never be shown", at the mode name |
| Context typing | `kis/context-requires` | see [Context typing](#context-typing) |
| Game-top operations | `kis/needs-game-mode` | `metafn__advance` and `metafn__select` only from the game mode |
| No return from a base | `kis/pop-from-base` | no `interface__return` or `metafn__revert` in a root page or the game mode |
| No trap | `kis/trapped-mode` | see [No trap](#no-trap) |
| Bounded stack | `kis/push-cycle` (warning) | see [Bounded stack](#bounded-stack) |

## Context typing

Each mode has a set of contexts it may appear in. Root pages and `<game-mode>` define the
context; overlays inherit the context of the stack they are pushed onto. In a mode whose contexts
include NoGame, the following are errors: `interface__call(m)` of an `m` with `requires="game"`,
`metafn__save`, `metafn__revert`, `metafn__play_voice`, `<for-history>` and `<for-choices>`, for
example a Save button on the title page. The error is reported at the site, with a note at the
origin when the operation runs inside a system function: "`metafn__save` needs a running game,
but `title` can be shown without one".

## No trap

A reachable overlay must have a way out. A mode **resets** when it has an `interface__goto`,
`metafn__start`, `metafn__load` or `metafn__quit`, or when it calls an overlay that resets
(computed to a fixpoint). A reachable overlay with no `interface__return` that cannot reset is
reported at its name. This is the system's counterpart of the story's stuck-choice lint.

## Bounded stack

A pushdown machine may push without bound, and the system is one. A cycle in the push graph
(Config calls Help, Help calls Config) is a warning, reported once per cycle at its first call
site: "modes push each other in a cycle, so the stack can grow without bound: `config`, `help`".
Without such cycles, the stack depth is bounded by the longest push path, and the system is a
finite automaton.

## Isolation

A `.kirp` render or `<r>` block that names a system item, or system markup that names a story
variable, is an error in compilation stage 3. The only channels are the ones in
[the system model](./model.md#information-flow). The game cannot call system functions
(`kis/sysfn-call-in-game`).
