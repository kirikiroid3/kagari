# Function Kinds

Every function has one of three **kinds**. The kind is written before `fn`, and it is the only
place where the language restricts what code may do.

```kis
fn open_config() { nav::call(Config) }                 // Turing-complete, runtime
#[scene] pushdown fn Confession() -> Answer { … }      // the story
const fn fade_in(mut screen: Screen) -> Screen { … }   // pure, compile time
```

| Kind          | May use | Evaluated |
| ------------- | ------- | --------- |
| `fn`          | Everything except story variables | At runtime |
| `pushdown fn` | [Finite types](#finite-types) at runtime, story variables; steps only in a `#[scene]` | At runtime, inside the story |
| `const fn`    | Every type; no story variables, no settings values, no meta, no signals, no I/O | At compile time where needed; may also be called at runtime |

The compiler never infers a kind. A function is `const` only when it says so.

## Call rules

| Caller ↓ / callee → | `fn` | `pushdown fn` | `const fn` |
| ------------------- | ---- | ------------- | ---------- |
| `fn`                | yes  | no: it can only [start an entry scene](./entry.md) | yes |
| `pushdown fn`       | no   | yes | yes, see [below](#calling-const-fn-from-the-story) |
| `const fn`          | no   | no  | yes |

A call that breaks this table is `E3001`. Inside the story a further rule separates
[scenes from instant procedures](./scenes.md#scenes-and-instant-procedures): only a `#[scene]`
may run steps (`E3002`), call another scene (`E3003`) or draw a random value (`E3024`).

The rules hold through function pointers and closures too, because their types carry the kind: a
value of type `pushdown fn() -> R` can only point to a `pushdown fn`, so the story cannot reach
Turing-complete code by calling through a pointer (see
[Values, References and Closures](../syntax/values.md)).

The table per kind, as a summary:

| Kind | Calls | Holds at runtime | Steps | Story variables |
| ---- | ----- | ---------------- | ----- | --------------- |
| `fn` | `fn`, `const fn`; starts entry scenes | anything | no | no |
| `#[scene] pushdown fn` | scenes, instant procedures, `const fn` (constant or finite arguments) | finite types | yes | read and write |
| `pushdown fn` (instant procedure) | instant procedures, `const fn`; no recursion, only `for` loops | finite types | no | read and write |
| `const fn` | `const fn` | anything | no | no |

## `pushdown fn`

A `pushdown fn` is part of the story. It may:

- take, hold and return values of [finite types](#finite-types) only (`E3008`);
- read and write [story variables](./story.md#story-variables);
- call other `pushdown fn` and `const fn`.

A `pushdown fn` marked `#[scene]` is a **scene**. Only a scene may run [steps](./steps.md), which
suspend the story until the player gives input, and call other scenes, including itself. Every
loop and every cycle of calls in a scene must pass through a step (see
[Termination](./scenes.md#termination)). A `pushdown fn` without `#[scene]` is an **instant
procedure**: it never steps, never recurses and has only bounded loops, so it always finishes
without input. Either way, the code between two inputs always finishes.

Scenes call each other directly, like Kotlin's `suspend` functions: there is no `.await`. A step
anywhere below a call suspends the whole story, and input resumes it at the same point.

## `const fn`

A `const fn` is pure and deterministic: its result depends only on its arguments. It cannot read
story variables (`E3007`), settings values, meta (`E3013`) or signals, and it has no I/O. Inside,
it is Turing-complete: loops, recursion, Rust integers, `Rat`, `Decimal`, `String`, `Vec`.

The compiler evaluates a `const fn` when its result is needed at compile time: constants,
[render functions](./presentation.md), and calls from the story. Evaluation has a fixed budget of
1,000,000 evaluation steps (*tentative*: the budget may become a compiler setting); running out
is `E4003` at the call that needed the value. A `const fn` called from `fn` code at runtime is an
ordinary function call.

### Calling `const fn` from the story

A `pushdown fn` may call a `const fn` when every argument is either a
[constant expression](../syntax/constants.md) or of a finite type:

- **All arguments constant.** The compiler evaluates the call at compile time and replaces it by
  its value (**folding**). The value may be of any type. `fade(ms(500))` is such a call.
- **Some arguments finite and runtime.** The compiler **tabulates** the function: it evaluates it
  on every combination of argument values the call can receive. This is a compile-time proof that
  the call finishes without error and gives a finite result on every possible input; during play
  the function is still called normally with the actual arguments. The result must be of a finite
  type (`E3008`). A call with more than 65,536 argument combinations (*tentative*: may become a
  compiler setting) is `E4004`.

```kix
const fn mood(affection: Int<0, 5>, met: bool) -> Mood {
    match (affection, met) {
        (4..=5, true) => Mood::Warm,
        (_, true) => Mood::Polite,
        _ => Mood::Stranger,
    }
}

#[scene]
pushdown fn Greeting() {
    match mood(AFFECTION_MIO, MET_MIO) {   // tabulated: 6 × 2 entries
        Mood::Warm => s!(t!["\"There you are!\""]),
        Mood::Polite => s!(t!["\"Hello.\""]),
        Mood::Stranger => s!(t!["\"…Who are you?\""]),
    }
}
```

For you as an author this means: write helper logic for the story as an ordinary `const fn` with
small finite parameters such as `bool`, enums and `Int<lo, hi>`. An error in the function (an
overflow, a division by zero, a runaway loop) is reported at compile time for the argument values
that trigger it, never during play.

## `fn`

A plain `fn` is Turing-complete runtime code: the game system, its UI, saves, settings and meta.
It cannot call a `pushdown fn` and cannot read story variables. Its only way into the story is
[starting an entry scene](./entry.md), and its only view of a running story is the
[`Narrative` handle](./system.md).

## Finite types

The type of every local, parameter, return value and story variable of a `pushdown fn` must be
finite. A type is finite when it has finitely many values that the compiler can enumerate:

| Finite | Not finite |
| ------ | ---------- |
| `bool`, `()`, `!` | Rust integers (`u8`, `i32`, …), `Rat`, `Decimal` |
| `Int<lo, hi>` | `String`, `Vec<T>` |
| Enums whose variants carry only finite data | `Screen` and the other builtin handle types |
| Structs, tuples and arrays `[T; N]` of finite types | First-class closures |
| `Option<T>` of a finite `T` | Signals, settings and meta values |
| `Text` (its values are the constant texts of the program) | Types that contain themselves (recursive types) |
| File asset types `ImageAsset`, `AudioAsset`, `VoiceAsset`, `VideoAsset`, `FontAsset` (their values are the asset constants of the program) | `EntityId`, `Part`, `Mark`, `Duration`, `Color` (they hold Rust integers); shader types such as `ColorShader<A>` |
| `pushdown fn`, `scene fn` and `const fn` pointers (their values are the named functions of the program) | |
| `&T`, `&mut T` of a finite `T`, as parameters | |

A constant expression of any type may still appear in a `pushdown fn`, because it is folded at
compile time. The full list for standard library types is in the STD Reference,
[Compiler Rules for std Items](../../../../std/en/src/appendix/rules.md).

## Why this works

The engine needs two properties of the story: every step finishes, and questions such as "can
this step be reached with `MET_B` false?" can be answered by the compiler. Both hold for a
pushdown system over finite data, and the kinds make the story exactly that:

- **The story is closed.** A `pushdown fn` can only call `pushdown fn` and `const fn`, and a
  `const fn` called from the story is either folded to a constant or tabulated, that is, proven to
  finish with a finite result on every input it can receive.
- **The only door is the entry.** Turing-complete code enters the story only by starting an entry
  scene, with finite arguments that the save records and the analyses hold for. It gets back only
  the entry scene's return value, plus meta the story writes and never reads.
- **Frames stay finite.** A stack frame holds a return point and finite locals; references and
  closures in the story are second-class, so they never become part of the state.
- **Function pointers are finite.** There are finitely many named functions, so a pointer is a
  value of a finite set.

The system and the presentation may then compute freely: no story analysis depends on them. The
background is in [Philosophy: Analyzable by Construction](../overview/philosophy.md).
