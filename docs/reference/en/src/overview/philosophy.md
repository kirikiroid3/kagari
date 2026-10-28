# Philosophy: Analyzable by Construction

Kagari is built so that the compiler can answer questions about a story with certainty, by
analysis rather than by playing it. This chapter explains why that is impossible for an ordinary
scripting language, how Kagari makes it possible, what a studio gets from it, and what it costs.

## The questions a studio asks

The correct behaviour of a visual novel lives in the author's head: which route a line belongs to,
which flags must hold at a scene, which sprite should be on screen after two branches merge. Bugs
in that knowledge are invisible to everyone else. Before shipping, a studio wants answers to
questions such as:

- Can the player reach this ending at all? Can they reach it without meeting Mio?
- Can the player arrive at a choice where no option is shown, and get stuck?
- Is there a scene or a line that no route ever reaches?
- After the roof route and the classroom route merge, does the next line show the background the
  author meant, on every path?
- Does the code between two clicks always finish?
- Will every save still load after a patch, and if not, where exactly does it stop matching?

Testing answers these questions only for the paths someone played. An answer for *every* path
needs an analysis of the program.

## Rice's theorem {#rices-theorem}

For a Turing-complete language, such an analysis cannot exist. **Rice's theorem** (1953) states
that every non-trivial semantic property of programs is undecidable. A *semantic* property is one
about what a program does, not how its text looks; *non-trivial* means that some programs have it
and some do not. For any such property, no algorithm can decide, for every program, whether the
program has it.

The questions above are all of this kind. Each of them can encode the halting problem: place the
ending after a computation that may or may not finish, and "is this ending reachable?" becomes
"does this computation halt?". "Does the code between two clicks finish?" *is* the halting
problem. So for an engine whose story may run arbitrary code, every tool must give up, guess, or
test a sample of paths. More engineering cannot fix this; it is a theorem.

## Undecidability comes from data, not from the stack {#data-not-stack}

Rice's theorem applies to Turing-complete programs. The way out is to write the story in a
language that is not Turing-complete, but still expressive enough for stories. The question is
which restriction to make.

<!-- diagram: philosophy-decidability (mermaid-ish)
quadrantChart-ish grid
  columns: "finite data" | "unbounded data"
  rows: "no call stack" | "call stack (recursion)"
  cell["no call stack, finite data"]: "Finite automaton: reachability decidable"
  cell["no call stack, unbounded data"]: "Two-counter machine (Minsky): already Turing-complete, reachability undecidable"
  cell["call stack, finite data"]: "Pushdown system: reachability decidable; the Kagari story (pushdown fn)" [highlighted]
  cell["call stack, unbounded data"]: "Turing-complete code: Rice's theorem applies; Kagari fn and const fn"
  boundary: "decidability boundary" between the finite-data and unbounded-data columns
-->
{{#include ../diagrams/philosophy-decidability.svg}}

- **A call stack alone is harmless.** A pushdown system, a finite-state machine with an unbounded
  call stack, has decidable reachability; it is computable in time polynomial in the size of the
  system. Scenes may call scenes, recurse, and return to wherever they were called from.
- **Unbounded data is the problem.** Two unbounded counters with conditional jumps already form a
  Minsky machine, which is Turing-complete without any stack. An integer that can grow without
  limit, a list, a string: each reintroduces such counters.

So Kagari restricts data, not control. The story may use recursion, loops, hub maps and shared
sub-scenes freely; every value it holds ranges over a finite set.

## How Kagari sidesteps the theorem {#how-kagari-sidesteps-the-theorem}

Kagari does not contradict Rice's theorem; it arranges the program so that the questions a studio
asks are only ever asked about the part where they are decidable, and so that the answers do not
depend on the rest. Three rules do this. The compiler checks all three; a violation is a compile
error, not a warning.

### The story is a pushdown system over finite data {#the-story-is-a-pushdown-system}

The story is written in [`pushdown fn`](../concepts/kinds.md), and inside it:

- Every local, parameter, return value and story variable has a
  [finite type](../concepts/kinds.md#finite-types): `bool`, `Int<lo, hi>`, enums, and structs,
  tuples and arrays of finite types. A stack frame is a return point plus finite locals, so the
  stack alphabet is finite: that is the definition of a pushdown system.
- A `pushdown fn` calls only `pushdown fn` and `const fn`. A `const fn` called from the story is
  either folded to a constant at compile time or, if its arguments are finite runtime values,
  *tabulated*: evaluated at compile time on every combination of arguments, which proves that it
  finishes without error and gives a finite result on every input the story can pass.
- Function pointers are finite values: there are finitely many named functions. Closures in the
  story are second-class (passed and called, never stored, returned or captured), so no captured
  environment ever becomes part of the state.
- Control flow is structured: there is no `goto`, so control leaves a function only by a call or a
  return.
- Every loop and every cycle of scene calls must pass through a step, and code that runs without
  input (instant procedures) neither recurses nor loops without a constant bound. So the code
  between two inputs always finishes.

On this model, reachability is decidable. The compiler explores the story exactly, from every
entry scene and every value of its arguments, and summarizes each scene per entry state, so a
scene called from many places in the same state is explored once.

### The presentation is pure and never read back {#the-presentation-is-pure}

What the player sees is computed by [render functions](../concepts/presentation.md), which are
`const fn`: Turing-complete, but pure and deterministic. A render receives only the screen before
its step and returns the screen after it. It cannot read story variables, and the story cannot
read the screen, because `Screen` is not a finite type. Presentation therefore has no influence on
which states the story reaches: the screens can be computed afterwards, over the states the story
analysis found. Because renders are pure, the compiler simply runs them, on every screen that can
reach their step.

### Turing-complete code enters only through entry scenes {#only-through-entry-scenes}

The game system is ordinary `fn` code. It cannot call a `pushdown fn` and cannot read a story
variable. Its only way into the story is to start an [entry scene](../concepts/entry.md), with
arguments of finite types; the analysis covers every value those arguments can take. Its only
ways back are the entry scene's return value, the ending, and meta that the story writes and never
reads. Whatever the system computes, the story behaves as one of the runs the compiler already
analyzed.

### Where limits remain {#where-limits-remain}

Two limits remain, and Kagari states them instead of hiding them:

- **Renders are Turing-complete.** Whether the set of screens a step can show stays finite is
  undecidable in general: a render that moves a character ten pixels further on each pass through
  a hub loop produces a new screen every time. *Tentative:* the compiler computes the sets of
  screens as a fixpoint and reports a render that makes a step's set grow past a cap, currently 64
  screens (`E5005`). The rule is sound, since an accepted program has exactly the computed sets,
  but it may reject a render that is bounded only above the cap. Every `const fn` evaluation also
  has a fixed budget, and running out of it is a compile error (`E4003`).
- **Decidable is not the same as cheap.** The number of states grows exponentially with the number
  of independent story variables. The analysis stops at a state cap (default 200 000,
  configurable in the [project](../tools/projects.md)) with the warning `W5006`; what it found is
  still reported, but the checks that need every state are skipped.

## What studios get

Every guarantee below holds for every run of the game: every entry scene, every value of its
arguments and every sequence of choices. Each is checked by `kagari check` and `kagari build`; the
[diagnostics](../diagnostics/index.md) chapters give the details.

| Guarantee | What the compiler checks | Diagnostics |
| --------- | ------------------------ | ----------- |
| **Termination between steps** | Every loop and every cycle of scene calls passes through a step; instant procedures and story calls of `const fn` finish | `E5001`, `E5002`, `E3004`, `E3005`, `E4003` |
| **No stuck choices** | No `choice!` is reachable with every arm's guard false | `E5003` |
| **No dead story** | Scenes no entry scene reaches, steps no state reaches, and story variables that never influence the story are reported | `W5008`, `W5009`, `W5010` |
| **No run-time errors in the story** | Errors such as an index out of range are found where the story would run into them | [Constant evaluation](../diagnostics/eval.md) |
| **Consistent screens** | The screen a step shows depends only on the story state, never on which path reached it | `W5004` |
| **Bounded screens** | No render makes the set of screens at a step grow without bound | `E5005` |
| **Sound game system** | Every mode can be shown, no overlay traps the player, no base page goes back, the mode stack stays bounded, and every ending is handled | `W6001`, `E6002`, `E6003`, `W6004`, `E2037` |

Beyond the compile-time checks, the model gives the runtime properties that ordinary engines
cannot offer:

- **Robust saves.** A [save](../concepts/saves.md) records the trace, not the state: the entry
  scene and its arguments, the stable ID of every choice, the number of steps since the last
  choice, and the random seed. Loading replays the trace. So a save is a *witness* that its
  position is reachable; no save of an unreachable position can exist. When the script changes,
  the save still loads if the trace still makes sense, and otherwise the load stops at the first
  choice that no longer resolves and says which one.
- **Deterministic replay.** Kagari has no floating-point numbers; story state uses finite types,
  and other code uses exact integers, `Rat` and `Decimal`. Renders are pure. Random draws come from
  a seeded generator whose seed is in the save, and the analysis treats each draw as a branch over
  every value it can take. The same trace therefore produces the same story state and the same
  screen on every machine.
- **Fast jumps and preview.** Because the story state and the logical screen are functions of the
  trace, any position can be rebuilt by replaying the story; load and backlog rewind already work
  this way. Presentation need not be replayed from the start: renders before the last
  [keyframe](../concepts/presentation.md) on the path cannot affect the screen. *Tentative:* live
  preview at the editor's cursor, which the same model is designed for, is not part of the
  toolchain yet.

## What studios give up

The guarantees have a price, paid only by the story:

- **Finite data in the story.** No Rust integers, `String`, `Vec`, `Rat` or `Decimal` in a
  `pushdown fn`. Counters are bounded `Int<lo, hi>` values, such as an affection score
  `Int<0, 5>`, and compound assignment saturates at the bounds.
- **No outside world in the story.** The story cannot call `fn`, so it cannot read the clock, the
  file system, the screen, settings or meta during a run. Meta reaches it only as entry arguments.
- **Restricted control.** Loops and recursion in a scene must pass through a step; instant
  procedures loop only over constant ranges and arrays and never recurse. There is no `goto`.
- **Second-class closures in the story.** A closure can be passed to a `pushdown fn` and called,
  but not stored, returned or captured, and it cannot step.
- **One-way presentation.** If what a step shows must depend on how the story got there, that
  dependency must be a story variable that chooses the render, not something read from the screen.
- **Bounded analysis budgets.** Tabulated calls have a maximum argument domain, renders a maximum
  number of screens per step, and the story a state cap. Very large stories may need the cap
  raised.
- **Language features that would weaken the guarantees everywhere.** There are no floating-point
  numbers, no user-defined traits (only the standard library's fixed set; generics are allowed),
  and no macros: [forms](../syntax/forms.md) are a fixed part of the syntax.

## Why the rest of the game stays free

None of these restrictions apply outside the story. The game system is `fn` code with the whole
language: Rust integers, `String`, `Vec`, first-class closures, signals, recursion, unbounded
loops. No analysis quantifies over its values; its checks are type checks and checks of the mode
graph, which the compiler collects from the finitely many handlers written in the source. A
handler may even loop forever; no guarantee of this chapter depends on it, because the system can
affect the story only through the single entry.

Presentation, too, is Turing-complete: a `const fn` may loop, recurse and compute with exact
numbers. Because it is pure and its inputs are known when the compiler needs it, the compiler does
not have to reason about it in general; it runs it.

The result is a division of labour. The part of the game whose correctness lives in the author's
head, the story, is the part the compiler can check completely. Everything else is ordinary
programming.
