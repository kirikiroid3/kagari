# Function Kinds

Every function has one of three kinds. The kind is written before `fn` and is the only place where
the language restricts what code may do.

```kis
fn open_config() { nav::call(Config) }                 // Turing-complete, runtime
pushdown fn Confession() -> Answer { … }               // the story
const fn fade_in(mut screen: Screen) -> Screen { … }   // pure, compile time
```

| Kind          | May use | Evaluated |
| ------------- | ------- | --------- |
| `fn`          | Everything except story variables | At runtime |
| `pushdown fn` | [Finite types](#finite-types) at runtime, story variables, steps | At runtime, inside the narrative |
| `const fn`    | Every type; no story variables, no settings values, no signals, no I/O | At compile time where needed; may also be called at runtime |

## Call rules

| Caller ↓ / callee → | `fn` | `pushdown fn` | `const fn` |
| ------------------- | ---- | ------------- | ---------- |
| `fn`                | yes  | no: it can only [start an entry scene](./narrative/scenes.md#entry-scenes) | yes |
| `pushdown fn`       | no   | yes | yes, see [below](#calling-const-fn-from-the-story) |
| `const fn`          | no   | no  | yes |

The rules hold through pointers and closures too, because their types carry the kind: a value of
type `pushdown fn() -> R` can only point to a `pushdown fn`, so the story cannot reach Turing-complete
code by calling through a pointer (see [Closure and pointer types](./values.md#closure-and-pointer-types)).

The compiler never infers a kind. A function is `const` only when it says so; a lint may suggest
`const` for a `fn` that qualifies.

## `pushdown fn`

A `pushdown fn` is part of the story. It may:

- take, hold and return values of [finite types](#finite-types) only;
- read and write [story variables](./narrative/overview.md#story-variables);
- run [steps](./narrative/elements.md), which suspend the story until the player gives input;
- call other `pushdown fn`, including itself, and `const fn`.

Every loop and every cycle of calls must pass through a step (see
[Termination](./narrative/scenes.md#termination)), so the code between two inputs always finishes.

Pushdown functions call each other directly, like Kotlin's `suspend` functions: there is no
`.await`. A step anywhere below a call suspends the whole story, and input resumes it at the same
point.

## `const fn`

A `const fn` is pure and deterministic: its result depends only on its arguments. It cannot read
story variables, settings values, meta or signals, and it has no I/O. Inside, it is
Turing-complete: loops, recursion, Rust integers, `Rat`, `Decimal`, `String`, `Vec`.

The compiler evaluates a `const fn` when its result is needed at compile time: constants,
[render functions](./presentation.md), and calls from the story. Evaluation has a fixed budget of
steps (*tentative*: the budget is a compiler setting); running out is a compile error at the call
that needed the value. A `const fn` called from `fn` code at runtime is an ordinary function call.

### Calling `const fn` from the story

A `pushdown fn` may call a `const fn` when every argument is either a constant expression or of a
finite type:

- **All arguments constant.** The call is evaluated at compile time and replaced by its value,
  which may be of any type. `fade(30)` is such a call.
- **Some arguments finite and runtime.** The compiler *tabulates* the function: it evaluates it on
  every combination of argument values. That proves the call terminates on every input it can
  receive, and turns it into a finite table the analyzer can read. The result must be of a finite
  type. A domain too large to tabulate (*tentative*: a compiler setting) is a compile error.

```kis
const fn mood(affection: Int<0, 5>, met: bool) -> Mood {
    match (affection, met) {
        (4..=5, true) => Mood::Warm,
        (_, true) => Mood::Polite,
        _ => Mood::Stranger,
    }
}

pushdown fn Greeting() {
    match mood(AFFECTION_MIO, MET_MIO) {   // tabulated: 6 × 2 entries
        Mood::Warm => <s>"There you are!"</s>,
        Mood::Polite => <s>"Hello."</s>,
        Mood::Stranger => <s>"…Who are you?"</s>,
    }
}
```

## `fn`

A plain `fn` is Turing-complete runtime code: the game system, its UI, saves, settings, and meta.
It cannot call a `pushdown fn` and cannot read story variables. Its only way into the story is
[starting an entry scene](./narrative/scenes.md#entry-scenes), and its only view of a running story
is the [`Narrative` interface](./system.md#the-narrative-interface).

## Finite types

The type of every local, parameter, return value and story variable of a `pushdown fn` must be
finite:

| Finite | Not finite |
| ------ | ---------- |
| `bool`, `()` | Rust integers, `Rat`, `Decimal` |
| `Int<lo, hi>` | `String`, `Vec<T>` |
| Enums whose variants carry only finite data | `Screen` |
| Structs, tuples and arrays `[T; N]` of finite types | First-class closures |
| `Option<T>` of a finite `T` | Signals, settings and meta values |
| `Text` (its values are the string literals of the program) | |
| Asset types (their values are the asset constants of the program) | |
| `pushdown fn` and `const fn` pointers (their values are the named functions of the program) | |
| `&T`, `&mut T` of a finite `T`, as parameters | |

A constant expression of any type may still appear in a `pushdown fn`, since it is folded at
compile time.

## Why this works

The engine proposal needs two properties of the story: every step terminates, and questions such
as "can this step be reached with `MET_B` false?" are decidable. Both hold for a pushdown system
over finite data, and the kinds make the story exactly that:

- **The story is closed.** A `pushdown fn` can only call `pushdown fn` and `const fn`, and a
  `const fn` called from the story is either a constant or a finite table. So everything that runs
  between the start of the story and any step is pushdown code over finite data.
- **The only door is the entry.** Turing-complete code enters the story only by starting an entry
  scene, with arguments of finite types that the save records and the analyzer quantifies over.
  It gets back only the entry scene's return value, plus meta the story writes and never reads.
- **Frames stay finite.** A stack frame holds a return point and finite locals; references and
  closures in the story are second-class, so they never become part of the state.
- **Function pointers are finite.** There are finitely many named functions, so a pointer is a
  value of a finite enum, the proposal's "computed call over a finite enum of scenes".

The system and the presentation may then compute freely: no narrative analysis depends on them.
