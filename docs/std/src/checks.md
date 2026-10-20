# System Checks

The system scope is checked by different rules than the story. The story's rules exist to keep
narrative analysis decidable and presentation consistent. The system has neither concern: its
state is a finite mode graph plus values no analysis quantifies over. It has others: every mode
must be reachable in the right context, and the player must never be trapped.

| Check                          | Game scope                         | System scope |
| ------------------------------ | ---------------------------------- | ------------ |
| Finite data                    | required, for decidability         | not required for settings; bounded types suggested |
| Every loop passes through a step | required                         | not applicable: handlers are finite sequences of operations, with no loops |
| Consistency, bounded screens   | required                           | not applicable: views are full |
| Reachability, dead code        | pushdown reachability              | reachability on the finite mode graph |
| Context typing                 | not applicable                     | `push(m)` only where `m.requires ⊆ ctx` |
| No `pop` from a base           | not applicable                     | required |
| No trap                        | stuck-choice lint                  | every reachable mode reaches a root page or `quit` |
| Bounded stack                  | not required                       | lint: the push graph is acyclic, so the stack depth is bounded |
| Exhaustive endings             | not applicable                     | `<on-exit>` covers every variant of `Ending` |
| Isolation                      | no system names except `std/system-ctx.kix` | no story names except `std/narrative.kix` and entry scenes |
| Operation arguments            | not applicable                     | `set` values in range, `start` names an entry scene, slot indices below `count` |

## Context typing

Each mode has a set of contexts it may appear in. Root pages and `<game-mode>` define the
context; overlays inherit the context of the stack they are pushed onto. The compiler propagates
contexts along `push` edges from every base and reports an overlay whose `requires` is not met on
some path, for example a Save button on the title page.

## No trap

From every reachable mode, some sequence of operations reaches a root page or `quit`. A page with
no way out other than closing the window is reported. This is the system's counterpart of the
story's stuck-choice lint.

## Bounded stack

A pushdown machine may push without bound, and the system is one. *Tentative:* a cycle in the
push graph (Config pushes Help, Help pushes Config) is a warning, because the player can grow the
stack without limit. Without such cycles, the stack depth is bounded by the longest push path,
and the system is a finite automaton.

## Isolation

A `.kirp` render or `<r>` block that names a system item, or system markup that names a story
variable, is an error in compilation stage 3. The only channels are the ones in
[the system model](./model.md#information-flow).
